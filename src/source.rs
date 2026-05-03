use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::logger::Logger;
use crate::scanner::{probe_ac_maps, ProcessScanner, ShmemDetection};

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
}

fn detect_shmem_game(
    scanner: &ProcessScanner,
    cfg: &Config,
    current: Option<ShmemGame>,
) -> Option<Detection> {
    if cfg.apps.iracing_teleport_enabled && scanner.is_running(&["iRacingSim64DX11.exe"]) {
        return Some(Detection {
            game: ShmemGame::Iracing,
            label: "iRacing",
        });
    }
    if cfg.apps.ac_teleport_enabled {
        // Skip the 100 ms probe when we're already committed to iRacing or an AC variant.
        let skip_probe = matches!(
            current,
            Some(ShmemGame::Iracing | ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc)
        );
        if skip_probe {
            // Re-return the current AC game to hold the slot without re-probing.
            if let Some(c @ (ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc)) = current {
                let label = match c {
                    ShmemGame::AcEvo => "Assetto Corsa EVO",
                    ShmemGame::Ac1 => "Assetto Corsa",
                    ShmemGame::Acc => "Assetto Corsa Competizione",
                    _ => unreachable!(),
                };
                return Some(Detection { game: c, label });
            }
        } else {
            let ac = probe_ac_maps();

            match ac.ac_evo {
                Some(ShmemDetection::Live) => {
                    return Some(Detection {
                        game: ShmemGame::AcEvo,
                        label: "Assetto Corsa EVO",
                    });
                }
                // Maps exist but no session — only trust if the process is running.
                Some(ShmemDetection::Stale)
                    if scanner.is_running(&["AssettoCorsa_EVO.exe", "assettocorsaevo.exe"]) =>
                {
                    return Some(Detection {
                        game: ShmemGame::AcEvo,
                        label: "Assetto Corsa EVO",
                    });
                }
                _ => {} // None or Stale without process → skip
            }

            match ac.ac1 {
                Some(ShmemDetection::Live) => {
                    // ACC and AC1 share the same map names — use process to distinguish.
                    if scanner.is_running(&["acc.exe"]) {
                        return Some(Detection {
                            game: ShmemGame::Acc,
                            label: "Assetto Corsa Competizione",
                        });
                    }
                    return Some(Detection {
                        game: ShmemGame::Ac1,
                        label: "Assetto Corsa",
                    });
                }
                Some(ShmemDetection::Stale) if scanner.is_running(&["acc.exe"]) => {
                    return Some(Detection {
                        game: ShmemGame::Acc,
                        label: "Assetto Corsa Competizione",
                    });
                }
                Some(ShmemDetection::Stale) if scanner.is_running(&["acs.exe"]) => {
                    return Some(Detection {
                        game: ShmemGame::Ac1,
                        label: "Assetto Corsa",
                    });
                }
                _ => {} // None or Stale without process → skip
            }
        }
    }
    // sim-relay UDP games (lowest priority — only when shmem games are absent)
    if cfg.apps.sim_relay_enabled {
        for game in sim_relay::games::GAMES {
            if game.console {
                continue;
            }
            if scanner.is_running(game.process_names) {
                return Some(Detection {
                    game: ShmemGame::SimRelay {
                        id: game.id,
                        name: game.name,
                    },
                    label: game.name,
                });
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

    fn start_shmem(&mut self, detection: &Detection, config: &Config, log: &Logger) {
        if !self.failures.is_allowed() {
            log.log(&format!("[{}] Skipping start — in backoff", self.name));
            return;
        }
        // Guard: previous detached thread may still hold its socket.
        if let Some(ref d) = self.detached {
            if !d.is_gone() {
                log.log(&format!(
                    "[{}] Waiting for detached thread to release socket",
                    self.name
                ));
                return;
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

        log.log(&format!("[{label}] Detected — starting"));
        self.state = SlotState::Running { handle, game };
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

// ── Main source loop ──────────────────────────────────────────────────────────

pub fn run(config: Config, log: &Logger, shutdown: Receiver<()>) {
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

    let mut scanner = ProcessScanner::new();
    let mut shmem = AppSlot::new("shmem");
    let mut last_heartbeat = Instant::now();

    log.log(&format!(
        "Scanning for games every {}s...",
        config.detection.scan_interval
    ));

    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }

        scanner.refresh();
        let desired = detect_shmem_game(&scanner, &config, shmem.current_game());

        if shmem.is_running_finished() {
            let name = game_label(shmem.current_game());
            log.log(&format!("[{name}] Thread exited unexpectedly"));
            let old = std::mem::replace(&mut shmem.state, SlotState::Idle);
            drop(old);
            shmem.failures.record(log, name);
        }

        // Pre-compute values that require a borrow on shmem.state so we can
        // call mutable methods inside match arms without a borrow conflict.
        let drain_expired = shmem
            .drain_since()
            .is_some_and(|s| s.elapsed() >= drain_timeout);
        let draining_game = shmem.draining_game();

        match (&shmem.state, desired.as_ref()) {
            (SlotState::Idle, Some(d)) => {
                shmem.start_shmem(d, &config, log);
            }

            (SlotState::Running { game: running, .. }, Some(d)) if *running == d.game => {}

            (SlotState::Running { .. }, Some(d)) => {
                let old_name = game_label(shmem.current_game());
                log.log(&format!("[{old_name}] Stopping (switching to {})", d.label));
                shmem.stop(log);
                shmem.failures.reset();
                shmem.start_shmem(d, &config, log);
            }

            (SlotState::Running { .. }, None) => {
                let name = game_label(shmem.current_game());
                log.log(&format!(
                    "[{name}] Game closed — draining {}s",
                    config.detection.drain_seconds
                ));
                shmem.begin_drain();
            }

            // Same game re-detected while draining — cancel shutdown (issue #5 fix).
            (SlotState::Draining { .. }, Some(d)) if draining_game == Some(d.game) => {
                log.log(&format!(
                    "[{}] Game re-detected — cancelling shutdown",
                    d.label
                ));
                shmem.cancel_drain();
            }

            // Different game detected while draining — stop old immediately, start new (issue #5).
            (SlotState::Draining { .. }, Some(d)) => {
                let old_name = game_label(shmem.current_game());
                log.log(&format!("[{old_name}] Stopping (switching to {})", d.label));
                shmem.stop(log);
                shmem.failures.reset();
                shmem.start_shmem(d, &config, log);
            }

            (SlotState::Draining { .. }, None) if drain_expired => {
                let name = game_label(shmem.current_game());
                log.log(&format!("[{name}] Stopped"));
                shmem.stop(log);
                shmem.failures.reset();
            }

            _ => {}
        }

        // Periodic status heartbeat every 60s.
        if last_heartbeat.elapsed() >= heartbeat_interval {
            log.log(&format!(
                "Status: {}/{}",
                game_label(shmem.current_game()),
                shmem.state_label()
            ));
            last_heartbeat = Instant::now();
        }

        std::thread::sleep(scan_interval);
    }

    log.log("Shutting down...");
    shmem.stop(log);
    log.log("All apps stopped. Goodbye.");
}
