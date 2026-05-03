use crate::maps::SharedMap;

/// Per-game shared memory configuration.
///
/// The `max_*_size` fields are upper bounds for pre-allocating LZ4 buffers.
/// Actual region sizes are queried at runtime via VirtualQuery on the source side.
/// The target creates maps at these sizes so all decompressed data fits.
pub struct GameConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub process_name: &'static str,
    pub physics_map: &'static str,
    pub graphics_map: &'static str,
    pub static_map: &'static str,
    /// Upper bound for physics page buffer allocation.
    pub max_physics_size: usize,
    /// Upper bound for graphics page buffer allocation.
    pub max_graphics_size: usize,
    /// Upper bound for static page buffer allocation.
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
    process_name: "acs.exe",
    physics_map: "Local\\acpmf_physics",
    graphics_map: "Local\\acpmf_graphics",
    static_map: "Local\\acpmf_static",
    max_physics_size: 2048,
    max_graphics_size: 4096,
    max_static_size: 2048,
};

/// Assetto Corsa EVO.
///
/// Same three-map architecture as AC1 with larger structs (embedded sub-structs
/// for tyre state, damage, session state, etc.).
pub const EVO: GameConfig = GameConfig {
    id: "evo",
    name: "Assetto Corsa EVO",
    process_name: "AssettoCorsaEVO.exe",
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
/// Test handles are opened and immediately dropped — no persistent handles are held.
/// Returns `None` when neither game's maps are available.
pub fn detect() -> Option<&'static GameConfig> {
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
}
