#[cfg(windows)]
use std::path::{Path, PathBuf};

/// SimHub game codes to configure on the target PC.
/// These are the internal names SimHub uses to identify each game.
#[cfg(windows)]
const GAMES_TO_CONFIGURE: &[&str] = &[
    "IRacing",
    "AssettoCorsa",
    "AssettoCorsaEVO",
    "AssettoCorsaCompetizione",
    "Wreckfest2",
];

#[cfg(windows)]
fn simhub_plugins_dir(simhub_path: Option<&str>) -> Option<PathBuf> {
    // If the user configured a custom SimHub path, derive PluginsData from it.
    if let Some(exe) = simhub_path {
        if let Some(parent) = Path::new(exe).parent() {
            let p = parent.join("PluginsData");
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    for candidate in &[
        r"C:\Program Files (x86)\SimHub\PluginsData",
        r"C:\Program Files\SimHub\PluginsData",
    ] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// Update GameSettings.json so SimHub skips the "configure this game" prompt.
/// Returns true if the file was modified.
#[cfg(windows)]
fn ensure_game_configured(plugins_dir: &Path, game_code: &str) -> bool {
    let path = plugins_dir.join("GameSettings.json");

    let content = if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[SimHub] Can't read GameSettings.json: {e}");
                return false;
            }
        }
    } else {
        "{}".to_string()
    };

    let mut settings: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[SimHub] Can't parse GameSettings.json: {e}");
            return false;
        }
    };

    let obj = match settings.as_object_mut() {
        Some(o) => o,
        None => {
            eprintln!("[SimHub] GameSettings.json is not a JSON object");
            return false;
        }
    };

    // Already configured — nothing to do.
    let already_ok = obj
        .get(game_code)
        .and_then(|v| v.get("ManualConfigurationDismissed"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if already_ok {
        return false;
    }

    // Preserve any existing fields; only set the critical flags.
    let entry = obj
        .entry(game_code.to_string())
        .or_insert_with(|| serde_json::json!({}));
    let entry_obj = entry.as_object_mut().unwrap();

    entry_obj.insert(
        "ManualConfigurationDismissed".into(),
        serde_json::json!(true),
    );
    entry_obj.insert("DisableConfigAlert".into(), serde_json::json!(true));
    entry_obj.insert(
        "AutomaticConfigurationDismissed".into(),
        serde_json::json!(true),
    );
    entry_obj
        .entry("LastActivations")
        .or_insert_with(|| serde_json::json!(["2026-01-01T00:00:00Z"]));
    entry_obj
        .entry("UDPForwardIpAddress")
        .or_insert_with(|| serde_json::json!("127.0.0.1"));
    entry_obj
        .entry("UDPForwardActive")
        .or_insert(serde_json::json!(false));
    entry_obj
        .entry("AddictionnalUDPRedirects")
        .or_insert_with(|| serde_json::json!([]));

    let formatted = match serde_json::to_string_pretty(&settings) {
        Ok(s) => s.replace('\n', "\r\n"),
        Err(e) => {
            eprintln!("[SimHub] Can't serialize GameSettings.json: {e}");
            return false;
        }
    };

    if let Err(e) = std::fs::write(&path, formatted.as_bytes()) {
        eprintln!("[SimHub] Can't write GameSettings.json: {e}");
        return false;
    }

    true
}

/// Unhide a game in HiddenGames.json if it was explicitly hidden.
/// Returns true if the file was modified.
#[cfg(windows)]
fn ensure_game_visible(plugins_dir: &Path, game_code: &str) -> bool {
    let path = plugins_dir.join("HiddenGames.json");

    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return false, // file absent = all games visible by default
    };

    let mut hidden: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let obj = match hidden.as_object_mut() {
        Some(o) => o,
        None => return false,
    };

    // Only act if the game is explicitly set to true (hidden).
    let is_hidden = obj
        .get(game_code)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !is_hidden {
        return false;
    }

    obj.insert(game_code.to_string(), serde_json::json!(false));

    let formatted = match serde_json::to_string_pretty(&hidden) {
        Ok(s) => s.replace('\n', "\r\n"),
        Err(_) => return false,
    };

    let _ = std::fs::write(&path, formatted.as_bytes());
    true
}

/// Create the per-game PluginsData subfolder if it doesn't exist.
/// SimHub may check for its existence as a sign the game has been used.
/// Returns true if the folder was created.
#[cfg(windows)]
fn ensure_game_folder(plugins_dir: &Path, game_code: &str) -> bool {
    let game_dir = plugins_dir.join(game_code);
    if game_dir.is_dir() {
        return false;
    }
    match std::fs::create_dir_all(&game_dir) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("[SimHub] Can't create {game_code} folder: {e}");
            false
        }
    }
}

/// Configure SimHub on the target PC for all supported games.
///
/// Creates/updates `GameSettings.json` with `ManualConfigurationDismissed = true`
/// so SimHub skips the "game not configured" prompt for games installed only on the
/// source PC. Also ensures each game's PluginsData subfolder exists.
///
/// Idempotent: only writes files when changes are needed. Safe to call on every
/// target startup — subsequent calls are a no-op once everything is configured.
pub fn setup_simhub_for_target(simhub_path: Option<&str>) {
    #[cfg(windows)]
    {
        let plugins_dir = match simhub_plugins_dir(simhub_path) {
            Some(d) => d,
            None => {
                eprintln!(
                    "[SimHub] PluginsData directory not found — SimHub may not be installed."
                );
                return;
            }
        };

        let mut any_changes = false;

        for game_code in GAMES_TO_CONFIGURE {
            if ensure_game_configured(&plugins_dir, game_code) {
                eprintln!("[SimHub] Configured {game_code} in GameSettings.json");
                any_changes = true;
            }
            if ensure_game_visible(&plugins_dir, game_code) {
                eprintln!("[SimHub] Unhid {game_code} in HiddenGames.json");
                any_changes = true;
            }
            if ensure_game_folder(&plugins_dir, game_code) {
                eprintln!("[SimHub] Created {game_code} data folder");
            }
        }

        if any_changes {
            eprintln!(
                "[SimHub] *** Configuration updated. Please restart SimHub to apply changes. ***"
            );
        }
    }

    #[cfg(not(windows))]
    {
        let _ = simhub_path;
    }
}
