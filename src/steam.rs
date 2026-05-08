//! Steam library discovery and ACF manifest management for AC game stubs.
//!
//! SimHub reads game install paths from Steam's appmanifest ACF files, not from
//! the Windows Uninstall registry. This module writes fake appmanifest files so
//! SimHub's ACManager finds a valid install path and does not throw
//! NullReferenceException.

use std::path::{Path, PathBuf};

// ── Game table ────────────────────────────────────────────────────────────────

#[cfg(windows)]
struct AcGame {
    appid: u32,
    name: &'static str,
    /// Folder name Steam uses under steamapps\common\.
    install_dir: &'static str,
    /// Name of the stub executable placed inside install_dir.
    exe_name: &'static str,
}

#[cfg(windows)]
const AC_GAMES: &[AcGame] = &[
    AcGame {
        appid: 244210,
        name: "Assetto Corsa",
        install_dir: "assettocorsa",
        exe_name: "acs.exe",
    },
    AcGame {
        appid: 805550,
        name: "Assetto Corsa Competizione",
        install_dir: "assettocorsacompetizione",
        exe_name: "acc.exe",
    },
    AcGame {
        appid: 3058630,
        name: "Assetto Corsa EVO",
        install_dir: "assettocorsa_evo",
        exe_name: "assettocorsa_evo.exe",
    },
];

// ── Windows registry helpers ──────────────────────────────────────────────────

/// Read a REG_SZ value from HKEY_LOCAL_MACHINE. Returns None on any error.
#[cfg(windows)]
fn read_hklm_sz(sub_key: &str, value_name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
    };

    let sub_key_w: Vec<u16> = sub_key.encode_utf16().chain(std::iter::once(0)).collect();
    let value_w: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey = 0isize;
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            sub_key_w.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        ) != 0
        {
            return None;
        }

        // First call: get required buffer size.
        let mut data_type: u32 = 0;
        let mut data_size: u32 = 0;
        if RegQueryValueExW(
            hkey,
            value_w.as_ptr(),
            std::ptr::null_mut(),
            &mut data_type,
            std::ptr::null_mut(),
            &mut data_size,
        ) != 0
            || data_type != REG_SZ
        {
            RegCloseKey(hkey);
            return None;
        }

        // Second call: read the data.
        let mut buf = vec![0u8; data_size as usize];
        if RegQueryValueExW(
            hkey,
            value_w.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            &mut data_size,
        ) != 0
        {
            RegCloseKey(hkey);
            return None;
        }
        RegCloseKey(hkey);

        // Convert UTF-16 LE bytes to String.
        let wide: Vec<u16> = buf
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        Some(String::from_utf16_lossy(&wide[..end]))
    }
}

/// Find the Steam installation directory via the Windows registry.
#[cfg(windows)]
fn steam_install_path() -> Option<PathBuf> {
    read_hklm_sz(r"SOFTWARE\Valve\Steam", "InstallPath")
        .or_else(|| read_hklm_sz(r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"))
        .map(PathBuf::from)
}

// ── VDF parser ────────────────────────────────────────────────────────────────

/// Extract the value from a `"key"  "value"` line in Valve KeyValues format.
#[cfg(windows)]
fn kv_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let line = line.trim();
    let quoted_key = format!("\"{key}\"");
    let pos = line.find(quoted_key.as_str())?;
    let after = line[pos + quoted_key.len()..].trim_start();
    if let Some(inner) = after.strip_prefix('"') {
        let end = inner.find('"')?;
        Some(&inner[..end])
    } else {
        None
    }
}

/// Parse Steam's `libraryfolders.vdf` and return valid library root directories.
#[cfg(windows)]
fn parse_library_folders(vdf: &Path) -> Vec<PathBuf> {
    let content = match std::fs::read_to_string(vdf) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut paths = Vec::new();
    for line in content.lines() {
        if let Some(v) = kv_value(line, "path") {
            // VDF stores backslashes doubled: "C:\\Steam" → C:\Steam
            let unescaped = v.replace("\\\\", "\\");
            let p = PathBuf::from(unescaped);
            if p.exists() {
                paths.push(p);
            }
        }
    }
    paths
}

// ── ACF helpers ───────────────────────────────────────────────────────────────

/// Return true when the directory has more than 10 entries — indicating a real
/// game install rather than our minimal stub placeholder.
#[cfg(windows)]
fn has_real_install(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .ok()
        .map(|e| e.flatten().count() > 10)
        .unwrap_or(false)
}

/// Build a minimal `appmanifest_*.acf` body for the given game.
#[cfg(windows)]
fn build_acf(game: &AcGame) -> String {
    format!(
        "\"AppState\"\r\n\
         {{\r\n\
         \t\"appid\"\t\t\"{appid}\"\r\n\
         \t\"Universe\"\t\t\"1\"\r\n\
         \t\"name\"\t\t\"{name}\"\r\n\
         \t\"StateFlags\"\t\t\"4\"\r\n\
         \t\"installdir\"\t\t\"{installdir}\"\r\n\
         \t\"LastUpdated\"\t\t\"0\"\r\n\
         \t\"SizeOnDisk\"\t\t\"0\"\r\n\
         \t\"buildid\"\t\t\"0\"\r\n\
         }}\r\n",
        appid = game.appid,
        name = game.name,
        installdir = game.install_dir,
    )
}

/// Populate the game's `steamapps\common\<installdir>` directory with the minimal
/// file structure that SimHub expects when it navigates into the install path.
#[cfg(windows)]
fn setup_game_common_dir(game: &AcGame, common_dir: &Path) {
    match game.appid {
        // Assetto Corsa
        244210 => {
            for d in &["system/cfg", "apps/python/SimHub", "content/cars"] {
                std::fs::create_dir_all(common_dir.join(d)).ok();
            }
            let ini = common_dir.join("system/cfg/assetto_corsa.ini");
            if !ini.exists() {
                std::fs::write(ini, "[SETTINGS]\r\n").ok();
            }
            for fname in &["SimHub.py", "simhub_shared_mem.py", "__init__.py"] {
                let py = common_dir.join("apps/python/SimHub").join(fname);
                if !py.exists() {
                    std::fs::write(py, "# sim-bridge stub\r\n").ok();
                }
            }
        }
        // Assetto Corsa Competizione
        805550 => {
            std::fs::create_dir_all(common_dir.join("Config")).ok();
            let cfg = common_dir.join("Config/broadcasting.json");
            if !cfg.exists() {
                std::fs::write(
                    cfg,
                    "{\r\n  \"updListenerPort\": 9000,\r\n  \
                     \"connectionPassword\": \"\",\r\n  \
                     \"commandPassword\": \"\"\r\n}\r\n",
                )
                .ok();
            }
        }
        // Assetto Corsa EVO
        3058630 => {
            for d in &["cfg", "content"] {
                std::fs::create_dir_all(common_dir.join(d)).ok();
            }
        }
        _ => {}
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Locate all Steam library root directories by reading the Windows registry and
/// parsing `libraryfolders.vdf`. Returns an empty vec on non-Windows or when
/// Steam is not installed.
pub fn find_steam_libraries(log: &impl Fn(&str)) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let steam = match steam_install_path() {
            Some(p) => p,
            None => {
                log("[steam] Steam installation not found in registry");
                return Vec::new();
            }
        };

        log(&format!("[steam] Steam found at {}", steam.display()));

        let vdf = steam.join("steamapps").join("libraryfolders.vdf");
        let mut libs = parse_library_folders(&vdf);

        // The main Steam directory is always a valid library even if absent from the VDF.
        if !libs.iter().any(|l| l == &steam) {
            libs.insert(0, steam);
        }

        log(&format!(
            "[steam] Found {} Steam library path(s)",
            libs.len()
        ));
        libs
    }
    #[cfg(not(windows))]
    {
        let _ = log;
        Vec::new()
    }
}

/// For each Steam library that does not have a real AC game install, write a fake
/// `appmanifest_{appid}.acf` so SimHub's ACManager finds a valid install path and
/// does not throw NullReferenceException.
///
/// Also creates `steamapps\common\<installdir>\` with the minimal file structure
/// SimHub expects when it navigates to the install path.
///
/// Returns the list of ACF paths that were created (pass to `cleanup_ac_appmanifests`
/// on shutdown). Skips libraries where the game appears genuinely installed (>10 files).
pub fn ensure_ac_appmanifests(
    libraries: &[PathBuf],
    stub_base_dir: &Path,
    log: &impl Fn(&str),
) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let _ = stub_base_dir;
        let mut created = Vec::new();

        for lib in libraries {
            let steamapps = lib.join("steamapps");
            if !steamapps.exists() {
                continue;
            }

            for game in AC_GAMES {
                let acf_path = steamapps.join(format!("appmanifest_{}.acf", game.appid));
                let common_dir = steamapps.join("common").join(game.install_dir);

                // Skip libraries where the game is genuinely installed.
                if acf_path.exists() && has_real_install(&common_dir) {
                    log(&format!(
                        "[steam] {} is installed — skipping fake ACF",
                        game.name
                    ));
                    continue;
                }

                // Create common/<installdir> with the minimal structure SimHub navigates.
                if std::fs::create_dir_all(&common_dir).is_ok() {
                    setup_game_common_dir(game, &common_dir);
                } else {
                    log(&format!(
                        "[steam] Cannot create {}: skipping",
                        common_dir.display()
                    ));
                    continue;
                }

                // Copy sim-bridge.exe as the named stub process so that when
                // StubManager spawns it, FindProcessPath resolves to this directory —
                // the same path that the appmanifest points to.
                if let Ok(src) = std::env::current_exe() {
                    let dst = common_dir.join(game.exe_name);
                    let needs_copy = !dst.exists() || {
                        let src_mod = std::fs::metadata(&src).and_then(|m| m.modified()).ok();
                        let dst_mod = std::fs::metadata(&dst).and_then(|m| m.modified()).ok();
                        src_mod.zip(dst_mod).is_none_or(|(s, d)| s > d)
                    };
                    if needs_copy {
                        if let Err(e) = std::fs::copy(&src, &dst) {
                            log(&format!(
                                "[steam] Cannot copy stub exe to {}: {e}",
                                dst.display()
                            ));
                        }
                    }
                }

                // Write the appmanifest.
                match std::fs::write(&acf_path, build_acf(game).as_bytes()) {
                    Ok(()) => {
                        log(&format!(
                            "[steam] Wrote appmanifest for {} ({})",
                            game.name,
                            acf_path.display()
                        ));
                        created.push(acf_path);
                    }
                    Err(e) => {
                        log(&format!(
                            "[steam] Cannot write appmanifest for {}: {e}",
                            game.name
                        ));
                    }
                }
            }
        }

        created
    }
    #[cfg(not(windows))]
    {
        let _ = (libraries, stub_base_dir, log);
        Vec::new()
    }
}

/// Remove ACF files previously created by `ensure_ac_appmanifests`.
pub fn cleanup_ac_appmanifests(created: &[PathBuf], log: &impl Fn(&str)) {
    #[cfg(windows)]
    {
        for path in created {
            match std::fs::remove_file(path) {
                Ok(()) => log(&format!("[steam] Removed {}", path.display())),
                Err(e) => log(&format!("[steam] Cannot remove {}: {e}", path.display())),
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (created, log);
    }
}
