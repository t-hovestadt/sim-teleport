use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::config::Config;
use crate::logger::Logger;

// ── Per-thread slot ───────────────────────────────────────────────────────────

struct TargetSlot {
    name: &'static str,
    handle: JoinHandle<()>,
    shutdown_tx: Sender<()>,
    spawn: Box<dyn Fn(Config, Receiver<()>) -> JoinHandle<()> + Send>,
    config: Config,
    crash_count: u32,
}

impl TargetSlot {
    fn is_crashed(&self) -> bool {
        self.handle.is_finished()
    }

    fn restart(&mut self, log: &Logger) {
        self.crash_count += 1;
        log.log(&format!(
            "[{}] Thread crashed (#{}) — restarting",
            self.name, self.crash_count
        ));
        let (tx, rx) = mpsc::channel::<()>();
        self.shutdown_tx = tx;
        self.handle = (self.spawn)(self.config.clone(), rx);
    }

    fn stop(self) {
        let _ = self.shutdown_tx.send(());
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
                    unicast: true,
                    high_priority: config.apps.high_priority,
                    busy_wait: config.apps.busy_wait,
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
                    unicast: true,
                    busy_wait: config.apps.busy_wait,
                    pin_core: None,
                    high_priority: config.apps.high_priority,
                    stale_timeout: std::time::Duration::from_secs(10),
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
        log.log(&format!("  iRacing Teleport  :{}", config.ports.iracing_teleport));
        let (tx, rx) = mpsc::channel::<()>();
        let cfg = config.clone();
        slots.push(TargetSlot {
            name: "iRacing Teleport Target",
            handle: spawn_teleport_target(config.clone(), rx),
            shutdown_tx: tx,
            spawn: Box::new(|c, r| spawn_teleport_target(c, r)),
            config: cfg,
            crash_count: 0,
        });
    }

    if config.apps.ac_teleport_enabled {
        log.log(&format!("  AC Teleport       :{}", config.ports.ac_teleport));
        let (tx, rx) = mpsc::channel::<()>();
        let cfg = config.clone();
        slots.push(TargetSlot {
            name: "AC Teleport Target",
            handle: spawn_ac_target(config.clone(), rx),
            shutdown_tx: tx,
            spawn: Box::new(|c, r| spawn_ac_target(c, r)),
            config: cfg,
            crash_count: 0,
        });
    }

    if config.apps.sim_relay_enabled {
        log.log("  Sim Relay          all game ports");
        let (tx, rx) = mpsc::channel::<()>();
        let cfg = config.clone();
        slots.push(TargetSlot {
            name: "Sim Relay Target",
            handle: spawn_relay_target(config.clone(), rx),
            shutdown_tx: tx,
            spawn: Box::new(|c, r| spawn_relay_target(c, r)),
            config: cfg,
            crash_count: 0,
        });
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
        slot.stop();
    }
    log.log("All apps stopped. Goodbye.");
}
