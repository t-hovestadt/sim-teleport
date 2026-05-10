use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::logger::Logger;

use super::{Detection, ShmemGame};

// ── Error recovery ────────────────────────────────────────────────────────────

pub(super) struct FailureTracker {
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

    pub(super) fn record(&mut self, log: &Logger, app: &str) {
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

    pub(super) fn reset(&mut self) {
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

pub(super) enum SlotState {
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

pub(super) struct AppSlot {
    name: &'static str,
    pub(super) state: SlotState,
    shutdown_tx: Option<Sender<()>>,
    pub(super) failures: FailureTracker,
    detached: Option<DetachedThread>,
    /// Consecutive scan cycles where the same game was re-detected during Draining.
    /// Drain is only cancelled after 3 consecutive hits to filter out process flicker
    /// (acs.exe briefly reappearing in the process list during AC's shutdown sequence).
    pub(super) consecutive_redetections: u32,
    /// Consecutive scan cycles where the game process was not found while in Running state.
    /// Drain only begins after 3 consecutive misses to absorb brief process-list blips
    /// during AC session transitions / loading screens.
    pub(super) consecutive_gone: u32,
    /// Set by cancel_drain() — the thread will exit soon because its shutdown signal was
    /// already sent during the preceding process-gone stop. The next is_running_finished()
    /// event is an expected clean exit from a session transition, not a crash; skip
    /// failures.record() for it.
    pub(super) expect_thread_exit: bool,
}

impl AppSlot {
    pub(super) fn new(name: &'static str) -> Self {
        Self {
            name,
            state: SlotState::Idle,
            shutdown_tx: None,
            failures: FailureTracker::new(),
            detached: None,
            consecutive_redetections: 0,
            consecutive_gone: 0,
            expect_thread_exit: false,
        }
    }

    pub(super) fn start_shmem(
        &mut self,
        detection: &Detection,
        config: &Config,
        log: &Logger,
    ) -> bool {
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
        let verbose = config.verbose;

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
                            verbose,
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
                            // sim-teleport owns detection and manages the lifecycle.
                            force_bind: true,
                            all: false,
                            local_forward: false,
                            // Bind to all interfaces so we catch games that send UDP to
                            // 127.0.0.1 (most games' default) as well as source_ip.
                            // Binding source_ip alone misses localhost-addressed packets.
                            bind: "0.0.0.0".to_string(),
                            high_priority: cfg.apps.high_priority,
                            scan_interval: cfg.detection.scan_interval,
                            grace_period: cfg.detection.drain_seconds,
                            include_console: false,
                            port_offset: cfg.apps.relay_port_offset,
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
        self.consecutive_gone = 0;
        self.expect_thread_exit = false;
        true
    }

    pub(super) fn begin_drain(&mut self) {
        let state = std::mem::replace(&mut self.state, SlotState::Idle);
        if let SlotState::Running { handle, game } = state {
            self.state = SlotState::Draining {
                since: Instant::now(),
                handle,
                game,
            };
        }
    }

    pub(super) fn cancel_drain(&mut self) {
        let state = std::mem::replace(&mut self.state, SlotState::Idle);
        if let SlotState::Draining { handle, game, .. } = state {
            self.state = SlotState::Running { handle, game };
            // The thread already received its shutdown signal during the process-gone
            // stop that triggered this drain. When it exits, that's an expected clean
            // transition — not a crash.
            self.expect_thread_exit = true;
        }
    }

    // Send shutdown signal then wait up to 5 seconds. If the thread is still
    // blocked (e.g. iRacing WaitForSingleObject hasn't fired), detach rather
    // than hanging the orchestrator — the thread will exit on its own once the
    // OS timeout fires (issue #4).
    pub(super) fn stop(&mut self, log: &Logger) {
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

    pub(super) fn current_game(&self) -> Option<ShmemGame> {
        match &self.state {
            SlotState::Running { game, .. } | SlotState::Draining { game, .. } => Some(*game),
            _ => None,
        }
    }

    pub(super) fn draining_game(&self) -> Option<ShmemGame> {
        match &self.state {
            SlotState::Draining { game, .. } => Some(*game),
            _ => None,
        }
    }

    pub(super) fn drain_since(&self) -> Option<Instant> {
        match &self.state {
            SlotState::Draining { since, .. } => Some(*since),
            _ => None,
        }
    }

    pub(super) fn is_running_finished(&self) -> bool {
        match &self.state {
            SlotState::Running { handle, .. } => handle.is_finished(),
            _ => false,
        }
    }

    pub(super) fn send_shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    pub(super) fn state_label(&self) -> &'static str {
        match &self.state {
            SlotState::Idle => "idle",
            SlotState::Running { .. } => "running",
            SlotState::Draining { .. } => "draining",
        }
    }
}
