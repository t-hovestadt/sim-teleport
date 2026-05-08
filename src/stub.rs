//! StubManager: spawn short-lived named processes so SimHub's plugin process-check passes.
//!
//! SimHub's AC plugins call IsProcessRunning before reading shared memory. On the target PC
//! no game process exists, so the plugins silently skip telemetry even after sim-bridge has
//! populated the maps. Spawning a copy of sim-bridge.exe named acs.exe / acc.exe /
//! assettocorsa_evo.exe satisfies the check.
//!
//! When Steam libraries are found, stubs are placed inside the Steam common directories that
//! the appmanifest ACF files point to (e.g. steamapps\common\assettocorsa\acs.exe). This
//! ensures SimHub's FindProcessPath → GetDirectoryName resolves to the same path that Steam's
//! ACManager reads from the appmanifest — preventing NullReferenceException.
//!
//! Falls back to %TEMP%\sim-bridge-stubs\{game}\ when Steam is not found.
//!
//! A Windows Job Object with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE ensures stubs are killed
//! even if sim-bridge crashes (handles are closed by the OS on process termination).

#[cfg(windows)]
use std::collections::HashMap;
use std::path::Path;

use crate::logger::Logger;

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
    /// When set, stub exes are placed in Steam's common directory so that
    /// FindProcessPath matches the path the appmanifest ACF points to.
    #[cfg(windows)]
    steam_common_dir: Option<std::path::PathBuf>,
    #[cfg(windows)]
    log: Logger,
}

impl StubManager {
    /// `steam_common_dir` — the `steamapps\common\` directory of the first Steam
    /// library, or `None` when Steam is not installed. Pass `None` to fall back to
    /// the legacy `%TEMP%\sim-bridge-stubs\` placement.
    pub fn new(log: Logger, steam_common_dir: Option<std::path::PathBuf>) -> Self {
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
                steam_common_dir,
                log,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (log, steam_common_dir);
            Self {}
        }
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
                    self.log
                        .log(&format!("[stub] spawned {name}.exe (pid {})", child.id()));
                    self.stubs.insert(name.to_string(), child);
                }
                None => self.log.log(&format!("[stub] failed to spawn {name}.exe")),
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
            self.log.log(&format!("[stub] killed {name}.exe"));
        }
        #[cfg(not(windows))]
        let _ = name;
    }

    #[cfg(windows)]
    fn spawn_stub(&self, name: &str) -> Option<std::process::Child> {
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;

        let exe = std::env::current_exe().ok()?;

        // Subdirectory and exe names must match the installdir values in the Steam
        // appmanifest ACF files so FindProcessPath resolves to the same path.
        let (game_subdir, exe_name): (&str, String) = match name {
            "acs" => ("assettocorsa", "acs.exe".to_string()),
            "acc" => ("assettocorsacompetizione", "acc.exe".to_string()),
            "assettocorsa_evo" => ("assettocorsa_evo", "assettocorsa_evo.exe".to_string()),
            other => (other, format!("{other}.exe")),
        };

        // Prefer the Steam common directory; fall back to %TEMP% when Steam is absent.
        let game_dir = if let Some(common) = &self.steam_common_dir {
            common.join(game_subdir)
        } else {
            std::env::temp_dir()
                .join("sim-bridge-stubs")
                .join(game_subdir)
        };
        std::fs::create_dir_all(&game_dir).ok()?;

        let stub_path = game_dir.join(&exe_name);

        // Re-copy if stub is absent or source binary is newer.
        let needs_copy = !stub_path.exists() || {
            let src_mod = std::fs::metadata(&exe).and_then(|m| m.modified()).ok();
            let dst_mod = std::fs::metadata(&stub_path)
                .and_then(|m| m.modified())
                .ok();
            src_mod.zip(dst_mod).is_none_or(|(s, d)| s > d)
        };
        if needs_copy {
            if let Err(e) = std::fs::copy(&exe, &stub_path) {
                self.log.log(&format!(
                    "[stub] failed to copy to {}: {e}",
                    stub_path.display()
                ));
                return None;
            }
        }

        let child = std::process::Command::new(&stub_path)
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

// ── Fake install directory structures ─────────────────────────────────────────

/// Create minimal directory structures for all three AC games so SimHub's ACManager
/// can find the files it checks when navigating from the install path.
#[cfg(windows)]
pub fn setup_all_game_environments(stub_dir: &Path, log: &Logger) {
    setup_ac1_environment(stub_dir, log);
    setup_acc_environment(stub_dir, log);
    setup_acevo_environment(stub_dir, log);
    setup_documents_folders(log);
}

#[cfg(not(windows))]
pub fn setup_all_game_environments(_stub_dir: &Path, _log: &Logger) {}

#[cfg(windows)]
fn setup_ac1_environment(stub_dir: &Path, log: &Logger) {
    let game_dir = stub_dir.join("assettocorsa");
    for d in &["system/cfg", "apps/python/SimHub", "content/cars"] {
        std::fs::create_dir_all(game_dir.join(d)).ok();
    }

    let ini = game_dir.join("system/cfg/assetto_corsa.ini");
    if !ini.exists() {
        std::fs::write(&ini, "[SETTINGS]\r\n").ok();
    }

    for fname in &["SimHub.py", "simhub_shared_mem.py", "__init__.py"] {
        let py = game_dir.join("apps/python/SimHub").join(fname);
        if !py.exists() {
            std::fs::write(&py, "# sim-bridge stub\r\n").ok();
        }
    }

    log.log("[stub] AC1 directory structure ready");
}

#[cfg(windows)]
fn setup_acc_environment(stub_dir: &Path, log: &Logger) {
    let game_dir = stub_dir.join("assettocorsacompetizione");
    std::fs::create_dir_all(game_dir.join("Config")).ok();

    let cfg = game_dir.join("Config/broadcasting.json");
    if !cfg.exists() {
        std::fs::write(&cfg, "{}\r\n").ok();
    }

    log.log("[stub] ACC directory structure ready");
}

#[cfg(windows)]
fn setup_acevo_environment(stub_dir: &Path, log: &Logger) {
    let game_dir = stub_dir.join("assettocorsa_evo");
    for d in &["cfg", "content"] {
        std::fs::create_dir_all(game_dir.join(d)).ok();
    }

    log.log("[stub] AC EVO directory structure ready");
}

#[cfg(windows)]
fn setup_documents_folders(log: &Logger) {
    let profile = match std::env::var("USERPROFILE") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => return,
    };
    let docs = profile.join("Documents");

    // AC1: Documents\Assetto Corsa\cfg\python.ini
    let ac1_cfg = docs.join("Assetto Corsa").join("cfg");
    if !ac1_cfg.exists() {
        std::fs::create_dir_all(&ac1_cfg).ok();
        let ini = ac1_cfg.join("python.ini");
        if !ini.exists() {
            std::fs::write(&ini, "[SIMHUB]\r\nACTIVE=1\r\n[SIMHUB_LOG]\r\nACTIVE=0\r\n").ok();
            log.log("[stub] Created Documents\\Assetto Corsa\\cfg\\python.ini");
        }
    }

    // ACC: Documents\Assetto Corsa Competizione\Config\broadcasting.json
    let acc_cfg = docs.join("Assetto Corsa Competizione").join("Config");
    if !acc_cfg.exists() {
        std::fs::create_dir_all(&acc_cfg).ok();
        let broadcasting = acc_cfg.join("broadcasting.json");
        if !broadcasting.exists() {
            std::fs::write(
                &broadcasting,
                "{\r\n  \"updListenerPort\": 9000,\r\n  \"connectionPassword\": \"\",\r\n  \"commandPassword\": \"\"\r\n}\r\n",
            )
            .ok();
            log.log("[stub] Created Documents\\ACC\\Config\\broadcasting.json");
        }
    }

    // AC EVO: Documents\Assetto Corsa EVO\
    let evo_dir = docs.join("Assetto Corsa EVO");
    if !evo_dir.exists() {
        std::fs::create_dir_all(&evo_dir).ok();
        log.log("[stub] Created Documents\\Assetto Corsa EVO");
    }

    // Wreckfest 2: Documents\My Games\Wreckfest 2
    let wf2_dir = docs.join("My Games").join("Wreckfest 2");
    if !wf2_dir.exists() {
        std::fs::create_dir_all(&wf2_dir).ok();
        log.log("[stub] Created Documents\\My Games\\Wreckfest 2");
    }
}

// ── Drop ──────────────────────────────────────────────────────────────────────

impl Drop for StubManager {
    fn drop(&mut self) {
        #[cfg(windows)]
        for (name, mut child) in self.stubs.drain() {
            let _ = child.kill();
            let _ = child.wait();
            self.log.log(&format!("[stub] cleanup: killed {name}.exe"));
        }
        // Job object handle closes here — OS kills any remaining stubs.
    }
}
