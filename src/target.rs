use lz4_flex::block::decompress_into;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::game::GameConfig;
use crate::maps::{MapError, SharedMap};
use crate::platform::{
    boost_thread_priority, pin_thread_to_core, set_high_priority, HighResTimer, MmcssGuard,
};
use crate::protocol::{Receiver as ProtoReceiver, MAX_DATAGRAM_SIZE, PAGE_HEARTBEAT};
use crate::stats::Stats;

pub struct TargetArgs {
    pub bind: String,
    pub group: String,
    pub unicast: bool,
    pub busy_wait: bool,
    pub pin_core: Option<usize>,
    pub high_priority: bool,
    pub stale_timeout: Duration,
}

pub fn run(
    game: &'static GameConfig,
    args: TargetArgs,
    shutdown: mpsc::Receiver<()>,
) -> std::io::Result<()> {
    let _timer = HighResTimer::acquire();
    boost_thread_priority();
    if args.high_priority {
        set_high_priority();
    }
    // MMCSS on target: provides reserved CPU time and lower scheduling jitter.
    // Applied here (not source) to avoid competing with the game's own registrations.
    let _mmcss = MmcssGuard::acquire();
    if let Some(core) = args.pin_core {
        pin_thread_to_core(core);
    }

    // Build UDP socket with a generous receive buffer.
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_recv_buffer_size(2 * 1024 * 1024)?;
    sock.set_reuse_address(true)?;
    let bind_addr: SocketAddr = args
        .bind
        .parse()
        .map_err(|e| std::io::Error::other(format!("invalid bind address: {e}")))?;
    sock.bind(&bind_addr.into())?;
    let socket: UdpSocket = sock.into();

    if args.busy_wait {
        // Spin on recv_from. Burns one core but cuts OS scheduler wake-up jitter.
        socket.set_nonblocking(true)?;
        println!("Busy-wait mode: target thread will burn one CPU core for lower latency.");
    } else {
        socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    }

    println!("Listening on {}", args.bind);

    if !args.unicast {
        let group: Ipv4Addr = args
            .group
            .parse()
            .map_err(|e| std::io::Error::other(format!("bad multicast address: {e}")))?;
        socket.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;
        println!("Joined multicast group {group}");
    }

    // Pre-allocate a single decompression buffer large enough for any page.
    let max_page = game
        .max_physics_size
        .max(game.max_graphics_size)
        .max(game.max_static_size);
    let mut decomp_buf = vec![0u8; max_page];

    let mut recv_buf = [0u8; MAX_DATAGRAM_SIZE];
    let mut proto = ProtoReceiver::new(max_page);
    let mut maps: Option<[SharedMap; 3]> = None;
    let mut last_update = Instant::now();
    let mut stats = Stats::new("target");
    let mut seq_start: Option<Instant> = None;

    loop {
        if shutdown.try_recv().is_ok() {
            stats.print_summary();
            return Ok(());
        }

        match socket.recv_from(&mut recv_buf) {
            Ok((len, _src)) => {
                // Peek buf_offset directly from the raw header bytes (offset 16..20)
                // before calling ingest(), so we can read it without holding the
                // borrow that ingest() places on `proto` via the returned &[u8].
                let buf_offset = peek_buf_offset(&recv_buf[..len]);

                let (assembled, new_seq) = proto.ingest(&recv_buf[..len]);

                if new_seq {
                    seq_start = Some(Instant::now());
                }

                // Heartbeat: reset stale timer, no decompression needed.
                if buf_offset == PAGE_HEARTBEAT {
                    last_update = Instant::now();
                    continue;
                }

                let page_idx = buf_offset as usize;
                if page_idx > 2 {
                    continue; // unknown page type
                }

                let compressed_len = if let Some(compressed) = assembled {
                    // Lazily create all three maps on first data arrival.
                    if maps.is_none() {
                        match create_all_maps(game) {
                            Ok(m) => {
                                println!("Created shared memory maps for {}.", game.name);
                                maps = Some(m);
                            }
                            Err(e) => {
                                return Err(std::io::Error::other(format!(
                                    "failed to create maps: {e}"
                                )));
                            }
                        }
                    }

                    let map = &mut maps.as_mut().unwrap()[page_idx];
                    let map_size = map.size();
                    let target_slice = map.as_slice_mut();

                    // Decompress into the staging buffer, then copy into the map.
                    let compressed_len = compressed.len();
                    match decompress_into(compressed, &mut decomp_buf[..max_page]) {
                        Ok(n) => {
                            let copy_len = n.min(map_size);
                            target_slice[..copy_len].copy_from_slice(&decomp_buf[..copy_len]);
                            Some((compressed_len, n))
                        }
                        Err(e) => {
                            eprintln!("decompression failed (page {page_idx}): {e}");
                            None
                        }
                    }
                    // `compressed` drops here, releasing the borrow on `proto`.
                } else {
                    None
                };

                // `proto` borrow is now released — safe to read its fields.
                if let Some((clen, decomp_len)) = compressed_len {
                    if let Some(start) = seq_start.take() {
                        let transit_us = start.elapsed().as_micros() as u64;
                        stats.record(
                            clen,
                            proto.last_fragment_count,
                            proto.last_source_us + transit_us,
                            decomp_len,
                        );
                    }
                    last_update = Instant::now();
                    stats.maybe_print();
                }
            }

            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if maps.is_some() && last_update.elapsed() >= args.stale_timeout {
                    println!(
                        "No data for {}s — closing {} maps.",
                        args.stale_timeout.as_secs(),
                        game.name
                    );
                    maps = None;
                }
            }

            Err(e) => return Err(e),
        }
    }
}

fn create_all_maps(game: &GameConfig) -> Result<[SharedMap; 3], MapError> {
    let physics = SharedMap::create(game.physics_map, game.max_physics_size)?;
    let graphics = SharedMap::create(game.graphics_map, game.max_graphics_size)?;
    let static_ = SharedMap::create(game.static_map, game.max_static_size)?;
    Ok([physics, graphics, static_])
}

/// Read `buf_offset` (u32 LE) from byte offset 16 of a raw protocol header.
/// Returns PAGE_HEARTBEAT on underflow so the caller can safely ignore the datagram.
fn peek_buf_offset(datagram: &[u8]) -> u32 {
    if datagram.len() < 20 {
        return PAGE_HEARTBEAT;
    }
    u32::from_le_bytes(datagram[16..20].try_into().unwrap())
}
