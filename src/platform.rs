// Windows-specific timing and thread-scheduling helpers.
// On non-Windows, all functions are no-ops and all guards are unit types.

// ── HighResTimer ──────────────────────────────────────────────────────────────

/// RAII guard that requests 1 ms timer resolution for the process lifetime.
///
/// Windows defaults to ~15.6 ms. At 60 Hz (16.7 ms per tick), that resolution
/// is barely workable; requesting 1 ms makes sleep precision acceptable.
pub struct HighResTimer {
    _private: (),
}

impl HighResTimer {
    /// Request 1 ms timer resolution. Always returns Self; prints a warning if
    /// the system call fails.
    pub fn acquire() -> Self {
        #[cfg(windows)]
        unsafe {
            // TIMERR_NOERROR == 0
            let result = windows_sys::Win32::Media::timeBeginPeriod(1);
            if result != 0 {
                eprintln!(
                    "timeBeginPeriod(1) failed (code {result}); timer resolution stays at ~15.6 ms"
                );
            }
        }
        Self { _private: () }
    }
}

impl Drop for HighResTimer {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Media::timeEndPeriod(1);
        }
    }
}

// ── MmcssGuard ────────────────────────────────────────────────────────────────

/// RAII guard that registers the calling thread with the Multimedia Class
/// Scheduler Service (MMCSS), reducing scheduling latency for real-time loops.
#[cfg(windows)]
pub struct MmcssGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(not(windows))]
pub struct MmcssGuard(());

impl MmcssGuard {
    /// Register the calling thread with MMCSS under `task` (e.g. `"Games"`).
    /// Returns `Some` on success; `None` on failure (prints a warning).
    /// On non-Windows always returns `Some`.
    pub fn acquire() -> Option<Self> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Threading::{
                AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW,
            };
            let _ = AvRevertMmThreadCharacteristics; // suppress unused import lint
            let wide: Vec<u16> = "Games\0".encode_utf16().collect();
            let mut task_index: u32 = 0;
            let h = AvSetMmThreadCharacteristicsW(wide.as_ptr(), &mut task_index);
            if h != 0 {
                Some(MmcssGuard(h))
            } else {
                eprintln!("MMCSS registration failed (continuing without it)");
                None
            }
        }
        #[cfg(not(windows))]
        Some(MmcssGuard(()))
    }
}

impl Drop for MmcssGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Threading::AvRevertMmThreadCharacteristics(self.0);
        }
    }
}

// ── Free functions ────────────────────────────────────────────────────────────

/// Raise the calling thread to ABOVE_NORMAL priority. Called unconditionally
/// at startup. No-op on non-Windows.
pub fn boost_thread_priority() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
        };
        let ok = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
        if ok == 0 {
            eprintln!("SetThreadPriority failed");
        }
    }
}

/// Raise the process to HIGH_PRIORITY_CLASS for lower OS scheduling jitter.
/// Stacks with `boost_thread_priority`. Only call when `--high-priority` is
/// set — on the game PC this competes with the game's own scheduling.
pub fn set_high_priority() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, SetPriorityClass, HIGH_PRIORITY_CLASS,
        };
        let ok = SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS);
        if ok == 0 {
            eprintln!("set_high_priority: SetPriorityClass failed");
        } else {
            println!("Process priority set to HIGH_PRIORITY_CLASS.");
        }
    }
}

/// Pin the calling thread to a specific CPU core. No-op on non-Windows or if
/// `core` is out of range (max 63).
pub fn pin_thread_to_core(core: usize) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadAffinityMask};
        if core >= 64 {
            eprintln!("pin_thread_to_core: core {core} out of range (max 63), ignoring");
            return;
        }
        let mask: usize = 1usize << core;
        let prev = SetThreadAffinityMask(GetCurrentThread(), mask);
        if prev == 0 {
            eprintln!("pin_thread_to_core: SetThreadAffinityMask failed for core {core}");
        } else {
            println!("Pinned thread to CPU core {core}.");
        }
    }
    #[cfg(not(windows))]
    let _ = core;
}
