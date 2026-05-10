/// Known Fanatec process names (lowercased for case-insensitive comparison).
const FANATEC_PROCESSES: &[&str] = &["fanatecservice.exe", "fwpnpservice.exe", "flcontrol.exe"];

/// Windows service names passed to `net stop` / `net start`.
const FANATEC_SERVICES: &[&str] = &["FanatecService", "FWPnpService"];

/// Returns `true` if any known Fanatec process is currently running.
pub fn is_fanatec_running() -> bool {
    let mut scanner = crate::scanner::ProcessScanner::new();
    scanner.refresh();
    scanner.is_running(FANATEC_PROCESSES)
}

/// Stop known Fanatec Windows services so they release any shared memory
/// handles they hold before the game creates its own maps.
///
/// Silently skips services that do not exist or are not currently running.
/// Returns `Err` only when a service is found but stopping it fails —
/// typically because the process is not running with administrator privileges.
pub fn stop_fanatec() -> Result<(), String> {
    for &svc in FANATEC_SERVICES {
        stop_service(svc)?;
    }
    Ok(())
}

/// Start Fanatec Windows services after the game has created its shared memory
/// maps.  Silently skips services that do not exist or are already running.
pub fn start_fanatec() -> Result<(), String> {
    for &svc in FANATEC_SERVICES {
        start_service(svc)?;
    }
    Ok(())
}

fn stop_service(name: &str) -> Result<(), String> {
    let out = run_net(&["stop", name])?;
    if out.success {
        return Ok(());
    }
    // "The service name is invalid." — service does not exist on this machine.
    // "The service has not been started."  — exists but not running.
    // Both are acceptable outcomes; just move on.
    if out.lower.contains("invalid") || out.lower.contains("not been started") {
        return Ok(());
    }
    Err(format!("net stop {name}: {}", out.text.trim()))
}

fn start_service(name: &str) -> Result<(), String> {
    let out = run_net(&["start", name])?;
    if out.success {
        return Ok(());
    }
    // "already been started" — already running; fine.
    // "service name is invalid" — doesn't exist on this machine; fine.
    if out.lower.contains("already been started") || out.lower.contains("invalid") {
        return Ok(());
    }
    Err(format!("net start {name}: {}", out.text.trim()))
}

struct NetOutput {
    success: bool,
    text: String,
    lower: String,
}

fn run_net(args: &[&str]) -> Result<NetOutput, String> {
    let result = std::process::Command::new("net")
        .args(args)
        .output()
        .map_err(|e| format!("net {}: {e}", args.join(" ")))?;
    // `net` writes its messages to stdout, not stderr.
    let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    let text = format!("{stdout}{stderr}");
    let lower = text.to_lowercase();
    Ok(NetOutput {
        success: result.status.success(),
        text,
        lower,
    })
}

/// Returns `true` if the current process is running with administrator privileges.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = 0;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut TOKEN_ELEVATION as *mut _,
            size,
            &mut size,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}
