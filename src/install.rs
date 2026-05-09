const TASK_NAME: &str = "SimTeleport";
// Legacy task name created by earlier releases (sim-bridge). Cleaned up in uninstall.
const LEGACY_TASK_NAME: &str = "SimBridge";

/// Register sim-teleport in Windows Task Scheduler to start on logon at highest privilege.
/// `mode` should be "source" or "target".
pub fn install(mode: &str) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_string_lossy();
    let task_run = format!("\"{}\" {}", exe_str, mode);

    let status = std::process::Command::new("schtasks")
        .args([
            "/create", "/tn", TASK_NAME, "/tr", &task_run, "/sc", "onlogon", "/rl", "highest", "/f",
        ])
        .status()?;

    if status.success() {
        println!("Registered Task Scheduler entry \"{}\".", TASK_NAME);
        println!("sim-teleport will start automatically when you log on.");
        println!();
        println!("To remove: sim-teleport uninstall");
    } else {
        anyhow::bail!(
            "schtasks /create failed (exit code {:?}). Try running as Administrator.",
            status.code()
        );
    }
    Ok(())
}

/// Remove the sim-teleport Task Scheduler entry.
/// Also removes the legacy "SimBridge" entry left by earlier releases.
pub fn uninstall() -> anyhow::Result<()> {
    let status = std::process::Command::new("schtasks")
        .args(["/delete", "/tn", TASK_NAME, "/f"])
        .status()?;

    if status.success() {
        println!("Removed Task Scheduler entry \"{}\".", TASK_NAME);
    } else {
        anyhow::bail!(
            "schtasks /delete failed (exit code {:?}). The entry may not exist.",
            status.code()
        );
    }

    // Best-effort cleanup of the legacy "SimBridge" entry. Ignore errors — the
    // entry may not exist on systems that never ran an older release.
    let _ = std::process::Command::new("schtasks")
        .args(["/delete", "/tn", LEGACY_TASK_NAME, "/f"])
        .status()
        .map(|s| {
            if s.success() {
                println!(
                    "Also removed legacy Task Scheduler entry \"{}\".",
                    LEGACY_TASK_NAME
                );
            }
        });

    Ok(())
}
