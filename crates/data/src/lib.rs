//! Thin wrapper over `ensemble-assets` pipeline for OpenEnsemble game data.
//!
//! This crate re-exports types from the [`pipeline`] crate and adds
//! OpenEnsemble-specific convenience (game directory discovery, etc.).
//!
//! ## Re-exported crates
//!
//! All format crates are available via `pipeline::` re-exports:
//! - [`pipeline::xmb`] — XMB binary XML
//! - [`pipeline::ddx`] — DDX textures
//! - [`pipeline::xtd`] — XTD terrain geometry
//! - [`pipeline::xtt`] — XTT terrain textures
//! - [`pipeline::database`] — Game database types (objects, civs, leaders, etc.)

pub mod paths;

// Re-export the pipeline crate as the primary API surface
pub use pipeline;

// Re-export format crates from pipeline for convenience
pub use pipeline::ddx;
pub use pipeline::database;
pub use pipeline::xmb;
pub use pipeline::xtd;
pub use pipeline::xtt;

// Re-export key types at crate root for ergonomics
pub use pipeline::source::{AssetSource, StdFileProvider};
pub use pipeline::hw1::loader;
pub use pipeline::hw1::scenario::{ScenarioData, ScenarioDescriptor, ScenarioList, ScenarioObject, ScenarioPlayer, ScenarioPosition};
pub use pipeline::database::hw1::{Database as GameDatabase, ProtoObject, Civ, Leader};

pub use paths::{GAME_DIR_ENV_VAR, game_dir, game_file, is_valid_game_dir};

/// Create an [`AssetSource`] from the configured game directory.
///
/// Uses [`paths::game_dir()`] to locate the installation, then loads
/// ERAs in the engine's confirmed load order via [`pipeline::hw1::loader`].
pub fn load_game_assets() -> AssetSource<StdFileProvider> {
    let dir = paths::game_dir();
    loader::load_game_dir(&dir.to_string_lossy())
}

/// Create an [`AssetSource`] with a scenario ERA layered on top.
pub fn load_scenario_assets(scenario_era: &str) -> AssetSource<StdFileProvider> {
    let dir = paths::game_dir();
    loader::load_with_scenario(&dir.to_string_lossy(), scenario_era)
}
