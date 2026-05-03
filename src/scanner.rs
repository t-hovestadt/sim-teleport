use std::collections::HashSet;
#[cfg(windows)]
use std::time::Duration;

use crate::logger::Logger;

/// Snapshot of running process names, built once per scan cycle.
/// All names are stored lowercased for case-insensitive matching.
pub struct ProcessScanner {
    snapshot: HashSet<String>,
}

impl Default for ProcessScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessScanner {
    pub fn new() -> Self {
        Self {
            snapshot: HashSet::new(),
        }
    }

    /// Rebuild the snapshot from the current process list.
    /// ~1 ms on Windows; no-op on non-Windows (source mode is Windows-only).
    pub fn refresh(&mut self) {
        self.snapshot.clear();
        #[cfg(windows)]
        self.refresh_windows();
    }

    /// Returns true if any of the given exe names (case-insensitive) is currently running.
    pub fn is_running(&self, names: &[&str]) -> bool {
        names
            .iter()
            .any(|n| self.snapshot.contains(&n.to_lowercase()))
    }

    pub fn process_count(&self) -> usize {
        self.snapshot.len()
    }

    #[cfg(windows)]
    fn refresh_windows(&mut self) {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    let null_pos = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name =
                        String::from_utf16_lossy(&entry.szExeFile[..null_pos]).to_lowercase();
                    self.snapshot.insert(name);
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);
        }
    }
}

// ── iRacing event probe (fast-negative only) ─────────────────────────────────

/// Fast check: if the event doesn't exist, iRacing is definitely not running.
/// BUT: if the event DOES exist, iRacing may still not be running — our own
/// teleport thread, FanaLab, and other telemetry apps hold event handles after
/// iRacing exits, keeping the event alive (ghost handle deadlock).
/// NEVER use this as the sole detection method. Use process name scanning instead.
#[allow(dead_code)]
#[cfg(windows)]
pub fn iracing_event_exists() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::OpenEventW;

    const SYNCHRONIZE: u32 = 0x0010_0000;
    let name: Vec<u16> = "Local\\IRSDKDataValidEvent"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let handle = OpenEventW(SYNCHRONIZE, 0, name.as_ptr());
        if handle == 0 {
            false
        } else {
            CloseHandle(handle);
            true
        }
    }
}

#[allow(dead_code)]
#[cfg(not(windows))]
pub fn iracing_event_exists() -> bool {
    false
}

// ── AC shared-memory probing ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum ShmemDetection {
    /// packetId advanced between two reads — game is in an active session.
    Live,
    /// Maps exist but packetId did not advance — game is on menu or maps are stale.
    Stale,
}

pub struct AcProbeResult {
    pub ac_evo: Option<ShmemDetection>,
    pub ac1: Option<ShmemDetection>,
}

/// Probe both AC map families. Opens the physics map, reads packetId at T0,
/// sleeps 100 ms, reads again at T1, then closes the handle.
/// Returns `Live` if packetId changed, `Stale` if maps exist but packetId is static,
/// `None` if maps don't exist.
pub fn probe_ac_maps(verbose: bool, log: &Logger) -> AcProbeResult {
    let evo_raw = probe_map("Local\\acevo_pmf_physics");
    let ac1_raw = probe_map("Local\\acpmf_physics");

    if verbose {
        match evo_raw {
            Some((ShmemDetection::Live, id0, id1)) => {
                log.log(&format!(
                    "[scan] AC EVO maps: found (LIVE, packetId {id0} → {id1})"
                ));
            }
            Some((ShmemDetection::Stale, id0, _)) => {
                log.log(&format!(
                    "[scan] AC EVO maps: found (stale, packetId={id0} unchanged)"
                ));
            }
            None => log.log("[scan] AC EVO maps: not found"),
        }
        match ac1_raw {
            Some((ShmemDetection::Live, id0, id1)) => {
                log.log(&format!(
                    "[scan] AC1 maps: found (LIVE, packetId {id0} → {id1})"
                ));
            }
            Some((ShmemDetection::Stale, id0, _)) => {
                log.log(&format!(
                    "[scan] AC1 maps: found (stale, packetId={id0} unchanged)"
                ));
            }
            None => log.log("[scan] AC1 maps: not found"),
        }
    }

    AcProbeResult {
        ac_evo: evo_raw.map(|(d, _, _)| d),
        ac1: ac1_raw.map(|(d, _, _)| d),
    }
}

/// Returns `(detection, id0, id1)` so the caller can log exact packetId values.
#[cfg(windows)]
fn probe_map(name: &str) -> Option<(ShmemDetection, i32, i32)> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
    };

    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide.as_ptr()) };
    if handle == 0 {
        return None;
    }

    let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 4) };
    if view.Value.is_null() {
        unsafe { CloseHandle(handle) };
        return None;
    }

    let id0 = unsafe { std::ptr::read_volatile(view.Value as *const i32) };
    std::thread::sleep(Duration::from_millis(100));
    let id1 = unsafe { std::ptr::read_volatile(view.Value as *const i32) };

    unsafe {
        UnmapViewOfFile(view);
        CloseHandle(handle);
    }

    if id1 != id0 {
        Some((ShmemDetection::Live, id0, id1))
    } else {
        Some((ShmemDetection::Stale, id0, id1))
    }
}

#[cfg(not(windows))]
fn probe_map(_name: &str) -> Option<(ShmemDetection, i32, i32)> {
    None
}
