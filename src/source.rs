use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::logger::Logger;
use crate::report::SessionReport;
use crate::scanner::{probe_ac_maps, AcProbeResult, ProcessScanner, ShmemDetection};

// ── Scan context (passed to detection functions) ──────────────────────────────

struct ScanCtx<'a> {
    verbose: bool,
    log: &'a Logger,
    report: &'a mut SessionReport,
}

// ── Game detection ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShmemGame {
    Iracing,
    AcEvo,
    Ac1,
    Acc,
    SimRelay {
        id: &'static str,
        name: &'static str,
    },
}

struct Detection {
    game: ShmemGame,
    label: &'static str,
    how: &'static str,
}

/// Determine the desired game from pre-computed probe results.
///
/// `iracing_detected`: true when `iracingsim64dx11.exe` is in the process snapshot.
/// `ac`: result of `probe_ac_maps()` — both fields are `None` when the probe was skipped.
/// `running_ac`: the game currently in `SlotState::Running`, if it is an AC variant.
///   When the AC probe was skipped because we were already running an AC game, this is
///   the game to re-return so the slot stays active without an unnecessary re-probe.
/// `scanner`: process snapshot — consulted for iRacing, AC stale tiebreaker, and sim-relay;
///   always fresh (refreshed unconditionally at the top of `run_detection_cycle`).
fn detect_shmem_game(
    iracing_detected: bool,
    ac: &AcProbeResult,
    running_ac: Option<ShmemGame>,
    scanner: &ProcessScanner,
    cfg: &Config,
    ctx: &mut ScanCtx<'_>,
) -> Option<Detection> {
    let verbose = ctx.verbose;
    let log = ctx.log;
    if cfg.apps.iracing_teleport_enabled && iracing_detected {
        return Some(Detection {
            game: ShmemGame::Iracing,
            label: "iRacing",
            how: "process scan",
        });
    }
    if cfg.apps.ac_teleport_enabled {
        // When the probe was skipped (both fields None) because we were already running
        // an AC game, re-return that game so the slot stays active.
        if ac.ac_evo.is_none() && ac.ac1.is_none() {
            if let Some(c @ (ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc)) = running_ac {
                let label = match c {
                    ShmemGame::AcEvo => "Assetto Corsa EVO",
                    ShmemGame::Ac1 => "Assetto Corsa",
                    ShmemGame::Acc => "Assetto Corsa Competizione",
                    _ => unreachable!(),
                };
                return Some(Detection {
                    game: c,
                    label,
                    how: "already running",
                });
            }
        }

        match ac.ac_evo {
            Some(ShmemDetection::Live) => {
                return Some(Detection {
                    game: ShmemGame::AcEvo,
                    label: "Assetto Corsa EVO",
                    how: "live shmem",
                });
            }
            Some(ShmemDetection::Stale) => {
                ctx.report.ac_evo_tiebreaker += 1;
                if scanner.is_running(&["AssettoCorsa_EVO.exe", "assettocorsaevo.exe"]) {
                    ctx.report.ac_evo_tiebreaker_hits += 1;
                    ctx.report.push_note(format!(
                        "{} AC EVO: stale maps, process found → on menu",
                        chrono::Local::now().format("%H:%M:%S")
                    ));
                    if verbose {
                        log.log("[scan] AC EVO tiebreaker: process found — game on menu");
                    }
                    return Some(Detection {
                        game: ShmemGame::AcEvo,
                        label: "Assetto Corsa EVO",
                        how: "shmem+process (menu)",
                    });
                }
                ctx.report.push_note(format!(
                    "{} AC EVO: stale maps, no process → ghost",
                    chrono::Local::now().format("%H:%M:%S")
                ));
                if verbose {
                    log.log("[scan] AC EVO tiebreaker: process not found — ghost maps, skipping");
                }
            }
            None => {}
        }

        match ac.ac1 {
            Some(ShmemDetection::Live) => {
                // ACC and AC1 share the same map names — use process to distinguish.
                if scanner.is_running(&["acc.exe"]) {
                    if verbose {
                        log.log(
                            "[scan] AC1/ACC tiebreaker: acc.exe found — Assetto Corsa Competizione",
                        );
                    }
                    return Some(Detection {
                        game: ShmemGame::Acc,
                        label: "Assetto Corsa Competizione",
                        how: "live shmem",
                    });
                }
                if verbose {
                    log.log("[scan] AC1/ACC tiebreaker: acc.exe not found — Assetto Corsa");
                }
                return Some(Detection {
                    game: ShmemGame::Ac1,
                    label: "Assetto Corsa",
                    how: "live shmem",
                });
            }
            Some(ShmemDetection::Stale) => {
                ctx.report.ac1_tiebreaker += 1;
                if scanner.is_running(&["acc.exe"]) {
                    ctx.report.ac1_tiebreaker_hits += 1;
                    ctx.report.push_note(format!(
                        "{} AC1: stale maps, acc.exe found → ACC on menu",
                        chrono::Local::now().format("%H:%M:%S")
                    ));
                    if verbose {
                        log.log("[scan] AC1/ACC tiebreaker: acc.exe found (stale) — Assetto Corsa Competizione");
                    }
                    return Some(Detection {
                        game: ShmemGame::Acc,
                        label: "Assetto Corsa Competizione",
                        how: "shmem+process (menu)",
                    });
                }
                if scanner.is_running(&["acs.exe"]) {
                    ctx.report.ac1_tiebreaker_hits += 1;
                    ctx.report.push_note(format!(
                        "{} AC1: stale maps, acs.exe found → AC on menu",
                        chrono::Local::now().format("%H:%M:%S")
                    ));
                    if verbose {
                        log.log("[scan] AC1/ACC tiebreaker: acs.exe found (stale) — Assetto Corsa");
                    }
                    return Some(Detection {
                        game: ShmemGame::Ac1,
                        label: "Assetto Corsa",
                        how: "shmem+process (menu)",
                    });
                }
                ctx.report.push_note(format!(
                    "{} AC1: stale maps, no process → ghost",
                    chrono::Local::now().format("%H:%M:%S")
                ));
                if verbose {
                    log.log("[scan] AC1 tiebreaker: no process found — ghost maps, skipping");
                }
            }
            None => {}
        }
    }
    // sim-relay UDP games (lowest priority — only when shmem games are absent)
    if cfg.apps.sim_relay_enabled {
        let game_count = sim_relay::games::GAMES
            .iter()
            .filter(|g| !g.console)
            .count();
        if verbose {
            log.log(&format!(
                "[scan] Process scan: checking {game_count} sim-relay game entries"
            ));
        }
        for game in sim_relay::games::GAMES {
            if game.console {
                continue;
            }
            if scanner.is_running(game.process_names) {
                ctx.report.process_scan_matches += 1;
                ctx.report.push_note(format!(
                    "{} Process match: {} → {} (port {})",
                    chrono::Local::now().format("%H:%M:%S"),
                    game.process_names.join(", "),
                    game.name,
                    game.default_port
                ));
                if verbose {
                    let names = game.process_names.join(", ");
                    log.log(&format!(
                        "[scan] Match: {names} → {} (port {})",
                        game.name, game.default_port
                    ));
                }
                return Some(Detection {
                    game: ShmemGame::SimRelay {
                        id: game.id,
                        name: game.name,
                    },
                    label: game.name,
                    how: "process scan",
                });
            }
        }
        let process_count = scanner.process_count();
        ctx.report.push_note(format!(
            "{} Process scan: 0 matches in {process_count} processes",
            chrono::Local::now().format("%H:%M:%S")
        ));
        if verbose {
            log.log(&format!(
                "[scan] Process scan: 0 matches in {process_count} processes",
            ));
            log.log("[scan] Expected processes (first 5):");
            for game in sim_relay::games::GAMES
                .iter()
                .filter(|g| !g.console)
                .take(5)
            {
                log.log(&format!("[scan]   {}: {:?}", game.name, game.process_names));
            }
        }
    }
    None
}

// ── Error recovery ────────────────────────────────────────────────────────────

struct FailureTracker {
    failures: Vec<Instant>,
    disabled_until: Option<Instant>,
}

impl FailureTracker {
    fn new() -> Self {
        Self {
            failures: Vec::new(),
            disabled_until: None,
        }
    }

    fn record(&mut self, log: &Logger, app: &str) {
        let now = Instant::now();
        self.failures
            .retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        self.failures.push(now);
        if self.failures.len() >= 3 {
            self.disabled_until = Some(now + Duration::from_secs(300));
            log.log(&format!("[{app}] Failed 3x in 60s — disabling for 5 min"));
        }
    }

    fn is_allowed(&self) -> bool {
        self.disabled_until.is_none_or(|t| Instant::now() > t)
    }

    fn reset(&mut self) {
        self.failures.clear();
        self.disabled_until = None;
    }
}

// ── DetachedThread guard ──────────────────────────────────────────────────────

// When stop() times out it drops the JoinHandle, detaching the thread. The
// detached thread still holds its UDP socket, so a new thread must not bind
// the same address until the old one has actually exited. We keep a reference
// to the old handle and poll is_finished() before allowing a new start.
struct DetachedThread {
    handle: JoinHandle<()>,
}

impl DetachedThread {
    fn is_gone(&self) -> bool {
        self.handle.is_finished()
    }
}

// ── AppSlot ───────────────────────────────────────────────────────────────────

enum SlotState {
    Idle,
    Running {
        handle: JoinHandle<()>,
        game: ShmemGame,
    },
    Draining {
        since: Instant,
        handle: JoinHandle<()>,
        game: ShmemGame,
    },
}

struct AppSlot {
    name: &'static str,
    state: SlotState,
    shutdown_tx: Option<Sender<()>>,
    failures: FailureTracker,
    detached: Option<DetachedThread>,
}

impl AppSlot {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            state: SlotState::Idle,
            shutdown_tx: None,
            failures: FailureTracker::new(),
            detached: None,
        }
    }

    fn start_shmem(&mut self, detection: &Detection, config: &Config, log: &Logger) -> bool {
        if !self.failures.is_allowed() {
            log.log(&format!("[{}] Skipping start — in backoff", self.name));
            return false;
        }
        // Guard: previous detached thread may still hold its socket.
        if let Some(ref d) = self.detached {
            if !d.is_gone() {
                log.log(&format!(
                    "[{}] Waiting for detached thread to release socket",
                    self.name
                ));
                return false;
            }
            self.detached = None;
        }
        let (tx, rx) = mpsc::channel::<()>();
        self.shutdown_tx = Some(tx);
        let game = detection.game;
        let label = detection.label;
        let cfg = config.clone();
        let name = self.name;

        let handle = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || match game {
                ShmemGame::Iracing => {
                    let (target, bind) = if cfg.network.unicast {
                        // Unicast: send directly to target_ip, bind to source_ip so
                        // firewall rules pass resync packets back to this exact address.
                        (
                            format!("{}:{}", cfg.network.target_ip, cfg.ports.iracing_teleport),
                            format!("{}:{}", cfg.network.source_ip, cfg.ports.iracing_teleport),
                        )
                    } else {
                        // Multicast (LAN default): zero config, no IPs needed.
                        (
                            format!(
                                "{}:{}",
                                teleport::DEFAULT_MULTICAST,
                                cfg.ports.iracing_teleport
                            ),
                            "0.0.0.0:0".to_string(),
                        )
                    };
                    if let Err(e) = teleport::run_source(
                        teleport::SourceConfig {
                            target,
                            bind,
                            unicast: cfg.network.unicast,
                            high_priority: cfg.apps.high_priority,
                            busy_wait: cfg.apps.busy_wait,
                            reconnect_timeout_secs: cfg.advanced.reconnect_timeout_secs,
                            datagram_size: cfg.advanced.datagram_size,
                            ..teleport::SourceConfig::default()
                        },
                        rx,
                    ) {
                        eprintln!("[{name}] iRacing Teleport: {e}");
                    }
                }
                ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc => {
                    // Tell ac-teleport exactly which maps to open so it is not fooled
                    // by stale maps left over from a previously closed AC variant.
                    let game_cfg: Option<&'static ac_teleport::GameConfig> = match game {
                        ShmemGame::AcEvo => Some(&ac_teleport::EVO),
                        ShmemGame::Ac1 => Some(&ac_teleport::AC1),
                        // ACC map format is unconfirmed; fall back to auto-detect.
                        ShmemGame::Acc => None,
                        _ => unreachable!(),
                    };
                    let (target, bind) = if cfg.network.unicast {
                        (
                            format!("{}:{}", cfg.network.target_ip, cfg.ports.ac_teleport),
                            format!("{}:{}", cfg.network.source_ip, cfg.ports.ac_teleport),
                        )
                    } else {
                        (
                            format!("{}:{}", teleport::DEFAULT_MULTICAST, cfg.ports.ac_teleport),
                            "0.0.0.0:0".to_string(),
                        )
                    };
                    if let Err(e) = ac_teleport::source::run(
                        ac_teleport::SourceArgs {
                            game: game_cfg,
                            target,
                            bind,
                            unicast: cfg.network.unicast,
                            busy_wait: cfg.apps.busy_wait,
                            pin_core: None,
                            high_priority: cfg.apps.high_priority,
                            poll_rate: cfg.advanced.ac_poll_rate,
                        },
                        rx,
                    ) {
                        eprintln!("[{name}] AC Teleport: {e}");
                    }
                }
                ShmemGame::SimRelay { id, .. } => {
                    if let Err(e) = sim_relay::source::run(
                        sim_relay::SourceArgs {
                            target: cfg.network.target_ip.clone(),
                            games: Some(vec![id.to_string()]),
                            // force_bind bypasses sim-relay's internal process detection;
                            // sim-bridge owns detection and manages the lifecycle.
                            force_bind: true,
                            all: false,
                            local_forward: false,
                            bind: cfg.network.source_ip.clone(),
                            high_priority: cfg.apps.high_priority,
                            scan_interval: cfg.detection.scan_interval,
                            grace_period: cfg.detection.drain_seconds,
                            include_console: false,
                        },
                        rx,
                    ) {
                        eprintln!("[{name}] Sim Relay: {e}");
                    }
                }
            })
            .expect("failed to spawn shmem thread");

        let app = match game {
            ShmemGame::Iracing => "iRacing Teleport",
            ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc => "AC Teleport",
            ShmemGame::SimRelay { .. } => "Sim Relay",
        };
        log.log(&format!(
            "[{app}] Detected {label} ({}) — starting",
            detection.how
        ));
        self.state = SlotState::Running { handle, game };
        true
    }

    fn begin_drain(&mut self) {
        let state = std::mem::replace(&mut self.state, SlotState::Idle);
        if let SlotState::Running { handle, game } = state {
            self.state = SlotState::Draining {
                since: Instant::now(),
                handle,
                game,
            };
        }
    }

    fn cancel_drain(&mut self) {
        let state = std::mem::replace(&mut self.state, SlotState::Idle);
        if let SlotState::Draining { handle, game, .. } = state {
            self.state = SlotState::Running { handle, game };
        }
    }

    // Send shutdown signal then wait up to 5 seconds. If the thread is still
    // blocked (e.g. iRacing WaitForSingleObject hasn't fired), detach rather
    // than hanging the orchestrator — the thread will exit on its own once the
    // OS timeout fires (issue #4).
    fn stop(&mut self, log: &Logger) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let state = std::mem::replace(&mut self.state, SlotState::Idle);
        match state {
            SlotState::Running { handle, .. } | SlotState::Draining { handle, .. } => {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !handle.is_finished() {
                    if Instant::now() >= deadline {
                        log.log(&format!("[{}] Thread still exiting — detaching", self.name));
                        self.detached = Some(DetachedThread { handle });
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                let _ = handle.join();
            }
            SlotState::Idle => {}
        }
    }

    fn current_game(&self) -> Option<ShmemGame> {
        match &self.state {
            SlotState::Running { game, .. } | SlotState::Draining { game, .. } => Some(*game),
            _ => None,
        }
    }

    fn draining_game(&self) -> Option<ShmemGame> {
        match &self.state {
            SlotState::Draining { game, .. } => Some(*game),
            _ => None,
        }
    }

    fn drain_since(&self) -> Option<Instant> {
        match &self.state {
            SlotState::Draining { since, .. } => Some(*since),
            _ => None,
        }
    }

    fn is_running_finished(&self) -> bool {
        match &self.state {
            SlotState::Running { handle, .. } => handle.is_finished(),
            _ => false,
        }
    }

    fn send_shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    fn state_label(&self) -> &'static str {
        match &self.state {
            SlotState::Idle => "idle",
            SlotState::Running { .. } => "running",
            SlotState::Draining { .. } => "draining",
        }
    }
}

fn game_label(game: Option<ShmemGame>) -> &'static str {
    match game {
        Some(ShmemGame::Iracing) => "iRacing Teleport",
        Some(ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc) => "AC Teleport",
        Some(ShmemGame::SimRelay { name, .. }) => name,
        None => "app",
    }
}

/// Returns false when the game process is no longer running.
/// Called every scan cycle while Running to detect game closure across all game types.
fn is_game_still_running(game: ShmemGame, scanner: &mut ProcessScanner, _log: &Logger) -> bool {
    match game {
        ShmemGame::Iracing => {
            scanner.refresh();
            scanner.is_running(&["iracingsim64dx11.exe"])
        }
        ShmemGame::AcEvo => {
            scanner.refresh();
            scanner.is_running(&["assettocorsa_evo.exe", "assettocorsaevo.exe"])
        }
        ShmemGame::Ac1 => {
            scanner.refresh();
            scanner.is_running(&["acs.exe"])
        }
        ShmemGame::Acc => {
            scanner.refresh();
            scanner.is_running(&["acc.exe"])
        }
        ShmemGame::SimRelay { id, .. } => {
            scanner.refresh();
            if let Some(def) = sim_relay::games::GAMES.iter().find(|g| g.id == id) {
                scanner.is_running(def.process_names)
            } else {
                false
            }
        }
    }
}

/// Run a full detection cycle: process scan, iRacing check, AC shmem probe
/// (when needed), then classify.
/// Called only when the slot is Idle or Draining — never while Running.
fn run_detection_cycle(
    config: &Config,
    scanner: &mut ProcessScanner,
    log: &Logger,
    report: &mut SessionReport,
) -> Option<Detection> {
    let verbose = config.verbose;
    let mut ctx = ScanCtx {
        verbose,
        log,
        report,
    };
    ctx.report.total_scans += 1;

    // Always refresh the process snapshot. Used for iRacing detection, AC
    // stale tiebreakers, and sim-relay scanning. ~1 ms on Windows.
    scanner.refresh();
    ctx.report.process_scans += 1;

    let iracing_detected = if config.apps.iracing_teleport_enabled {
        ctx.report.iracing_probes += 1;
        let found = scanner.is_running(&["iracingsim64dx11.exe"]);
        if found {
            ctx.report.iracing_hits += 1;
        }
        if verbose {
            log.log(if found {
                "[scan] iRacing process: FOUND"
            } else {
                "[scan] iRacing process: not found"
            });
        }
        found
    } else {
        false
    };

    // AC shmem probe: skip if iRacing already detected (saves ~200ms sleep).
    let ac_probe = if config.apps.ac_teleport_enabled && !iracing_detected {
        let result = probe_ac_maps(verbose, log);
        if result.ac_evo.is_some() {
            ctx.report.ac_evo_probes += 1;
            match result.ac_evo {
                Some(ShmemDetection::Live) => ctx.report.ac_evo_live += 1,
                Some(ShmemDetection::Stale) => ctx.report.ac_evo_stale += 1,
                None => {}
            }
        }
        if result.ac1.is_some() {
            ctx.report.ac1_probes += 1;
            match result.ac1 {
                Some(ShmemDetection::Live) => ctx.report.ac1_live += 1,
                Some(ShmemDetection::Stale) => ctx.report.ac1_stale += 1,
                None => {}
            }
        }
        result
    } else {
        AcProbeResult {
            ac_evo: None,
            ac1: None,
        }
    };

    // running_ac is None — this function is not called while Running.
    detect_shmem_game(iracing_detected, &ac_probe, None, scanner, config, &mut ctx)
}

// ── Main source loop ──────────────────────────────────────────────────────────

pub fn run(config: Config, log: &Logger, shutdown: Receiver<()>, version_string: &str) {
    if config.network.source_ip == config.network.target_ip {
        log.log("ERROR: source_ip and target_ip are the same. source mode runs on the gaming PC, target mode runs on the SimHub PC. Check sim-bridge.toml.");
        return;
    }

    log.log(&format!(
        "Network: {} -> {}",
        config.network.source_ip, config.network.target_ip
    ));
    log.log(&format!(
        "Ports: iRacing {} | AC {} | Sim Relay (native game ports)",
        config.ports.iracing_teleport, config.ports.ac_teleport
    ));
    // Issue #14: log enabled/disabled status so users can confirm config is applied.
    log.log(&format!(
        "Apps: iRacing [{}] | AC [{}] | Sim Relay [{}]",
        if config.apps.iracing_teleport_enabled {
            "enabled"
        } else {
            "disabled"
        },
        if config.apps.ac_teleport_enabled {
            "enabled"
        } else {
            "disabled"
        },
        if config.apps.sim_relay_enabled {
            "enabled"
        } else {
            "disabled"
        },
    ));

    let scan_interval = Duration::from_secs(config.detection.scan_interval);
    let drain_timeout = Duration::from_secs(config.detection.drain_seconds);
    let heartbeat_interval = Duration::from_secs(60);
    let report_interval = Duration::from_secs(60);

    let mut scanner = ProcessScanner::new();
    let mut shmem = AppSlot::new("shmem");
    let mut last_heartbeat = Instant::now();
    let mut last_report = Instant::now();

    let config_summary = if config.network.unicast {
        format!(
            "unicast=true source={} target={}",
            config.network.source_ip, config.network.target_ip
        )
    } else {
        format!("unicast=false target={}", config.network.target_ip)
    };
    let mut report = SessionReport::new(version_string.to_string(), config_summary);

    log.log(&format!(
        "Scanning for games every {}s...",
        config.detection.scan_interval
    ));

    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }

        if matches!(shmem.state, SlotState::Running { .. }) {
            // Game is active — watch for thread exit and game process liveness.
            if shmem.is_running_finished() {
                let name = game_label(shmem.current_game());
                log.log(&format!(
                    "[{name}] Game closed — draining {}s",
                    config.detection.drain_seconds
                ));
                shmem.failures.record(log, name);
                shmem.begin_drain();
            } else if let Some(game) = shmem.current_game() {
                if !is_game_still_running(game, &mut scanner, log) {
                    let name = game_label(Some(game));
                    log.log(&format!("[{name}] Game process gone — stopping"));
                    report.push_note(format!(
                        "{} {} process gone",
                        chrono::Local::now().format("%H:%M:%S"),
                        name
                    ));
                    shmem.send_shutdown();
                    std::thread::sleep(Duration::from_millis(500));
                    shmem.begin_drain();
                }
            }
        } else {
            // Idle or Draining — run full detection cycle.
            let desired = run_detection_cycle(&config, &mut scanner, log, &mut report);

            let drain_expired = shmem
                .drain_since()
                .is_some_and(|s| s.elapsed() >= drain_timeout);
            let draining_game = shmem.draining_game();

            match (&shmem.state, desired.as_ref()) {
                (SlotState::Idle, Some(d)) => {
                    let app = match d.game {
                        ShmemGame::Iracing => "iRacing Teleport",
                        ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc => "AC Teleport",
                        ShmemGame::SimRelay { .. } => "Sim Relay",
                    };
                    if shmem.start_shmem(d, &config, log) {
                        report.start_session(app, d.label, d.how);
                    }
                }

                // Same game re-detected while draining — cancel shutdown.
                (SlotState::Draining { .. }, Some(d)) if draining_game == Some(d.game) => {
                    log.log(&format!(
                        "[{}] Game re-detected — cancelling shutdown",
                        d.label
                    ));
                    shmem.cancel_drain();
                }

                // Different game detected while draining — stop old, start new.
                (SlotState::Draining { .. }, Some(d)) => {
                    let old_name = game_label(shmem.current_game());
                    log.log(&format!("[{old_name}] Stopping (switching to {})", d.label));
                    shmem.stop(log);
                    report.end_session("switched to new game");
                    shmem.failures.reset();
                    let app = match d.game {
                        ShmemGame::Iracing => "iRacing Teleport",
                        ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc => "AC Teleport",
                        ShmemGame::SimRelay { .. } => "Sim Relay",
                    };
                    if shmem.start_shmem(d, &config, log) {
                        report.start_session(app, d.label, d.how);
                    }
                }

                (SlotState::Draining { .. }, None) if drain_expired => {
                    let name = game_label(shmem.current_game());
                    log.log(&format!("[{name}] Stopped"));
                    shmem.stop(log);
                    report.end_session("drain expired");
                    shmem.failures.reset();
                }

                _ => {}
            }
        }

        // Periodic status heartbeat and report write every 60s.
        if last_heartbeat.elapsed() >= heartbeat_interval {
            log.log(&format!(
                "Status: {}/{}",
                game_label(shmem.current_game()),
                shmem.state_label()
            ));
            last_heartbeat = Instant::now();
        }
        if last_report.elapsed() >= report_interval {
            report.write();
            last_report = Instant::now();
        }

        std::thread::sleep(scan_interval);
    }

    log.log("Shutting down...");
    shmem.stop(log);
    report.end_session("Ctrl-C shutdown");
    report.write();
    log.log("All apps stopped. Goodbye.");
}
