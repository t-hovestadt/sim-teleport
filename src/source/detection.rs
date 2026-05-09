use crate::config::Config;
use crate::logger::Logger;
use crate::report::SessionReport;
use crate::scanner::{probe_ac_maps, AcProbeResult, ProcessScanner, ShmemDetection};

use super::{Detection, ShmemGame};

// ── Scan context (passed to detection functions) ──────────────────────────────

struct ScanCtx<'a> {
    verbose: bool,
    log: &'a Logger,
    report: &'a mut SessionReport,
}

// ── Game detection ────────────────────────────────────────────────────────────

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

/// Returns false when the game process is no longer running.
/// Called every scan cycle while Running to detect game closure across all game types.
pub(super) fn is_game_still_running(
    game: ShmemGame,
    scanner: &mut ProcessScanner,
    _log: &Logger,
) -> bool {
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
pub(super) fn run_detection_cycle(
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
