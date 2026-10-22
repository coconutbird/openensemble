//! Shared code for the `OpenEnsemble` executables.
//!
//! This library holds the installation policy that the executables share. It finds the
//! directory of the installed game.

use std::path::PathBuf;
use std::sync::LazyLock;

/// The name of the environment variable that sets the game directory.
pub const GAME_DIR_ENV_VAR: &str = "OPENENSEMBLE_GAME_DIR";

/// The game directory. The first access finds, logs and caches it.
static GAME_DIR: LazyLock<PathBuf> = LazyLock::new(find_game_dir);

/// Returns the directory of the installed game.
///
/// The function uses the path in [`GAME_DIR_ENV_VAR`] if that path exists. Otherwise, it uses
/// the current working directory. The first call logs and caches the result.
#[must_use]
pub fn game_dir() -> &'static PathBuf {
    &GAME_DIR
}

fn find_game_dir() -> PathBuf {
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
}
