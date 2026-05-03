use std::collections::HashSet;

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
        names.iter().any(|n| self.snapshot.contains(&n.to_lowercase()))
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
                    let name = String::from_utf16_lossy(&entry.szExeFile[..null_pos])
                        .to_lowercase();
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
