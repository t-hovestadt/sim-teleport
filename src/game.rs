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

/// Resolve a game id string ("ac1" or "evo") to its config.
pub fn resolve(id: &str) -> Option<&'static GameConfig> {
    match id {
        "ac1" => Some(&AC1),
        "evo" => Some(&EVO),
        _ => None,
    }
}
