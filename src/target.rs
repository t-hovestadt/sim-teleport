use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::logger::Logger;

// ── App identity enum (replaces Box<dyn Fn>) ──────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum TargetApp {
    IracingTeleport,
    AcTeleport,
    SimRelay,
}

impl TargetApp {
    fn name(self) -> &'static str {
        match self {
            TargetApp::IracingTeleport => "iRacing Teleport Target",
            TargetApp::AcTeleport => "AC Teleport Target",
            TargetApp::SimRelay => "Sim Relay Target",
        }
    }

    fn spawn(self, config: Config, rx: Receiver<()>) -> JoinHandle<()> {
        match self {
            TargetApp::IracingTeleport => spawn_teleport_target(config, rx),
            TargetApp::AcTeleport => spawn_ac_target(config, rx),
            TargetApp::SimRelay => spawn_relay_target(config, rx),
        }
    }
}

// ── Per-thread slot ───────────────────────────────────────────────────────────

struct TargetSlot {
    app: TargetApp,
    handle: JoinHandle<()>,
    shutdown_tx: Sender<()>,
    config: Config,
    crash_count: u32,
    next_restart: Option<Instant>,
}

impl TargetSlot {
    fn new(app: TargetApp, config: Config) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let handle = app.spawn(config.clone(), rx);
        Self {
            app,
            handle,
            shutdown_tx: tx,
            config,
            crash_count: 0,
            next_restart: None,
        }
    }

    fn is_crashed(&self) -> bool {
        self.handle.is_finished()
    }

    fn restart_ready(&self) -> bool {
        self.next_restart.is_none_or(|t| Instant::now() >= t)
    }

    fn restart(&mut self, log: &Logger) {
        if !self.restart_ready() {
            return;
        }
        self.crash_count += 1;
        // Exponential backoff: 2s, 5s, 15s, then cap at 60s.
        let delay_secs = match self.crash_count {
            1 => 2,
            2 => 5,
            3 => 15,
            _ => 60,
        };
        log.log(&format!(
            "[{}] Thread crashed (#{}) — restarting in {}s",
            self.app.name(),
            self.crash_count,
            delay_secs
        ));
        self.next_restart = Some(Instant::now() + Duration::from_secs(delay_secs));
        let (tx, rx) = mpsc::channel::<()>();
        self.shutdown_tx = tx;
        self.handle = self.app.spawn(self.config.clone(), rx);
    }

    // Send shutdown, then wait up to 5 seconds before detaching to avoid
    // blocking the orchestrator on a stuck thread.
    fn stop(self, log: &Logger) {
        let _ = self.shutdown_tx.send(());
        let name = self.app.name();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.handle.is_finished() {
            if Instant::now() >= deadline {
                log.log(&format!("[{name}] Thread still exiting — detaching"));
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.handle.join();
    }
}

// ── Spawn helpers ─────────────────────────────────────────────────────────────

fn spawn_teleport_target(config: Config, rx: Receiver<()>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("iRacing Teleport Target".to_string())
        .spawn(move || {
            if let Err(e) = teleport::run_target(
                teleport::TargetConfig {
                    bind: format!("0.0.0.0:{}", config.ports.iracing_teleport),
                    unicast: config.network.unicast,
                    fanalab: config.apps.fanalab,
                    high_priority: config.apps.high_priority,
                    busy_wait: config.apps.busy_wait,
                    stale_timeout_secs: config.advanced.stale_timeout_secs,
                    ..teleport::TargetConfig::default()
                },
                rx,
            ) {
                eprintln!("[iRacing Teleport Target] {e}");
            }
        })
        .expect("failed to spawn iRacing Teleport target thread")
}

fn spawn_ac_target(config: Config, rx: Receiver<()>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("AC Teleport Target".to_string())
        .spawn(move || {
            // game: None = dual mode — creates shared maps for both EVO and AC1
            // simultaneously, so the target handles any AC variant without restart.
            if let Err(e) = ac_teleport::target::run(
                ac_teleport::TargetArgs {
                    game: None,
                    bind: format!("0.0.0.0:{}", config.ports.ac_teleport),
                    group: teleport::DEFAULT_MULTICAST.to_string(),
                    unicast: config.network.unicast,
                    busy_wait: config.apps.busy_wait,
                    pin_core: None,
                    high_priority: config.apps.high_priority,
                    stale_timeout: std::time::Duration::from_secs(
                        config.advanced.stale_timeout_secs,
                    ),
                },
                rx,
            ) {
                eprintln!("[AC Teleport Target] {e}");
            }
        })
        .expect("failed to spawn AC Teleport target thread")
}

fn spawn_relay_target(config: Config, rx: Receiver<()>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("Sim Relay Target".to_string())
        .spawn(move || {
            if let Err(e) = sim_relay::target::run(
                sim_relay::TargetArgs {
                    source: Some(config.network.source_ip.clone()),
                    games: None,
                    all: true,
                    forward_to: None,
                    high_priority: config.apps.high_priority,
                    busy_wait: config.apps.busy_wait,
                },
                rx,
            ) {
                eprintln!("[Sim Relay Target] {e}");
            }
        })
        .expect("failed to spawn Sim Relay target thread")
}

// ── Main target loop ──────────────────────────────────────────────────────────

pub fn run(config: Config, log: &Logger, shutdown: Receiver<()>) {
    // Issue #6: only start threads for enabled apps.
    // Issue #7: thread high_priority / busy_wait from config.
    let mut slots: Vec<TargetSlot> = Vec::new();

    if config.apps.iracing_teleport_enabled {
        log.log(&format!(
            "  iRacing Teleport  :{}",
            config.ports.iracing_teleport
        ));
        slots.push(TargetSlot::new(TargetApp::IracingTeleport, config.clone()));
    }

    if config.apps.ac_teleport_enabled {
        log.log(&format!(
            "  AC Teleport       :{}",
            config.ports.ac_teleport
        ));
        slots.push(TargetSlot::new(TargetApp::AcTeleport, config.clone()));
    }

    if config.apps.sim_relay_enabled {
        log.log("  Sim Relay          all game ports");
        slots.push(TargetSlot::new(TargetApp::SimRelay, config.clone()));
    }

    if slots.is_empty() {
        log.log("No apps enabled — nothing to do. Check sim-bridge.toml.");
        return;
    }

    log.log("Waiting for telemetry...");

    // Issue #8: health-monitoring loop — check every 10 s, restart crashed threads.
    loop {
        match shutdown.recv_timeout(Duration::from_secs(10)) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                for slot in &mut slots {
                    if slot.is_crashed() {
                        slot.restart(log);
                    }
                }
            }
        }
    }

    log.log("Shutting down...");
    for slot in slots {
        slot.stop(log);
    }
    log.log("All apps stopped. Goodbye.");
}
