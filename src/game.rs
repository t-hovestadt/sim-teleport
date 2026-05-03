use std::thread;
use std::time::Duration;

use crate::maps::SharedMap;

/// Per-game shared memory configuration.
///
/// The `max_*_size` fields document known struct sizes. Source-side compression
/// buffers are now sized from actual runtime map sizes (`SharedMap::size()`), so
/// these values are informational. The target decompression buffer uses them as a
/// lower bound, but `DUAL_MAP_SIZE` (64 KB) always dominates in practice.
pub struct GameConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub physics_map: &'static str,
    pub graphics_map: &'static str,
    pub static_map: &'static str,
    /// Known physics struct size in bytes.
    pub max_physics_size: usize,
    /// Known graphics struct size in bytes.
    pub max_graphics_size: usize,
    /// Known static struct size in bytes.
    pub max_static_size: usize,
}

/// Assetto Corsa 1 (AC1).
///
/// Three shared memory maps updated at different rates:
/// - Physics: every physics step (~333 Hz), packetId at offset 0
/// - Graphics: every render frame (~60 Hz), packetId at offset 0, AC_STATUS at offset 4
/// - Static: once per session load
pub const AC1: GameConfig = GameConfig {
    id: "ac1",
    name: "Assetto Corsa",
    physics_map: "Local\\acpmf_physics",
    graphics_map: "Local\\acpmf_graphics",
    static_map: "Local\\acpmf_static",
    // SPageFilePhysics / SPageFileGraphics / SPageFileStatic from the AC SDK.
    max_physics_size: 9312,
    max_graphics_size: 9568,
    max_static_size: 8128,
};

/// Assetto Corsa EVO.
///
/// Same three-map architecture as AC1 with larger structs (embedded sub-structs
/// for tyre state, damage, session state, etc.).
pub const EVO: GameConfig = GameConfig {
    id: "evo",
    name: "Assetto Corsa EVO",
    physics_map: "Local\\acevo_pmf_physics",
    graphics_map: "Local\\acevo_pmf_graphics",
    static_map: "Local\\acevo_pmf_static",
    max_physics_size: 4096,
    max_graphics_size: 16384,
    max_static_size: 4096,
};

/// Detection priority: EVO first (newer game), then AC1.
pub const DETECTION_ORDER: &[&GameConfig] = &[&EVO, &AC1];

/// Resolve a game id string ("ac1" or "evo") to its config.
pub fn resolve(id: &str) -> Option<&'static GameConfig> {
    match id {
        "ac1" => Some(&AC1),
        "evo" => Some(&EVO),
        _ => None,
    }
}

/// Probe shared memory to find the first running game in detection order (EVO → AC1).
///
/// Two-pass algorithm:
///   Pass 1 — liveness check: prefer a game whose packetId is advancing (live session).
///            This correctly handles stale EVO maps left over from a closed game.
///   Pass 2 — existence fallback: if no game has live data (both on menu or loading),
///            fall back to map-existence only. EVO retains priority in this case.
///
/// Returns `None` when neither game's maps are available at all.
pub fn detect() -> Option<&'static GameConfig> {
    // Pass 1: look for a game with an advancing packetId (active session).
    for &game in DETECTION_ORDER {
        if probe_live(game) {
            return Some(game);
        }
    }
    // Pass 2: neither game has live data — pick the first whose maps exist.
    DETECTION_ORDER.iter().copied().find(|&game| probe(game))
}

/// Open test handles to all three maps for `game` and immediately drop them.
/// Returns `true` if all three maps exist (game is running or recently was).
fn probe(game: &GameConfig) -> bool {
    // Each `.is_ok()` call drops the `Ok(SharedMap)` immediately, closing the handle.
    // We must not hold handles open while waiting — a held handle keeps the mapping
    // alive after the game exits, making stale data look like a running game.
    SharedMap::open(game.physics_map).is_ok()
        && SharedMap::open(game.graphics_map).is_ok()
        && SharedMap::open(game.static_map).is_ok()
}

/// Open the physics and graphics maps and check whether packetId advances over 200 ms.
/// Returns `true` only if at least one packetId changed — i.e. the game is in an
/// active session. Returns `false` for stale maps (game closed, packetId frozen) and
/// for maps where the game is on the menu (packetId = 0 throughout).
fn probe_live(game: &GameConfig) -> bool {
    let phys = match SharedMap::open(game.physics_map) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let gfx = match SharedMap::open(game.graphics_map) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if SharedMap::open(game.static_map).is_err() {
        return false;
    }

    let p1 = packet_id(phys.as_slice());
    let g1 = packet_id(gfx.as_slice());
    thread::sleep(Duration::from_millis(200));
    let p2 = packet_id(phys.as_slice());
    let g2 = packet_id(gfx.as_slice());

    p2 != p1 || g2 != g1
}

/// Read packetId (little-endian i32 at offset 0) from a shared memory slice.
fn packet_id(slice: &[u8]) -> i32 {
    if slice.len() < 4 {
        return 0;
    }
    i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_order_is_evo_first() {
        assert_eq!(DETECTION_ORDER[0].id, "evo");
        assert_eq!(DETECTION_ORDER[1].id, "ac1");
    }

    #[test]
    #[cfg(not(windows))]
    fn detect_returns_none_on_non_windows() {
        // MockSharedMap::open always returns Err, so detect() always returns None.
        assert!(detect().is_none());
    }

    #[test]
    #[cfg(not(windows))]
    fn probe_returns_false_on_non_windows() {
        assert!(!probe(&AC1));
        assert!(!probe(&EVO));
    }

    #[test]
    #[cfg(not(windows))]
    fn probe_live_returns_false_on_non_windows() {
        assert!(!probe_live(&AC1));
        assert!(!probe_live(&EVO));
    }

    #[test]
    fn packet_id_reads_le_i32() {
        assert_eq!(packet_id(&[0x01, 0x00, 0x00, 0x00]), 1);
        assert_eq!(packet_id(&[0xFF, 0xFF, 0xFF, 0x7F]), i32::MAX);
        assert_eq!(packet_id(&[0x00, 0x00, 0x00, 0x00]), 0);
    }

    #[test]
    fn packet_id_returns_zero_on_short_slice() {
        assert_eq!(packet_id(&[0x01, 0x02]), 0);
        assert_eq!(packet_id(&[]), 0);
    }
}
