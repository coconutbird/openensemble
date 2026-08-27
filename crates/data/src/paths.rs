//! Game path utilities for locating Halo Wars game files.
//!
//! The game directory is determined by:
//! 1. The `OPENENSEMBLE_GAME_DIR` environment variable (if set)
//! 2. Falls back to the current working directory
//!
//! # Example
//! ```ignore
//! // Set the game directory via environment variable:
//! // export OPENENSEMBLE_GAME_DIR="/path/to/HaloWarsDE"
//!
//! use data::paths;
//!
//! let game_dir = paths::game_dir();
//! let root_era = paths::era_path("root");
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

/// Environment variable name for the game directory.
pub const GAME_DIR_ENV_VAR: &str = "OPENENSEMBLE_GAME_DIR";

/// Cached game directory path.
static GAME_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Get the game data directory.
///
/// Checks the `OPENENSEMBLE_GAME_DIR` environment variable first,
/// then falls back to the current working directory.
///
/// The result is cached after the first call.
pub fn game_dir() -> &'static PathBuf {
    GAME_DIR.get_or_init(|| {
        if let Ok(path) = std::env::var(GAME_DIR_ENV_VAR) {
            let path = PathBuf::from(path);
            if path.exists() {
                log::info!(
                    "Using game directory from {}: {}",
                    GAME_DIR_ENV_VAR,
                    path.display()
                );
                return path;
            }
            log::warn!(
                "{} is set to '{}' but path does not exist, falling back to current directory",
                GAME_DIR_ENV_VAR,
                path.display()
            );
        }

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        log::info!(
            "Using current directory as game directory: {}",
            cwd.display()
        );
        cwd
    })
}

/// Get the full path to an ERA archive by name.
///
/// # Arguments
/// * `name` - The ERA archive name without extension (e.g., "root", "`blood_gulch`")
///
/// # Returns
/// Full path to the ERA file (e.g., "/path/to/game/root.era")
#[must_use]
pub fn era_path(name: &str) -> PathBuf {
    game_dir().join(format!("{name}.era"))
}

/// Get the full path to a file within the game directory.
///
/// # Arguments
/// * `relative_path` - Path relative to the game directory
///
/// # Returns
/// Full path to the file
#[must_use]
pub fn game_file(relative_path: &str) -> PathBuf {
    game_dir().join(relative_path)
}

/// Check if the game directory appears to be valid.
///
/// Looks for the presence of expected files like "root.era".
#[must_use]
pub fn is_valid_game_dir() -> bool {
    era_path("root").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_era_path_format() {
        // We can't test the actual path since it depends on env/cwd,
        // but we can verify the format is correct
        let path = era_path("test_archive");
        assert!(path.to_string_lossy().ends_with("test_archive.era"));
    }

    #[test]
    fn test_game_file_format() {
        let path = game_file("data/objects.xml");
        assert!(path.to_string_lossy().contains("data"));
        assert!(path.to_string_lossy().ends_with("objects.xml"));
    }
}
