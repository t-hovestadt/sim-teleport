use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::logger::Logger;
use crate::scanner::ProcessScanner;

// ── Game detection ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShmemGame {
    Iracing,
    AcTeleport,
}

struct Detection {
    game: ShmemGame,
    label: &'static str,
}

fn detect_shmem_game(scanner: &ProcessScanner, cfg: &Config) -> Option<Detection> {
    if cfg.apps.iracing_teleport_enabled && scanner.is_running(&["iRacingSim64DX11.exe"]) {
        return Some(Detection { game: ShmemGame::Iracing, label: "iRacing" });
    }
    if cfg.apps.ac_teleport_enabled {
        if scanner.is_running(&["AssettoCorsa_EVO.exe", "assettocorsaevo.exe"]) {
            return Some(Detection { game: ShmemGame::AcTeleport, label: "Assetto Corsa EVO" });
        }
        if scanner.is_running(&["acs.exe"]) {
            return Some(Detection { game: ShmemGame::AcTeleport, label: "Assetto Corsa" });
        }
        if scanner.is_running(&["acc.exe"]) {
            return Some(Detection { game: ShmemGame::AcTeleport, label: "Assetto Corsa Competizione" });
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
        Self { failures: Vec::new(), disabled_until: None }
    }

    fn record(&mut self, log: &Logger, app: &str) {
        let now = Instant::now();
        self.failures.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
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

// ── AppSlot ───────────────────────────────────────────────────────────────────

enum SlotState {
    Idle,
    Running { handle: JoinHandle<()>, game: ShmemGame },
    Draining { since: Instant, handle: JoinHandle<()>, game: ShmemGame },
    AlwaysOn { handle: JoinHandle<()> },
}

struct AppSlot {
    name: &'static str,
    state: SlotState,
    shutdown_tx: Option<Sender<()>>,
    failures: FailureTracker,
}

impl AppSlot {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            state: SlotState::Idle,
            shutdown_tx: None,
            failures: FailureTracker::new(),
        }
    }

    fn start_shmem(&mut self, detection: &Detection, config: &Config, log: &Logger) {
        if !self.failures.is_allowed() {
            log.log(&format!("[{}] Skipping start — in backoff", self.name));
            return;
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
                    if let Err(e) = teleport::run_source(
                        teleport::SourceConfig {
                            target: format!(
                                "{}:{}",
                                cfg.network.target_ip, cfg.ports.iracing_teleport
                            ),
                            bind: "0.0.0.0:0".to_string(),
                            unicast: true,
                            ..teleport::SourceConfig::default()
                        },
                        rx,
                    ) {
                        eprintln!("[{name}] iRacing Teleport: {e}");
                    }
                }
                ShmemGame::AcTeleport => {
                    // game: None → ac-teleport probes shared memory to auto-detect
                    // the exact variant (EVO / AC1 / ACC) at startup.
                    if let Err(e) = ac_teleport::source::run(
                        ac_teleport::SourceArgs {
                            game: None,
                            target: format!(
                                "{}:{}",
                                cfg.network.target_ip, cfg.ports.ac_teleport
                            ),
                            bind: format!("0.0.0.0:{}", cfg.ports.ac_teleport),
                            unicast: true,
                            busy_wait: false,
                            pin_core: None,
                            high_priority: false,
                            poll_rate: 60,
                        },
                        rx,
                    ) {
                        eprintln!("[{name}] AC Teleport: {e}");
                    }
                }
            })
            .expect("failed to spawn shmem thread");

        log.log(&format!("[{label}] Detected — starting"));
        self.state = SlotState::Running { handle, game };
    }

    fn start_relay(&mut self, config: &Config, log: &Logger) {
        let (tx, rx) = mpsc::channel::<()>();
        self.shutdown_tx = Some(tx);
        let cfg = config.clone();
        let name = self.name;

        let handle = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                if let Err(e) = sim_relay::source::run(
                    sim_relay::SourceArgs {
                        target: cfg.network.target_ip.clone(),
                        games: None,
                        all: false,
                        local_forward: false,
                        bind: "0.0.0.0".to_string(),
                        high_priority: false,
                        scan_interval: cfg.detection.scan_interval,
                        grace_period: cfg.detection.drain_seconds,
                        include_console: false,
                        force_bind: false,
                    },
                    rx,
                ) {
                    eprintln!("[{name}] Sim Relay: {e}");
                }
            })
            .expect("failed to spawn relay thread");

        self.state = SlotState::AlwaysOn { handle };
        log.log("[Sim Relay] Started (always-on, auto-detects UDP games)");
    }

    fn begin_drain(&mut self) {
        let state = std::mem::replace(&mut self.state, SlotState::Idle);
        if let SlotState::Running { handle, game } = state {
            self.state = SlotState::Draining { since: Instant::now(), handle, game };
        }
    }

    fn cancel_drain(&mut self) {
        let state = std::mem::replace(&mut self.state, SlotState::Idle);
        if let SlotState::Draining { handle, game, .. } = state {
            self.state = SlotState::Running { handle, game };
        }
    }

    fn stop(&mut self) {
        let state = std::mem::replace(&mut self.state, SlotState::Idle);
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        match state {
            SlotState::Running { handle, .. }
            | SlotState::Draining { handle, .. }
            | SlotState::AlwaysOn { handle } => {
                let _ = handle.join();
            }
            SlotState::Idle => {}
        }
    }

    fn ensure_relay_alive(&mut self, config: &Config, log: &Logger) {
        let finished = match &self.state {
            SlotState::AlwaysOn { handle } => handle.is_finished(),
            _ => false,
        };
        if finished {
            let old = std::mem::replace(&mut self.state, SlotState::Idle);
            drop(old);
            self.failures.record(log, self.name);
            if self.failures.is_allowed() {
                log.log("[Sim Relay] Restarting after crash...");
                self.start_relay(config, log);
            }
        }
    }

    fn current_game(&self) -> Option<ShmemGame> {
        match &self.state {
            SlotState::Running { game, .. } | SlotState::Draining { game, .. } => Some(*game),
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
}

fn game_label(game: Option<ShmemGame>) -> &'static str {
    match game {
        Some(ShmemGame::Iracing) => "iRacing Teleport",
        Some(ShmemGame::AcTeleport) => "AC Teleport",
        None => "app",
    }
}

// ── Main source loop ──────────────────────────────────────────────────────────

pub fn run(config: Config, log: &Logger, shutdown: Receiver<()>) {
    log.log(&format!(
        "Network: {} -> {}",
        config.network.source_ip, config.network.target_ip
    ));
    log.log(&format!(
        "Ports: iRacing {} | AC {} | Sim Relay (native game ports)",
        config.ports.iracing_teleport, config.ports.ac_teleport
    ));

    let scan_interval = Duration::from_secs(config.detection.scan_interval);
    let drain_timeout = Duration::from_secs(config.detection.drain_seconds);

    let mut scanner = ProcessScanner::new();
    let mut shmem = AppSlot::new("shmem");
    let mut relay = AppSlot::new("Sim Relay");

    if config.apps.sim_relay_enabled {
        relay.start_relay(&config, log);
    }

    log.log(&format!(
        "Scanning for games every {}s...",
        config.detection.scan_interval
    ));

    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }

        scanner.refresh();
        let desired = detect_shmem_game(&scanner, &config);

        if shmem.is_running_finished() {
            let name = game_label(shmem.current_game());
            log.log(&format!("[{name}] Thread exited unexpectedly"));
            let old = std::mem::replace(&mut shmem.state, SlotState::Idle);
            drop(old);
            shmem.failures.record(log, name);
        }

        // Pre-compute drain expiry to avoid holding a borrow on shmem.state
        // while calling mutable methods inside match arms.
        let drain_expired = shmem
            .drain_since()
            .is_some_and(|s| s.elapsed() >= drain_timeout);

        match (&shmem.state, desired.as_ref()) {
            (SlotState::Idle, Some(d)) => {
                shmem.start_shmem(d, &config, log);
            }

            (SlotState::Running { game: running, .. }, Some(d)) if *running == d.game => {}

            (SlotState::Running { .. }, Some(d)) => {
                let old_name = game_label(shmem.current_game());
                log.log(&format!("[{old_name}] Stopping (switching to {})", d.label));
                shmem.stop();
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

            (SlotState::Draining { .. }, Some(d)) => {
                log.log(&format!("[{}] Game re-detected — cancelling shutdown", d.label));
                shmem.cancel_drain();
            }

            (SlotState::Draining { .. }, None) if drain_expired => {
                let name = game_label(shmem.current_game());
                log.log(&format!("[{name}] Stopped"));
                shmem.stop();
                shmem.failures.reset();
            }

            _ => {}
        }

        if config.apps.sim_relay_enabled {
            relay.ensure_relay_alive(&config, log);
        }

        std::thread::sleep(scan_interval);
    }

    log.log("Shutting down...");
    shmem.stop();
    relay.stop();
    log.log("All apps stopped. Goodbye.");
}
