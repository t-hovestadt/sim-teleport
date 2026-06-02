use lz4_flex::block::decompress_into;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::game::{self, GameConfig};
use crate::maps::SharedMap;
use crate::platform::{
    boost_thread_priority, pin_thread_to_core, set_high_priority, HighResTimer, MmcssGuard,
};
use crate::protocol::{
    Receiver as ProtoReceiver, GAME_ID_AC1, GAME_ID_ACC, GAME_ID_EVO, MAX_DATAGRAM_SIZE,
    PAGE_GAME_ANNOUNCE, PAGE_HEARTBEAT,
};
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
    pub on_game_announce: Option<Arc<dyn Fn(u8) + Send + Sync>>,
}

// ── Map abstractions ──────────────────────────────────────────────────────────

/// Try to create or open one shared memory map.
/// Logs a warning and returns `None` if both strategies fail so the caller can
/// continue with the remaining maps rather than aborting the whole target.
fn try_create_map(name: &str, size: usize) -> Option<SharedMap> {
    match SharedMap::create(name, size) {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!("[AC Teleport] warning: could not create/open map {name}: {e}");
            None
        }
    }
}

/// One game's three shared memory maps (physics / graphics / static).
/// Each slot is `None` when map creation failed (logged at startup); writes to
/// that slot are silently skipped so the other two maps continue working.
struct GameMapSet {
    maps: [Option<SharedMap>; 3],
}

impl GameMapSet {
    fn create(game: &GameConfig, size: usize) -> Self {
        Self {
            maps: [
                try_create_map(game.physics_map, size),
                try_create_map(game.graphics_map, size),
                try_create_map(game.static_map, size),
            ],
        }
    }

    fn write_page(&mut self, page_idx: usize, data: &[u8]) {
        if let Some(Some(map)) = self.maps.get_mut(page_idx) {
            let copy_len = data.len().min(map.size());
            map.as_slice_mut()[..copy_len].copy_from_slice(&data[..copy_len]);
        }
    }

    /// Zero all available map pages so FanaLab reads RPM=0 and resets wheel LEDs.
    fn zero_all(&mut self) {
        for map in self.maps.iter_mut().filter_map(Option::as_mut) {
            map.as_slice_mut().fill(0);
        }
    }

    fn page_size(&self, page_idx: usize) -> usize {
        if let Some(Some(m)) = self.maps.get(page_idx) {
            m.size()
        } else {
            0
        }
    }
}

/// Which AC variant was last announced via PAGE_GAME_ANNOUNCE.
#[derive(Clone, Copy, PartialEq)]
enum ActiveGame {
    Evo,
    Ac1,
}

/// Either one game's maps (lazy, dropped on stale) or both games' maps (eager, zeroed on stale).
enum MapMode {
    /// Single-game: maps created lazily on first data arrival; dropped on stale timeout.
    Single(GameMapSet),
    /// Dual-game: maps for both EVO and AC1 created at startup; status zeroed on stale.
    /// `active` tracks which game's maps should receive writes, set by PAGE_GAME_ANNOUNCE.
    /// Until a game is announced, writes go to both maps to avoid missing the first frame.
    Dual {
        evo: GameMapSet,
        ac1: GameMapSet,
        active: Option<ActiveGame>,
    },
}

impl MapMode {
    fn write_page(&mut self, page_idx: usize, data: &[u8]) {
        match self {
            Self::Single(set) => set.write_page(page_idx, data),
            Self::Dual { evo, ac1, active } => match active {
                Some(ActiveGame::Evo) => evo.write_page(page_idx, data),
                Some(ActiveGame::Ac1) => ac1.write_page(page_idx, data),
                // No game announced yet — write to both so the first frames are not lost.
                None => {
                    evo.write_page(page_idx, data);
                    ac1.write_page(page_idx, data);
                }
            },
        }
    }

    fn zero_all_pages(&mut self) {
        match self {
            Self::Single(set) => set.zero_all(),
            Self::Dual { evo, ac1, active } => match active {
                Some(ActiveGame::Evo) => evo.zero_all(),
                Some(ActiveGame::Ac1) => ac1.zero_all(),
                None => {
                    evo.zero_all();
                    ac1.zero_all();
                }
            },
        }
    }

    fn page_size(&self, page_idx: usize) -> usize {
        match self {
            Self::Single(set) => set.page_size(page_idx),
            Self::Dual { evo, ac1, .. } => {
                // Both sets use DUAL_MAP_SIZE, so either value is the same.
                // Return max anyway for forward compatibility.
                evo.page_size(page_idx).max(ac1.page_size(page_idx))
            }
        }
    }

    // DIAGNOSTIC: read the first `n` bytes of the EVO physics map for hex logging.
    // In Single mode this reads maps[0] of whatever game was started; in Dual mode
    // it always reads the evo set (acevo_pmf_physics).
    fn evo_physics_peek(&self, n: usize) -> &[u8] {
        let slice: &[u8] = match self {
            Self::Single(set) => set.maps[0].as_ref().map_or(&[], |m| m.as_slice()),
            Self::Dual { evo, .. } => evo.maps[0].as_ref().map_or(&[], |m| m.as_slice()),
        };
        &slice[..n.min(slice.len())]
    }

    // DIAGNOSTIC: read the first `n` bytes of the EVO graphics map.
    fn evo_graphics_peek(&self, n: usize) -> &[u8] {
        let slice: &[u8] = match self {
            Self::Single(set) => set.maps[1].as_ref().map_or(&[], |m| m.as_slice()),
            Self::Dual { evo, .. } => evo.maps[1].as_ref().map_or(&[], |m| m.as_slice()),
        };
        &slice[..n.min(slice.len())]
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
    let mut maps: Option<MapMode> = if args.game.is_none() {
        let evo = GameMapSet::create(&game::EVO, DUAL_MAP_SIZE);
        let ac1 = GameMapSet::create(&game::AC1, DUAL_MAP_SIZE);
        println!(
            "Created shared memory maps for {} and {}",
            game::EVO.name,
            game::AC1.name
        );
        println!(
            "  {} ({DUAL_MAP_SIZE} bytes), {} ({DUAL_MAP_SIZE} bytes), {} ({DUAL_MAP_SIZE} bytes)",
            game::EVO.physics_map,
            game::EVO.graphics_map,
            game::EVO.static_map,
        );
        println!(
            "  {} ({DUAL_MAP_SIZE} bytes), {} ({DUAL_MAP_SIZE} bytes), {} ({DUAL_MAP_SIZE} bytes)",
            game::AC1.physics_map,
            game::AC1.graphics_map,
            game::AC1.static_map,
        );
        Some(MapMode::Dual {
            evo,
            ac1,
            active: None,
        })
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
    let mut last_hex_dump = Instant::now(); // DIAGNOSTIC
    let mut stats = Stats::new("target");
    let mut seq_start: Option<Instant> = None;
    let mut first_frame_logged = false;
    let mut first_data_at: Option<Instant> = None;
    let mut on_first_fired = false;
    // Track whether a game announce has been received, and which game_id.
    // Used to deduplicate announces (source may resend on reconnect) and to
    // gate on_first_data so we don't fire it before we know which game is active.
    let mut game_announced = false;
    let mut last_announced_game_id: Option<u8> = None;

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

                // Game announce: source tells us which AC variant is running.
                if buf_offset == PAGE_GAME_ANNOUNCE {
                    if let Some(bytes) = assembled {
                        if let Some(&game_id) = bytes.first() {
                            println!("[DIAG] PAGE_GAME_ANNOUNCE: game_id={game_id}"); // DIAGNOSTIC
                                                                                      // Deduplicate: skip if this is the same game_id as before to
                                                                                      // avoid re-triggering switchgame/stub respawn on reconnect.
                            if last_announced_game_id == Some(game_id) {
                                continue;
                            }
                            last_announced_game_id = Some(game_id);
                            game_announced = true;
                            if let Some(cb) = &args.on_game_announce {
                                cb(game_id);
                            }
                            // Gate dual-mode writes to the announced game's maps only.
                            if let Some(MapMode::Dual { active, .. }) = maps.as_mut() {
                                *active = match game_id {
                                    GAME_ID_AC1 | GAME_ID_ACC => Some(ActiveGame::Ac1),
                                    GAME_ID_EVO => Some(ActiveGame::Evo),
                                    _ => None,
                                };
                                let name = match active {
                                    Some(ActiveGame::Evo) => game::EVO.name,
                                    Some(ActiveGame::Ac1) => game::AC1.name,
                                    None => "unknown",
                                };
                                println!("[AC Teleport] Active game: {name} (game_id={game_id})");
                            }
                        }
                    }
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
                        let set = GameMapSet::create(game, DUAL_MAP_SIZE);
                        println!("Created shared memory maps for {}.", game.name);
                        maps = Some(MapMode::Single(set));
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
                            // DIAGNOSTIC: print first 32 bytes of EVO physics + graphics every 5s.
                            if last_hex_dump.elapsed() >= Duration::from_secs(5) {
                                let phys_peek = map_mode.evo_physics_peek(32);
                                let phys_hex = phys_peek
                                    .iter()
                                    .map(|b| format!("{b:02x}"))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                println!("[DIAG] acevo_pmf_physics[0..32]: {phys_hex}");
                                let gfx_peek = map_mode.evo_graphics_peek(32);
                                let gfx_hex = gfx_peek
                                    .iter()
                                    .map(|b| format!("{b:02x}"))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                println!("[DIAG] acevo_pmf_graphics[0..32]: {gfx_hex}");
                                last_hex_dump = Instant::now();
                            }
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
                        println!("[AC Teleport] Tip: if SimHub shows no data, enable the Assetto Corsa plugin:");
                        println!("[AC Teleport]   SimHub > Settings > In-game apps tab > Assetto Corsa > enable");
                        first_frame_logged = true;
                        first_data_at = Some(Instant::now());
                        // Fire on_first_data unconditionally on the first assembled frame —
                        // same as iRacing Teleport. This is what makes the AC target start
                        // and switch SimHub. The game announce (if/when it arrives) only
                        // CORRECTS the active game via on_game_announce; it must never be a
                        // precondition for starting, or a missed/late announce silently
                        // strands the target with data flowing but SimHub never switched.
                        if let Some(cb) = &args.on_first_data {
                            cb();
                        }
                        on_first_fired = true;
                    }

                    // Fallback: data flowing for 2 s with no announce — fire on_first_data
                    // so AC still starts (orchestrator defaults to AC1).  With periodic
                    // re-announces from the source this path will rarely trigger.
                    if !on_first_fired && !game_announced {
                        if let Some(t) = first_data_at {
                            if t.elapsed() >= Duration::from_secs(2) {
                                println!("[AC Teleport] No game announce after 2s — defaulting");
                                if let Some(cb) = &args.on_first_data {
                                    cb();
                                }
                                on_first_fired = true;
                            }
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
                            first_data_at = None;
                            on_first_fired = false;
                            if let Some(cb) = &args.on_stale {
                                cb();
                            }
                        }
                        Some(MapMode::Dual { .. }) => {
                            // Dual mode: keep maps alive (already zeroed above).
                            first_frame_logged = false;
                            first_data_at = None;
                            on_first_fired = false;
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
