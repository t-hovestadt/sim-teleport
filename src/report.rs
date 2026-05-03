use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;

use chrono::{DateTime, Local};

pub struct GameSession {
    pub app: &'static str,
    pub label: String,
    pub how: &'static str,
    pub started: DateTime<Local>,
    pub stopped: Option<DateTime<Local>>,
    pub stop_reason: &'static str,
}

pub struct SessionReport {
    pub version_string: String,
    pub start_time: DateTime<Local>,
    pub config_summary: String,

    // Detection counters
    pub total_scans: u64,
    pub iracing_probes: u64,
    pub iracing_hits: u64,
    pub ac_evo_probes: u64,
    pub ac_evo_live: u64,
    pub ac_evo_stale: u64,
    pub ac_evo_tiebreaker: u64,
    pub ac_evo_tiebreaker_hits: u64,
    pub ac1_probes: u64,
    pub ac1_live: u64,
    pub ac1_stale: u64,
    pub ac1_tiebreaker: u64,
    pub ac1_tiebreaker_hits: u64,
    pub process_scans: u64,
    pub process_scan_matches: u64,

    pub sessions: Vec<GameSession>,
    pub detection_log: VecDeque<String>,
    pub errors: Vec<String>,
}

impl SessionReport {
    pub fn new(version_string: String, config_summary: String) -> Self {
        Self {
            version_string,
            start_time: Local::now(),
            config_summary,
            total_scans: 0,
            iracing_probes: 0,
            iracing_hits: 0,
            ac_evo_probes: 0,
            ac_evo_live: 0,
            ac_evo_stale: 0,
            ac_evo_tiebreaker: 0,
            ac_evo_tiebreaker_hits: 0,
            ac1_probes: 0,
            ac1_live: 0,
            ac1_stale: 0,
            ac1_tiebreaker: 0,
            ac1_tiebreaker_hits: 0,
            process_scans: 0,
            process_scan_matches: 0,
            sessions: Vec::new(),
            detection_log: VecDeque::new(),
            errors: Vec::new(),
        }
    }

    pub fn push_note(&mut self, note: String) {
        if self.detection_log.len() >= 50 {
            self.detection_log.pop_front();
        }
        self.detection_log.push_back(note);
    }

    pub fn start_session(&mut self, app: &'static str, label: &str, how: &'static str) {
        self.sessions.push(GameSession {
            app,
            label: label.to_string(),
            how,
            started: Local::now(),
            stopped: None,
            stop_reason: "",
        });
    }

    pub fn end_session(&mut self, reason: &'static str) {
        if let Some(s) = self.sessions.last_mut() {
            if s.stopped.is_none() {
                s.stopped = Some(Local::now());
                s.stop_reason = reason;
            }
        }
    }

    pub fn write(&self) {
        let path = report_path();
        match std::fs::File::create(&path) {
            Ok(mut f) => {
                if let Err(e) = write_to(&mut f, self) {
                    eprintln!("Warning: could not write session report: {e}");
                }
            }
            Err(e) => eprintln!("Warning: could not create session report: {e}"),
        }
    }
}

fn report_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("sim-bridge-report.txt")))
        .unwrap_or_else(|| PathBuf::from("sim-bridge-report.txt"))
}

fn write_to(f: &mut impl Write, r: &SessionReport) -> std::io::Result<()> {
    let now = Local::now();
    let end = r
        .sessions
        .iter()
        .rev()
        .find_map(|s| s.stopped)
        .unwrap_or(now);
    let duration_secs = (now - r.start_time).num_seconds().max(0) as u64;

    writeln!(f, "=== sim-bridge session report ===")?;
    writeln!(f, "Version: {}", r.version_string)?;
    writeln!(f, "Mode: source")?;
    writeln!(f, "Config: {}", r.config_summary)?;
    writeln!(f, "Start: {}", r.start_time.format("%Y-%m-%d %H:%M:%S"))?;
    writeln!(f, "End:   {}", end.format("%Y-%m-%d %H:%M:%S"))?;
    writeln!(f, "Duration: {}", format_duration(duration_secs))?;
    writeln!(f)?;

    writeln!(f, "=== Detection summary ===")?;
    writeln!(f, "Total scan cycles: {}", r.total_scans)?;
    if r.iracing_probes > 0 {
        writeln!(
            f,
            "  iRacing event probes: {} ({} hits, {} misses)",
            r.iracing_probes,
            r.iracing_hits,
            r.iracing_probes - r.iracing_hits
        )?;
    }
    if r.ac_evo_probes > 0 {
        writeln!(
            f,
            "  AC EVO shmem probes: {} ({} found: {} live, {} stale)",
            r.ac_evo_probes,
            r.ac_evo_live + r.ac_evo_stale,
            r.ac_evo_live,
            r.ac_evo_stale
        )?;
    }
    if r.ac_evo_tiebreaker > 0 {
        writeln!(
            f,
            "  AC EVO tiebreaker: {} ({} process found — {} ghost maps)",
            r.ac_evo_tiebreaker,
            r.ac_evo_tiebreaker_hits,
            r.ac_evo_tiebreaker - r.ac_evo_tiebreaker_hits
        )?;
    }
    if r.ac1_probes > 0 {
        writeln!(
            f,
            "  AC1 shmem probes: {} ({} found: {} live, {} stale)",
            r.ac1_probes,
            r.ac1_live + r.ac1_stale,
            r.ac1_live,
            r.ac1_stale
        )?;
    }
    if r.ac1_tiebreaker > 0 {
        writeln!(
            f,
            "  AC1 tiebreaker: {} ({} process found, {} ghost)",
            r.ac1_tiebreaker,
            r.ac1_tiebreaker_hits,
            r.ac1_tiebreaker - r.ac1_tiebreaker_hits
        )?;
    }
    if r.process_scans > 0 {
        writeln!(
            f,
            "  Process scans: {} ({} sim-relay match{})",
            r.process_scans,
            r.process_scan_matches,
            if r.process_scan_matches == 1 {
                ""
            } else {
                "es"
            }
        )?;
    }
    writeln!(f)?;

    writeln!(f, "=== Game sessions ===")?;
    if r.sessions.is_empty() {
        writeln!(f, "(none)")?;
    }
    for (i, s) in r.sessions.iter().enumerate() {
        let duration = if let Some(stopped) = s.stopped {
            format_duration((stopped - s.started).num_seconds().max(0) as u64)
        } else {
            "still running".to_string()
        };
        writeln!(f, "[{}] {} ({})", i + 1, s.app, s.label)?;
        writeln!(
            f,
            "    Started: {} ({})",
            s.started.format("%H:%M:%S"),
            s.how
        )?;
        if let Some(stopped) = s.stopped {
            writeln!(
                f,
                "    Stopped: {} ({})",
                stopped.format("%H:%M:%S"),
                s.stop_reason
            )?;
        } else {
            writeln!(f, "    Stopped: still running")?;
        }
        writeln!(f, "    Duration: {duration}")?;
    }
    writeln!(f)?;

    writeln!(f, "=== Errors and warnings ===")?;
    if r.errors.is_empty() {
        writeln!(f, "(none)")?;
    } else {
        for e in &r.errors {
            writeln!(f, "{e}")?;
        }
    }
    writeln!(f)?;

    writeln!(
        f,
        "=== Detection details (last {}) ===",
        r.detection_log.len()
    )?;
    if r.detection_log.is_empty() {
        writeln!(f, "(none)")?;
    } else {
        for note in &r.detection_log {
            writeln!(f, "{note}")?;
        }
    }

    Ok(())
}

fn format_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}
