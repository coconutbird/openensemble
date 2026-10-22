//! Shared helpers for the integration tests.
//!
//! Use this crate only as a dev-dependency.
//!
//! - [`game_dir`] and [`installed_scenario`] read the game directory from
//!   `OPENENSEMBLE_GAME_DIR`. Use them only in tests that have
//!   `#[ignore = "requires a local Halo Wars DE installation"]`.
//! - [`assert_close`] compares two `f32` values with an absolute tolerance.

use std::path::Path;

/// The environment variable that names the Halo Wars DE installation.
pub const GAME_DIR_ENV: &str = "OPENENSEMBLE_GAME_DIR";

/// Read the Halo Wars DE installation directory from `OPENENSEMBLE_GAME_DIR`.
///
/// # Panics
///
/// Panics if the variable is unset, empty or does not name a directory.
#[must_use]
pub fn game_dir() -> String {
    let dir = std::env::var(GAME_DIR_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            panic!("{GAME_DIR_ENV} is not set: set it to a Halo Wars DE installation directory")
        });
    assert!(
        Path::new(&dir).is_dir(),
        "{GAME_DIR_ENV}={dir} is not a directory"
    );
    dir
}

/// Load an installed scenario and its layered database with [`sim::load_scenario_from_game_dir`].
///
/// # Panics
///
/// Panics if [`game_dir`] panics or if the scenario does not load.
#[must_use]
pub fn installed_scenario(name: &str) -> sim::LoadedGameScenario {
    sim::load_scenario_from_game_dir(&game_dir(), name)
        .unwrap_or_else(|error| panic!("installed scenario {name} should load: {error}"))
}

/// The absolute tolerance of [`assert_close`].
pub const CLOSE_TOLERANCE: f32 = 1.0e-6;

/// Assert that `actual` and `expected` differ by [`CLOSE_TOLERANCE`] or less.
///
/// The bound is inclusive. Do not use this function for a test that needs a relative tolerance
/// or a strict bound.
///
/// # Panics
///
/// Panics if `(actual - expected).abs()` is more than [`CLOSE_TOLERANCE`].
#[track_caller]
pub fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= CLOSE_TOLERANCE,
        "{actual} != {expected}"
    );
}
