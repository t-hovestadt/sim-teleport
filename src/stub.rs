/// StubManager: spawn short-lived named processes so SimHub's plugin process-check passes.
///
/// SimHub's AC plugin (ACSharedMemory.dll) calls IsProcessRunning before reading shared memory.
/// On the target PC no game process exists, so the plugin silently skips telemetry even after
/// sim-bridge has populated the maps. Spawning a copy of sim-bridge.exe named acs.exe (etc.)
/// satisfies the check. Stubs are killed when AC data goes stale and on sim-bridge shutdown.
///
/// A Windows Job Object with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE ensures stubs are killed
/// even if sim-bridge crashes (handles are closed by the OS on process termination).

#[cfg(windows)]
use std::collections::HashMap;

/// Stub process is this binary renamed in a temp dir, launched with the hidden "stub" subcommand.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
};

pub struct StubManager {
    #[cfg(windows)]
    stubs: HashMap<String, std::process::Child>,
    #[cfg(windows)]
    job: HANDLE,
}

impl StubManager {
    pub fn new() -> Self {
        #[cfg(windows)]
        {
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job != 0 {
                unsafe {
                    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                    SetInformationJobObject(
                        job,
                        JobObjectExtendedLimitInformation,
                        &raw const info as *const _,
                        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                    );
                }
            }
            Self {
                stubs: HashMap::new(),
                job,
            }
        }
        #[cfg(not(windows))]
        Self {}
    }

    /// Ensure a stub process with the given name is running. No-op if already alive.
    pub fn ensure_running(&mut self, name: &str) {
        #[cfg(windows)]
        {
            let is_dead = if let Some(child) = self.stubs.get_mut(name) {
                child.try_wait().ok().flatten().is_some()
            } else {
                false
            };
            if is_dead {
                self.stubs.remove(name);
            }
            if self.stubs.contains_key(name) {
                return; // still alive
            }
            match self.spawn_stub(name) {
                Some(child) => {
                    eprintln!("[stub] spawned {name}.exe (pid {})", child.id());
                    self.stubs.insert(name.to_string(), child);
                }
                None => eprintln!("[stub] failed to spawn {name}.exe"),
            }
        }
        #[cfg(not(windows))]
        let _ = name;
    }

    /// Kill a named stub process if running.
    pub fn kill(&mut self, name: &str) {
        #[cfg(windows)]
        if let Some(mut child) = self.stubs.remove(name) {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!("[stub] killed {name}.exe");
        }
        #[cfg(not(windows))]
        let _ = name;
    }

    /// Spawn stubs for all AC variants (AC1, EVO, ACC).
    pub fn ensure_running_all_ac(&mut self) {
        self.ensure_running("acs");
        self.ensure_running("assettocorsa_evo");
        self.ensure_running("acc");
    }

    /// Kill stubs for all AC variants.
    pub fn kill_all_ac(&mut self) {
        self.kill("acs");
        self.kill("assettocorsa_evo");
        self.kill("acc");
    }

    #[cfg(windows)]
    fn spawn_stub(&self, name: &str) -> Option<std::process::Child> {
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;

        let exe = std::env::current_exe().ok()?;
        let stub_dir = std::env::temp_dir().join("sim-bridge-stubs");
        std::fs::create_dir_all(&stub_dir).ok()?;
        let stub_path = stub_dir.join(format!("{name}.exe"));

        // Re-copy if stub is absent or older than this binary.
        let needs_copy = !stub_path.exists() || {
            let src_mod = std::fs::metadata(&exe).and_then(|m| m.modified()).ok();
            let dst_mod = std::fs::metadata(&stub_path)
                .and_then(|m| m.modified())
                .ok();
            src_mod.zip(dst_mod).is_none_or(|(s, d)| s > d)
        };
        if needs_copy {
            if let Err(e) = std::fs::copy(&exe, &stub_path) {
                eprintln!("[stub] failed to copy to {}: {e}", stub_path.display());
                return None;
            }
        }

        let mut child = std::process::Command::new(&stub_path)
            .arg("stub")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .ok()?;

        // Assign to job object so it's killed automatically when sim-bridge exits.
        if self.job != 0 {
            unsafe {
                AssignProcessToJobObject(self.job, child.as_raw_handle() as HANDLE);
            }
        }

        Some(child)
    }
}

impl Default for StubManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
impl Drop for StubManager {
    fn drop(&mut self) {
        for (name, mut child) in self.stubs.drain() {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!("[stub] cleanup: killed {name}.exe");
        }
        // Job object handle closes here — OS kills any remaining stubs.
    }
}
