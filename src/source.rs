use lz4_flex::block::{compress_into, get_maximum_output_size};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::game::GameConfig;
use crate::maps::{MapError, SharedMap};
use crate::platform::{boost_thread_priority, pin_thread_to_core, set_high_priority, HighResTimer};
use crate::protocol::{Sender, PAGE_GRAPHICS, PAGE_HEARTBEAT, PAGE_PHYSICS, PAGE_STATIC};
use crate::stats::Stats;

const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
const STATIC_RESEND_INTERVAL: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const STATIC_CHANGE_BYTES: usize = 100;

pub struct SourceArgs {
    pub target: String,
    pub bind: String,
    pub unicast: bool,
    pub busy_wait: bool,
    pub pin_core: Option<usize>,
    pub high_priority: bool,
    pub poll_rate: u32,
}

pub fn run(
    game: &'static GameConfig,
    args: SourceArgs,
    shutdown: mpsc::Receiver<()>,
) -> std::io::Result<()> {
    // Always boost thread to ABOVE_NORMAL; only raise process class when --high-priority.
    let _timer = HighResTimer::acquire();
    boost_thread_priority();
    if args.high_priority {
        set_high_priority();
    }
    if let Some(core) = args.pin_core {
        pin_thread_to_core(core);
    }
    // NOTE: MmcssGuard intentionally omitted on source. On the game PC the game
    // holds its own MMCSS registrations; a competing registration risks micro-stutters.
    // MMCSS belongs on the target side only.

    // Build UDP socket with a generous send buffer.
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_send_buffer_size(2 * 1024 * 1024)?;
    let bind_addr: SocketAddr = args
        .bind
        .parse()
        .map_err(|e| std::io::Error::other(format!("invalid bind address: {e}")))?;
    sock.bind(&bind_addr.into())?;
    let socket: UdpSocket = sock.into();
    let target_addr: SocketAddr = args
        .target
        .parse()
        .map_err(|e| std::io::Error::other(format!("invalid target address: {e}")))?;
    if args.unicast {
        socket.connect(target_addr)?;
    }

    // Pre-allocate LZ4 compression buffers — no heap allocation on the hot path.
    let mut physics_cbuf = vec![0u8; get_maximum_output_size(game.max_physics_size)];
    let mut graphics_cbuf = vec![0u8; get_maximum_output_size(game.max_graphics_size)];
    let mut static_cbuf = vec![0u8; get_maximum_output_size(game.max_static_size)];
    let mut static_snapshot = vec![0u8; game.max_static_size];

    macro_rules! send_datagram {
        ($d:expr) => {
            if args.unicast {
                socket.send($d).map(|_| ())
            } else {
                socket.send_to($d, target_addr).map(|_| ())
            }
        };
    }

    println!("Waiting for {} to start...", game.name);
    let mut maps = loop {
        match try_open_maps(game) {
            Ok(m) => {
                println!("Connected to {} shared memory.", game.name);
                break m;
            }
            Err(_) => match shutdown.recv_timeout(RECONNECT_INTERVAL) {
                Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            },
        }
    };

    let mut sender = Sender::new();
    let mut stats = Stats::new("source");

    let mut last_physics_id: i32 = 0;
    let mut last_graphics_id: i32 = 0;
    let mut last_status: i32 = 0;
    let mut static_last_send = Instant::now()
        .checked_sub(STATIC_RESEND_INTERVAL)
        .unwrap_or(Instant::now());
    let mut heartbeat_timer = Instant::now();
    let mut last_nonzero_tick = Instant::now();

    let tick = Duration::from_micros(1_000_000 / args.poll_rate.max(1) as u64);
    let mut next_tick = Instant::now();

    loop {
        if shutdown.try_recv().is_ok() {
            stats.print_summary();
            return Ok(());
        }

        let tick_start = Instant::now();

        // ── Physics ──────────────────────────────────────────────────────────
        let phys_size = maps[0].size();
        let phys_id = read_i32(maps[0].as_slice(), 0);
        if phys_id != last_physics_id {
            match compress_into(maps[0].as_slice(), &mut physics_cbuf) {
                Ok(n) => {
                    let source_us = tick_start.elapsed().as_micros() as u64;
                    let result = sender.send(&physics_cbuf[..n], source_us, PAGE_PHYSICS, |d| {
                        send_datagram!(d)
                    });
                    match result {
                        Ok(frags) => stats.record(n, frags, source_us, phys_size),
                        Err(e) => eprintln!("send failed (physics): {e}"),
                    }
                }
                Err(e) => eprintln!("compression failed (physics): {e}"),
            }
            last_physics_id = phys_id;
        }

        // ── Graphics ─────────────────────────────────────────────────────────
        let gfx_size = maps[1].size();
        let gfx_id = read_i32(maps[1].as_slice(), 0);
        if gfx_id != last_graphics_id {
            match compress_into(maps[1].as_slice(), &mut graphics_cbuf) {
                Ok(n) => {
                    let source_us = tick_start.elapsed().as_micros() as u64;
                    let result = sender.send(&graphics_cbuf[..n], source_us, PAGE_GRAPHICS, |d| {
                        send_datagram!(d)
                    });
                    match result {
                        Ok(frags) => stats.record(n, frags, source_us, gfx_size),
                        Err(e) => eprintln!("send failed (graphics): {e}"),
                    }
                }
                Err(e) => eprintln!("compression failed (graphics): {e}"),
            }
            last_graphics_id = gfx_id;
            // AC_STATUS is i32 at byte offset 4 (second field after packetId).
            last_status = read_i32(maps[1].as_slice(), 4);
        }

        // Update the nonzero-tick tracker; used to detect game closure below.
        if phys_id != 0 || gfx_id != 0 {
            last_nonzero_tick = Instant::now();
        }

        // ── Static ───────────────────────────────────────────────────────────
        {
            let st_slice = maps[2].as_slice();
            let cmp_len = STATIC_CHANGE_BYTES.min(st_slice.len());
            let static_changed = st_slice[..cmp_len] != static_snapshot[..cmp_len];
            if static_changed || static_last_send.elapsed() >= STATIC_RESEND_INTERVAL {
                match compress_into(st_slice, &mut static_cbuf) {
                    Ok(n) => {
                        let source_us = tick_start.elapsed().as_micros() as u64;
                        let result = sender.send(&static_cbuf[..n], source_us, PAGE_STATIC, |d| {
                            send_datagram!(d)
                        });
                        match result {
                            Ok(frags) => stats.record(n, frags, source_us, st_slice.len()),
                            Err(e) => eprintln!("send failed (static): {e}"),
                        }
                    }
                    Err(e) => eprintln!("compression failed (static): {e}"),
                }
                let copy_len = st_slice.len().min(static_snapshot.len());
                static_snapshot[..copy_len].copy_from_slice(&st_slice[..copy_len]);
                static_last_send = Instant::now();
            }
        }

        // ── Heartbeat (when game is at menu / AC_OFF == 0) ───────────────────
        if last_status == 0 && heartbeat_timer.elapsed() >= HEARTBEAT_INTERVAL {
            let source_us = tick_start.elapsed().as_micros() as u64;
            let _ = sender.send_heartbeat(source_us, |d| send_datagram!(d));
            heartbeat_timer = Instant::now();
        }

        // ── Reconnect detection ───────────────────────────────────────────────
        // If all packetIds have been zero for RECONNECT_INTERVAL, the game has
        // likely closed. Drop the maps and re-enter the wait loop.
        if last_nonzero_tick.elapsed() >= RECONNECT_INTERVAL {
            drop(maps);
            println!(
                "No telemetry for {}s — waiting for {} to restart...",
                RECONNECT_INTERVAL.as_secs(),
                game.name
            );
            stats.print_summary();
            stats = Stats::new("source");
            maps = loop {
                match try_open_maps(game) {
                    Ok(m) => {
                        println!("Reconnected to {} shared memory.", game.name);
                        break m;
                    }
                    Err(_) => match shutdown.recv_timeout(RECONNECT_INTERVAL) {
                        Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    },
                }
            };
            last_physics_id = 0;
            last_graphics_id = 0;
            last_status = 0;
            last_nonzero_tick = Instant::now();
            static_last_send = Instant::now()
                .checked_sub(STATIC_RESEND_INTERVAL)
                .unwrap_or(Instant::now());
            next_tick = Instant::now();
            continue;
        }

        stats.maybe_print();

        // ── Tick timing ──────────────────────────────────────────────────────
        next_tick += tick;
        if args.busy_wait {
            while Instant::now() < next_tick {
                std::hint::spin_loop();
            }
        } else {
            let now = Instant::now();
            if next_tick > now {
                std::thread::sleep(next_tick - now);
            } else {
                next_tick = now; // fell behind — skip ahead
            }
        }
    }
}

/// Open all three shared memory maps for the given game.
/// Returns `Err(MapError::Unavailable)` if any map is missing.
fn try_open_maps(game: &GameConfig) -> Result<[SharedMap; 3], MapError> {
    let physics = SharedMap::open(game.physics_map)?;
    let graphics = SharedMap::open(game.graphics_map)?;
    let static_ = SharedMap::open(game.static_map)?;
    Ok([physics, graphics, static_])
}

/// Read a little-endian i32 from `slice` at `offset`. Returns 0 on underflow.
fn read_i32(slice: &[u8], offset: usize) -> i32 {
    if offset + 4 > slice.len() {
        return 0;
    }
    i32::from_le_bytes(slice[offset..offset + 4].try_into().unwrap())
}

// Suppress unused import warning for PAGE_HEARTBEAT when not used directly.
const _: u32 = PAGE_HEARTBEAT;
