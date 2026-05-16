use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::logger::Logger;
use crate::report::SessionReport;
use crate::scanner::ProcessScanner;

mod detection;
mod slot;
mod wreckfest;

use detection::{is_game_still_running, run_detection_cycle};
use slot::{AppSlot, SlotState};
use wreckfest::ensure_wreckfest_telemetry_config;

// ── Game identity ─────────────────────────────────────────────────────────────

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

fn game_label(game: Option<ShmemGame>) -> &'static str {
    match game {
        Some(ShmemGame::Iracing) => "iRacing Teleport",
        Some(ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc) => "AC Teleport",
        Some(ShmemGame::SimRelay { name, .. }) => name,
        None => "app",
    }
}

// ── Main source loop ──────────────────────────────────────────────────────────

pub fn run(config: Config, log: &Logger, shutdown: Receiver<()>, version_string: &str) {
    if config.network.source_ip == config.network.target_ip {
        log.log("ERROR: source_ip and target_ip are the same. source mode runs on the gaming PC, target mode runs on the SimHub PC. Check sim-teleport.toml.");
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

    // Auto-create Wreckfest 2 telemetry config if the game's save directory already exists.
    // Called again on detection in case the game was first run after source started.
    ensure_wreckfest_telemetry_config(log);

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
                // Only record a failure for unexpected thread exits. When
                // expect_thread_exit is set the thread was shut down explicitly
                // during an iRacing session-transition (process-gone → drain →
                // cancel_drain). That's a clean cycle, not a crash.
                if shmem.expect_thread_exit {
                    shmem.expect_thread_exit = false;
                } else {
                    shmem.failures.record(log, name);
                }
                shmem.begin_drain();
            } else if let Some(game) = shmem.current_game() {
                if !is_game_still_running(game, &mut scanner, log) {
                    shmem.consecutive_gone += 1;
                    let gone_threshold = match game {
                        ShmemGame::Iracing => 10, // 30s at 3s scan interval for session transitions
                        _ => 3,
                    };
                    if shmem.consecutive_gone >= gone_threshold {
                        let name = game_label(Some(game));
                        log.log(&format!(
                            "[{name}] Game process gone ({} consecutive scans) — stopping",
                            shmem.consecutive_gone
                        ));
                        report.push_note(format!(
                            "{} {} process gone",
                            chrono::Local::now().format("%H:%M:%S"),
                            name
                        ));
                        shmem.consecutive_gone = 0;
                        shmem.send_shutdown();
                        std::thread::sleep(Duration::from_millis(500));
                        shmem.begin_drain();
                    }
                } else {
                    shmem.consecutive_gone = 0;
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
                    // On first Wreckfest 2 detection, ensure telemetry config exists
                    // (handles the case where the game was first launched after source started).
                    if let ShmemGame::SimRelay {
                        id: "wreckfest2", ..
                    } = d.game
                    {
                        ensure_wreckfest_telemetry_config(log);
                    }
                    let app = match d.game {
                        ShmemGame::Iracing => "iRacing Teleport",
                        ShmemGame::AcEvo | ShmemGame::Ac1 | ShmemGame::Acc => "AC Teleport",
                        ShmemGame::SimRelay { .. } => "Sim Relay",
                    };
                    if shmem.start_shmem(d, &config, log) {
                        report.start_session(app, d.label, d.how);
                    }
                }

                // Same game re-detected while draining — require 3 consecutive hits before
                // cancelling. A single blip (acs.exe flickering during shutdown) won't count.
                (SlotState::Draining { .. }, Some(d)) if draining_game == Some(d.game) => {
                    shmem.consecutive_redetections += 1;
                    if shmem.consecutive_redetections >= 3 {
                        log.log(&format!(
                            "[{}] Game confirmed back ({} scans) — cancelling shutdown",
                            d.label, shmem.consecutive_redetections
                        ));
                        shmem.consecutive_redetections = 0;
                        shmem.cancel_drain();
                    }
                }

                // Different game detected while draining — stop old, start new.
                (SlotState::Draining { .. }, Some(d)) => {
                    shmem.consecutive_redetections = 0;
                    shmem.consecutive_gone = 0;
                    let old_name = game_label(shmem.current_game());
                    log.log(&format!("[{old_name}] Stopping (switching to {})", d.label));
                    {
                        let (msgs, bytes, lat) = *shmem.session_stats.lock().unwrap();
                        report.update_session_stats(msgs, bytes, lat);
                    }
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
                    shmem.consecutive_redetections = 0;
                    shmem.consecutive_gone = 0;
                    let name = game_label(shmem.current_game());
                    log.log(&format!("[{name}] Stopped"));
                    {
                        let (msgs, bytes, lat) = *shmem.session_stats.lock().unwrap();
                        report.update_session_stats(msgs, bytes, lat);
                    }
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
    {
        let (msgs, bytes, lat) = *shmem.session_stats.lock().unwrap();
        report.update_session_stats(msgs, bytes, lat);
    }
    shmem.stop(log);
    report.end_session("Ctrl-C shutdown");
    report.write();
    log.log("All apps stopped. Goodbye.");
}
