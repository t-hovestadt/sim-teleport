use lz4_flex::block::decompress_into;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::game::{self, GameConfig};
use crate::maps::{MapError, SharedMap};
use crate::platform::{
    boost_thread_priority, pin_thread_to_core, set_high_priority, HighResTimer, MmcssGuard,
};
use crate::protocol::{Receiver as ProtoReceiver, MAX_DATAGRAM_SIZE, PAGE_HEARTBEAT};
use crate::stats::Stats;

/// Target maps created at this size for each page regardless of the actual game struct size.
/// 64 KB is generous for both current games and any future struct growth.
const DUAL_MAP_SIZE: usize = 65536;

pub struct TargetArgs {
    /// `None` = dual mode (creates maps for both EVO and AC1 simultaneously).
    /// `Some(cfg)` = single-game mode (creates only that game's 3 maps, lazily).
    pub game: Option<&'static GameConfig>,
    pub bind: String,
    pub group: String,
    pub unicast: bool,
    pub busy_wait: bool,
    pub pin_core: Option<usize>,
    pub high_priority: bool,
    pub stale_timeout: Duration,
    pub on_first_data: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_stale: Option<Arc<dyn Fn() + Send + Sync>>,
}

// ── Map abstractions ──────────────────────────────────────────────────────────

/// One game's three shared memory maps.
struct GameMapSet {
    maps: [SharedMap; 3],
}

impl GameMapSet {
    fn create(game: &GameConfig, size: usize) -> Result<Self, MapError> {
        let maps = [
            SharedMap::create(game.physics_map, size)?,
            SharedMap::create(game.graphics_map, size)?,
            SharedMap::create(game.static_map, size)?,
        ];
        Ok(Self { maps })
    }

    fn write_page(&mut self, page_idx: usize, data: &[u8]) {
        if page_idx >= self.maps.len() {
            return;
        }
        let map = &mut self.maps[page_idx];
        let copy_len = data.len().min(map.size());
        map.as_slice_mut()[..copy_len].copy_from_slice(&data[..copy_len]);
    }

    /// Zero all three map pages so FanaLab reads RPM=0 and resets wheel LEDs.
    fn zero_all(&mut self) {
        for map in self.maps.iter_mut() {
            map.as_slice_mut().fill(0);
        }
    }

    fn page_size(&self, page_idx: usize) -> usize {
        self.maps[page_idx].size()
    }
}

/// Either one game's maps (lazy, dropped on stale) or both games' maps (eager, zeroed on stale).
enum MapMode {
    /// Single-game: maps created lazily on first data arrival; dropped on stale timeout.
    Single(GameMapSet),
    /// Dual-game: maps for both EVO and AC1 created at startup; status zeroed on stale.
    Dual { evo: GameMapSet, ac1: GameMapSet },
}

impl MapMode {
    fn write_page(&mut self, page_idx: usize, data: &[u8]) {
        match self {
            Self::Single(set) => set.write_page(page_idx, data),
            Self::Dual { evo, ac1 } => {
                evo.write_page(page_idx, data);
                ac1.write_page(page_idx, data);
            }
        }
    }

    fn zero_all_pages(&mut self) {
        match self {
            Self::Single(set) => set.zero_all(),
            Self::Dual { evo, ac1 } => {
                evo.zero_all();
                ac1.zero_all();
            }
        }
    }

    fn page_size(&self, page_idx: usize) -> usize {
        match self {
            Self::Single(set) => set.page_size(page_idx),
            Self::Dual { evo, .. } => evo.page_size(page_idx),
        }
    }
}

// ── Main run ──────────────────────────────────────────────────────────────────

pub fn run(args: TargetArgs, shutdown: mpsc::Receiver<()>) -> std::io::Result<()> {
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
        socket.set_nonblocking(true)?;
        println!("Busy-wait mode: target thread will burn one CPU core for lower latency.");
    } else {
        socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    }

    if !args.unicast {
        let group: Ipv4Addr = args
            .group
            .parse()
            .map_err(|e| std::io::Error::other(format!("bad multicast address: {e}")))?;
        socket.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;
        println!("Joined multicast group {group}");
    }

    // In dual mode, create all six maps eagerly so SimHub can connect before data arrives.
    // In single-game mode, create lazily on first data arrival (existing behavior).
    let mut maps: Option<MapMode> =
        if args.game.is_none() {
            let evo = GameMapSet::create(&game::EVO, DUAL_MAP_SIZE)
                .map_err(|e| std::io::Error::other(format!("failed to create EVO maps: {e}")))?;
            let ac1 = GameMapSet::create(&game::AC1, DUAL_MAP_SIZE)
                .map_err(|e| std::io::Error::other(format!("failed to create AC1 maps: {e}")))?;
            println!(
                "Created shared memory maps for {} and {}",
                game::EVO.name,
                game::AC1.name
            );
            println!(
            "  {} ({DUAL_MAP_SIZE} bytes), {} ({DUAL_MAP_SIZE} bytes), {} ({DUAL_MAP_SIZE} bytes)",
            game::EVO.physics_map, game::EVO.graphics_map, game::EVO.static_map
        );
            println!(
            "  {} ({DUAL_MAP_SIZE} bytes), {} ({DUAL_MAP_SIZE} bytes), {} ({DUAL_MAP_SIZE} bytes)",
            game::AC1.physics_map, game::AC1.graphics_map, game::AC1.static_map
        );
            Some(MapMode::Dual { evo, ac1 })
        } else {
            None
        };

    println!("Listening on {}", args.bind);

    // Decompression buffer sized to the largest possible page across both games.
    let max_page = game::EVO
        .max_physics_size
        .max(game::EVO.max_graphics_size)
        .max(game::EVO.max_static_size)
        .max(game::AC1.max_physics_size)
        .max(game::AC1.max_graphics_size)
        .max(game::AC1.max_static_size)
        .max(DUAL_MAP_SIZE);
    let mut decomp_buf = vec![0u8; max_page];

    let mut recv_buf = [0u8; MAX_DATAGRAM_SIZE];
    let mut proto = ProtoReceiver::new(max_page);
    let mut last_update = Instant::now();
    let mut stats = Stats::new("target");
    let mut seq_start: Option<Instant> = None;
    let mut first_frame_logged = false;

    loop {
        if shutdown.try_recv().is_ok() {
            stats.print_summary();
            return Ok(());
        }

        match socket.recv_from(&mut recv_buf) {
            Ok((len, _src)) => {
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
                    continue;
                }

                let compressed_len = if let Some(compressed) = assembled {
                    // Single-game mode: lazily create maps on first data arrival.
                    if maps.is_none() {
                        let game = match args.game {
                            Some(g) => g,
                            None => {
                                eprintln!("error: no game configured for single-game mode");
                                continue;
                            }
                        };
                        match GameMapSet::create(game, DUAL_MAP_SIZE) {
                            Ok(set) => {
                                println!("Created shared memory maps for {}.", game.name);
                                maps = Some(MapMode::Single(set));
                            }
                            Err(e) => {
                                return Err(std::io::Error::other(format!(
                                    "failed to create maps: {e}"
                                )));
                            }
                        }
                    }

                    let Some(map_mode) = maps.as_mut() else {
                        continue;
                    };
                    let map_size = map_mode.page_size(page_idx);
                    let compressed_len = compressed.len();

                    match decompress_into(compressed, &mut decomp_buf[..max_page]) {
                        Ok(n) => {
                            if n > map_size {
                                eprintln!(
                                    "warn: decompressed page {page_idx} is {n} bytes \
                                     but map is {map_size} bytes — truncating"
                                );
                            }
                            let write_len = n.min(map_size);
                            map_mode.write_page(page_idx, &decomp_buf[..write_len]);
                            Some((compressed_len, n))
                        }
                        Err(e) => {
                            eprintln!("decompression failed (page {page_idx}): {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                if let Some((clen, decomp_len)) = compressed_len {
                    if !first_frame_logged {
                        println!("[AC Teleport] First frame received ({decomp_len} bytes)");
                        first_frame_logged = true;
                        if let Some(cb) = &args.on_first_data {
                            cb();
                        }
                    }
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
                if last_update.elapsed() >= args.stale_timeout {
                    // FanaLab workaround: zero all map data so FanaLab reads RPM=0
                    // and resets wheel base LEDs. Without this, stale RPM keeps LEDs lit.
                    // See: https://forum.fanatec.com/topic/19449
                    if let Some(mode) = maps.as_mut() {
                        mode.zero_all_pages();
                    }
                    match maps.as_mut() {
                        None => {}
                        Some(MapMode::Single(_)) => {
                            println!(
                                "No data for {}s — closing shared memory maps.",
                                args.stale_timeout.as_secs()
                            );
                            maps = None;
                            first_frame_logged = false;
                            if let Some(cb) = &args.on_stale {
                                cb();
                            }
                        }
                        Some(MapMode::Dual { .. }) => {
                            // Dual mode: keep maps alive (already zeroed above).
                            first_frame_logged = false;
                            if let Some(cb) = &args.on_stale {
                                cb();
                            }
                            // Reset so we don't re-zero on every subsequent timeout tick.
                            last_update = Instant::now();
                        }
                    }
                }
            }

            Err(e) => return Err(e),
        }
    }
}

/// Read `buf_offset` (u32 LE) from byte offset 16 of a raw protocol header.
/// Returns PAGE_HEARTBEAT on underflow so the caller can safely ignore the datagram.
fn peek_buf_offset(datagram: &[u8]) -> u32 {
    if datagram.len() < 20 {
        return PAGE_HEARTBEAT;
    }
    u32::from_le_bytes(datagram[16..20].try_into().unwrap())
}
