use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

type DataCb = Arc<dyn Fn() + Send + Sync>;
type RelayGameCb = Arc<dyn Fn(&str, bool) + Send + Sync>;

use crate::config::Config;
use crate::logger::Logger;
use crate::simhub_setup;
use crate::stub::{self, StubManager};

// ── SimHub auto-switching ─────────────────────────────────────────────────────

struct ActiveGameTracker {
    current: Mutex<Option<String>>,
}

impl ActiveGameTracker {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            current: Mutex::new(None),
        })
    }

    fn try_activate(&self, game_code: &str, simhub_path: Option<&str>) {
        let mut guard = self.current.lock().unwrap();
        if guard.as_deref() == Some(game_code) {
            return;
        }
        *guard = Some(game_code.to_string());
        drop(guard);
        switch_simhub_game(game_code, simhub_path);
    }

    fn deactivate(&self) {
        *self.current.lock().unwrap() = None;
    }
}

#[cfg(windows)]
fn switch_simhub_game(game_code: &str, simhub_path: Option<&str>) {
    let exe = simhub_path.unwrap_or("C:/Program Files (x86)/SimHub/SimHubWPF.exe");
    match std::process::Command::new(exe)
        .arg("-switchgame")
        .arg(game_code)
        .spawn()
    {
        Ok(_) => println!("[simhub] switched to {game_code}"),
        Err(e) => eprintln!("[simhub] failed to switch to {game_code}: {e}"),
    }
}

#[cfg(not(windows))]
fn switch_simhub_game(game_code: &str, _simhub_path: Option<&str>) {
    println!("[simhub] would switch to {game_code} (no-op on non-Windows)");
}

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

    fn spawn(
        self,
        config: Config,
        rx: Receiver<()>,
        on_first_data: Option<DataCb>,
        on_stale: Option<DataCb>,
        relay_on_game: Option<RelayGameCb>,
    ) -> JoinHandle<()> {
        match self {
            TargetApp::IracingTeleport => {
                spawn_teleport_target(config, rx, on_first_data, on_stale)
            }
            TargetApp::AcTeleport => spawn_ac_target(config, rx, on_first_data, on_stale),
            TargetApp::SimRelay => spawn_relay_target(config, rx, relay_on_game),
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
    on_first_data: Option<DataCb>,
    on_stale: Option<DataCb>,
    relay_on_game: Option<RelayGameCb>,
}

impl TargetSlot {
    fn new(
        app: TargetApp,
        config: Config,
        on_first_data: Option<DataCb>,
        on_stale: Option<DataCb>,
        relay_on_game: Option<RelayGameCb>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let handle = app.spawn(
            config.clone(),
            rx,
            on_first_data.clone(),
            on_stale.clone(),
            relay_on_game.clone(),
        );
        Self {
            app,
            handle,
            shutdown_tx: tx,
            config,
            crash_count: 0,
            next_restart: None,
            on_first_data,
            on_stale,
            relay_on_game,
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
        self.handle = self.app.spawn(
            self.config.clone(),
            rx,
            self.on_first_data.clone(),
            self.on_stale.clone(),
            self.relay_on_game.clone(),
        );
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

fn spawn_teleport_target(
    config: Config,
    rx: Receiver<()>,
    on_first_data: Option<DataCb>,
    on_stale: Option<DataCb>,
) -> JoinHandle<()> {
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
                    on_first_data,
                    on_stale,
                    ..teleport::TargetConfig::default()
                },
                rx,
            ) {
                eprintln!("[iRacing Teleport Target] {e}");
            }
        })
        .expect("failed to spawn iRacing Teleport target thread")
}

fn spawn_ac_target(
    config: Config,
    rx: Receiver<()>,
    on_first_data: Option<DataCb>,
    on_stale: Option<DataCb>,
) -> JoinHandle<()> {
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
                    on_first_data,
                    on_stale,
                },
                rx,
            ) {
                eprintln!("[AC Teleport Target] {e}");
            }
        })
        .expect("failed to spawn AC Teleport target thread")
}

fn spawn_relay_target(
    config: Config,
    rx: Receiver<()>,
    on_game_active: Option<RelayGameCb>,
) -> JoinHandle<()> {
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
                    on_game_active,
                    port_offset: config.apps.relay_port_offset,
                },
                rx,
            ) {
                eprintln!("[Sim Relay Target] {e}");
            }
        })
        .expect("failed to spawn Sim Relay target thread")
}

/// Built-in SimHub game codes for sim-relay games, used when [simhub.relay] has no entry.
/// Config takes priority: if simhub.relay.<id> is set, that overrides this table.
fn builtin_relay_simhub_code(id: &str) -> Option<&'static str> {
    match id {
        "wreckfest2" => Some("Wreckfest2"),
        "f1-25" => Some("F12025"),
        "f1-24" => Some("F12024"),
        "f1-23" => Some("F12023"),
        "f1-22" => Some("F12022"),
        "f1-21" => Some("F12021"),
        "f1-20" => Some("F12020"),
        "dirt-rally2" => Some("DirtRally2"),
        "dirt5" => Some("Dirt5"),
        "wrc-24" => Some("WRC2024"),
        "wrc-23" => Some("WRC2023"),
        "beamng-sh" | "beamng-outgauge" => Some("BeamNGDrive"),
        "pcars2" | "kartkraft" => Some("ProjectCars2"),
        "ams2" => Some("AMS2"),
        "forza-fm7" => Some("ForzaMotorsport7"),
        "forza-fh4" => Some("ForzaHorizon4"),
        "forza-fh5" => Some("ForzaHorizon5"),
        "forza-fm" => Some("ForzaMotorsport"),
        _ => None,
    }
}

// ── Main target loop ──────────────────────────────────────────────────────────

pub fn run(config: Config, log: &Logger, shutdown: Receiver<()>) {
    // Auto-configure SimHub's PluginsData on the target PC so games like AC
    // are marked as configured even though they're not installed here.
    simhub_setup::setup_simhub_for_target(config.simhub.path.as_deref());

    // Issue #6: only start threads for enabled apps.
    // Issue #7: thread high_priority / busy_wait from config.
    let mut slots: Vec<TargetSlot> = Vec::new();

    let tracker = ActiveGameTracker::new();
    let simhub_path = config.simhub.path.clone();
    let iracing_code = config.simhub.iracing.clone();
    let ac_code = config.simhub.ac.clone();

    let t1 = tracker.clone();
    let p1 = simhub_path.clone();
    let c1 = iracing_code.clone();
    let iracing_on_first: DataCb = Arc::new(move || t1.try_activate(&c1, p1.as_deref()));
    let t2 = tracker.clone();
    let iracing_on_stale: DataCb = Arc::new(move || t2.deactivate());

    let stub_mgr = Arc::new(Mutex::new(StubManager::new(log.clone())));

    // Create fake install directories and Steam registry entries at startup.
    // Stub processes are spawned on demand when data arrives, not here.
    if config.apps.ac_teleport_enabled {
        let stub_dir = std::env::temp_dir().join("sim-bridge-stubs");
        std::fs::create_dir_all(&stub_dir).ok();
        stub::setup_all_game_environments(&stub_dir, log);
        stub::setup_game_registry(&stub_dir, log);
    }

    let t3 = tracker.clone();
    let p3 = simhub_path.clone();
    let c3 = ac_code.clone();
    let sm_first = stub_mgr.clone();
    let ac_on_first: DataCb = Arc::new(move || {
        // Spawn AC1 stub only — ac-teleport target writes both AC1 and EVO maps
        // simultaneously so the protocol carries no variant field. AC1 is the
        // tested case; EVO stub spawning is deferred until variant detection is added.
        sm_first.lock().unwrap().ensure_running("acs");
        t3.try_activate(&c3, p3.as_deref());
    });
    let t4 = tracker.clone();
    let sm_stale = stub_mgr.clone();
    let ac_on_stale: DataCb = Arc::new(move || {
        sm_stale.lock().unwrap().kill("acs");
        // Kill EVO stub defensively in case it was spawned by a future code path.
        sm_stale.lock().unwrap().kill("assettocorsa_evo");
        t4.deactivate();
    });

    let t5 = tracker;
    let relay_codes = config.simhub.relay.clone();
    let simhub_path_relay = simhub_path.clone();
    let relay_on_game: RelayGameCb = Arc::new(move |id: &str, active: bool| {
        if active {
            // Config overrides built-in table; built-in table is the fallback.
            let code = relay_codes
                .get(id)
                .map(|s| s.as_str())
                .or_else(|| builtin_relay_simhub_code(id));
            if let Some(code) = code {
                t5.try_activate(code, simhub_path_relay.as_deref());
            }
        } else {
            t5.deactivate();
        }
    });

    if config.apps.iracing_teleport_enabled {
        log.log(&format!(
            "  iRacing Teleport  :{}",
            config.ports.iracing_teleport
        ));
        slots.push(TargetSlot::new(
            TargetApp::IracingTeleport,
            config.clone(),
            Some(iracing_on_first),
            Some(iracing_on_stale),
            None,
        ));
    }

    if config.apps.ac_teleport_enabled {
        log.log(&format!(
            "  AC Teleport       :{}",
            config.ports.ac_teleport
        ));
        slots.push(TargetSlot::new(
            TargetApp::AcTeleport,
            config.clone(),
            Some(ac_on_first),
            Some(ac_on_stale),
            None,
        ));
    }

    if config.apps.sim_relay_enabled {
        log.log("  Sim Relay          all game ports");
        slots.push(TargetSlot::new(
            TargetApp::SimRelay,
            config.clone(),
            None,
            None,
            Some(relay_on_game),
        ));
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
