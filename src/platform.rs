//! Platform-specific performance helpers.
//!
//! On Windows: sets timer resolution to 1 ms, raises thread/process priority,
//! and detects running game processes.
//! On other platforms: no-ops.

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, SetPriorityClass, SetThreadPriority,
        HIGH_PRIORITY_CLASS, THREAD_PRIORITY_ABOVE_NORMAL,
    };

    /// RAII guard that requests 1 ms Windows timer resolution for the lifetime
    /// of the process. The default 15.6 ms resolution caps how quickly the OS
    /// wakes a sleeping thread, adding jitter to every recv_from call.
    pub struct HighResTimer;

    impl HighResTimer {
        pub fn acquire() -> Self {
            let result = unsafe { timeBeginPeriod(1) };
            if result != 0 {
                eprintln!(
                    "timeBeginPeriod(1) failed (code {result}); timer resolution stays at ~15.6 ms"
                );
            }
            Self
        }
    }

    impl Drop for HighResTimer {
        fn drop(&mut self) {
            unsafe { timeEndPeriod(1) };
        }
    }

    /// Raise the calling thread to ABOVE_NORMAL priority so the OS scheduler
    /// preempts it less often during the hot send/receive loop.
    pub fn boost_thread_priority() {
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL) };
    }

    /// Raise the process to HIGH_PRIORITY_CLASS for lower OS scheduling jitter.
    /// Safe on the SimHub PC. On the gaming PC this competes with the game's own
    /// scheduling — only use it if you know the machine can handle it.
    pub fn set_high_priority() {
        let ok = unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) };
        if ok == 0 {
            eprintln!("set_high_priority: SetPriorityClass failed");
        } else {
            println!("Process priority set to HIGH_PRIORITY_CLASS.");
        }
    }

    /// Check whether any process whose name (case-insensitive) matches one of
    /// `names` is currently running. Used for --auto-detect on the source side.
    pub fn is_process_running(names: &[&str]) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return false;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut found = false;
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    let null_pos = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let exe = String::from_utf16_lossy(&entry.szExeFile[..null_pos]);
                    if names.iter().any(|n| n.eq_ignore_ascii_case(&exe)) {
                        found = true;
                        break;
                    }
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);
            found
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub struct HighResTimer;
    impl HighResTimer {
        pub fn acquire() -> Self {
            Self
        }
    }
    pub fn boost_thread_priority() {}
    pub fn set_high_priority() {}
    pub fn is_process_running(_names: &[&str]) -> bool {
        false
    }
}

pub use imp::{boost_thread_priority, is_process_running, set_high_priority, HighResTimer};
