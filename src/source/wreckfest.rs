use crate::logger::Logger;

/// Create `config.json` in Wreckfest 2's telemetry folder(s) if not already present.
///
/// Wreckfest 2 saves telemetry config under a Steam-ID-numbered subdirectory inside
/// the user's save directory. We scan both the Unreal `LocalAppData` location and the
/// `Documents\My Games` location for any numbered subdirectories and create the config
/// in each one found. If no save directory exists (game never run), this is a no-op.
pub(super) fn ensure_wreckfest_telemetry_config(log: &Logger) {
    #[cfg(windows)]
    {
        use std::path::PathBuf;

        // The format the game actually reads (matches games.rs notes and README).
        let config_json =
            "{\r\n  \"udp\": [\r\n    {\r\n      \"enabled\": 1,\r\n      \"ip\": \"127.0.0.1\",\r\n      \"port\": \"23123\"\r\n    }\r\n  ]\r\n}\r\n";

        let mut telemetry_dirs: Vec<PathBuf> = Vec::new();

        // %LOCALAPPDATA%\Wreckfest2\Saved\SaveGames\<SteamID>\savegame\telemetry
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let save_root = PathBuf::from(&local)
                .join("Wreckfest2")
                .join("Saved")
                .join("SaveGames");
            scan_steam_id_subdirs(&save_root, &mut telemetry_dirs);
        }

        // %USERPROFILE%\Documents\My Games\Wreckfest 2\<SteamID>\savegame\telemetry
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let save_root = PathBuf::from(&profile)
                .join("Documents")
                .join("My Games")
                .join("Wreckfest 2");
            scan_steam_id_subdirs(&save_root, &mut telemetry_dirs);
        }

        if telemetry_dirs.is_empty() {
            return; // Game never run; no save directory to write to.
        }

        for telemetry_dir in &telemetry_dirs {
            let config_path = telemetry_dir.join("config.json");
            if config_path.exists() {
                continue; // User may have configured it; leave it alone.
            }
            if std::fs::create_dir_all(telemetry_dir).is_ok() {
                match std::fs::write(&config_path, config_json) {
                    Ok(()) => {
                        log.log(&format!(
                            "[Wreckfest 2] Created telemetry config: {}",
                            config_path.display()
                        ));
                        log.log("[Wreckfest 2] Restart the game for telemetry to activate");
                    }
                    Err(e) => {
                        log.log(&format!(
                            "[Wreckfest 2] Failed to write config {}: {e}",
                            config_path.display()
                        ));
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    let _ = log;
}

/// Scan `root` for numbered subdirectories (Steam IDs) and push
/// `<subdir>/savegame/telemetry` into `out`.
#[cfg(windows)]
fn scan_steam_id_subdirs(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Accept any directory whose name is all digits (Steam ID).
            let name = entry.file_name();
            if name.to_string_lossy().chars().all(|c| c.is_ascii_digit()) {
                out.push(path.join("savegame").join("telemetry"));
            }
        }
    }
}
