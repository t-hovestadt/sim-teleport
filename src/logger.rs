use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
const LOG_FILENAME: &str = "sim-bridge.log";

#[derive(Clone)]
pub struct Logger {
    file: Option<Arc<Mutex<BufWriter<File>>>>,
}

impl Logger {
    /// Open the log file next to the executable. Truncates if >= 10 MB.
    pub fn open() -> anyhow::Result<Self> {
        let path = log_path();
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() >= MAX_LOG_BYTES {
                let _ = std::fs::write(&path, "");
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            file: Some(Arc::new(Mutex::new(BufWriter::new(file)))),
        })
    }

    /// Fallback logger that only writes to stdout (no file).
    pub fn stderr() -> Self {
        Self { file: None }
    }

    pub fn log(&self, msg: &str) {
        let now = Local::now();
        let console_ts = now.format("%H:%M:%S");
        let line = format!("[{console_ts}] {msg}");
        println!("{line}");
        if let Some(file) = &self.file {
            if let Ok(mut w) = file.lock() {
                let file_ts = now.format("%Y-%m-%d %H:%M:%S");
                let _ = writeln!(w, "[{file_ts}] {msg}");
                let _ = w.flush();
            }
        }
    }
}

fn log_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(LOG_FILENAME)))
        .unwrap_or_else(|| PathBuf::from(LOG_FILENAME))
}
