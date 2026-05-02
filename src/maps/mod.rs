/// Error returned when opening or creating a shared memory map.
#[derive(Debug)]
pub enum MapError {
    /// The named mapping does not exist (game not running).
    Unavailable,
    /// Any other OS error.
    Other(String),
}

impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapError::Unavailable => write!(f, "shared memory map not available"),
            MapError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for MapError {}

// ── Platform dispatch ─────────────────────────────────────────────────────────

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::WindowsSharedMap as SharedMap;

#[cfg(not(windows))]
mod mock;
#[cfg(not(windows))]
pub use mock::MockSharedMap as SharedMap;
