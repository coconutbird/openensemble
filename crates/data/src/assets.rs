//! Unified asset loading from local files or ERA archives.
//!
//! [`AssetSource`] provides a single interface for loading assets with automatic
//! fallback through multiple sources:
//!
//! 1. **Override directory** - Local files for development/modding
//! 2. **Scenario ERA** - Scenario-specific assets (e.g., blood_gulch.era)
//! 3. **Root ERA** - Global assets (root.era)
//!
//! # Example
//!
//! ```ignore
//! use data::assets::AssetSource;
//!
//! // Create asset source for a scenario
//! let mut source = AssetSource::for_scenario("blood_gulch")?;
//!
//! // Read raw bytes
//! let bytes = source.read("art/terrain/stone_df.ddx")?;
//!
//! // With override directory for modding
//! let mut source = AssetSource::for_scenario("blood_gulch")?
//!     .with_override_dir("mods/my_mod");
//! ```

use crate::era::{DecryptReader, EraArchive};
use crate::paths::{GAME_DIR_ENV_VAR, era_path, game_dir, is_valid_game_dir};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Type alias for ERA archive opened from a file path.
pub type FileEraArchive = EraArchive<DecryptReader<BufReader<File>>>;

/// Errors that can occur during asset loading.
#[derive(Debug, Error)]
pub enum AssetError {
    #[error("Asset not found: {0}")]
    NotFound(String),

    #[error("Game directory not configured. Set {0} environment variable.")]
    GameDirNotSet(&'static str),

    #[error("Game directory is invalid: {0}")]
    InvalidGameDir(PathBuf),

    #[error("Failed to open ERA archive: {0}")]
    EraOpen(#[from] crate::era::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Pre-indexed ERA archive for fast lookups.
struct IndexedEra {
    archive: FileEraArchive,
    /// Maps normalized path (lowercase, backslashes) to entry index.
    index: HashMap<String, usize>,
}

impl IndexedEra {
    fn open(path: &Path) -> Result<Self, AssetError> {
        let archive = EraArchive::open(path)?;
        let index = archive
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                e.filename
                    .as_ref()
                    .map(|n| (n.replace('/', "\\").to_lowercase(), i))
            })
            .collect();
        Ok(Self { archive, index })
    }

    fn read(&mut self, normalized_path: &str) -> Option<Vec<u8>> {
        let idx = self.index.get(normalized_path)?;
        self.archive.read_entry(*idx).ok()
    }

    fn contains(&self, normalized_path: &str) -> bool {
        self.index.contains_key(normalized_path)
    }
}

/// Unified asset source for loading from overrides or ERA archives.
///
/// Holds ERA archives open for efficient repeated lookups.
/// Searches sources in order: override dir → scenario ERA → root ERA.
pub struct AssetSource {
    /// Local directory to check first (for development/modding).
    override_dir: Option<PathBuf>,
    /// Scenario ERA (e.g., blood_gulch.era).
    scenario_era: Option<IndexedEra>,
    /// Root ERA (root.era).
    root_era: Option<IndexedEra>,
}

impl AssetSource {
    /// Create an asset source for a scenario.
    ///
    /// Opens the scenario ERA and root ERA, building indexes for fast lookups.
    pub fn for_scenario(scenario_name: &str) -> Result<Self, AssetError> {
        if !is_valid_game_dir() {
            if std::env::var(GAME_DIR_ENV_VAR).is_err() {
                return Err(AssetError::GameDirNotSet(GAME_DIR_ENV_VAR));
            }
            return Err(AssetError::InvalidGameDir(game_dir().clone()));
        }

        let scenario_era_path = era_path(scenario_name);
        let root_era_path = era_path("root");

        let scenario_era = if scenario_era_path.exists() {
            log::info!("Opening scenario ERA: {}", scenario_era_path.display());
            Some(IndexedEra::open(&scenario_era_path)?)
        } else {
            log::warn!("Scenario ERA not found: {}", scenario_era_path.display());
            None
        };

        let root_era = if root_era_path.exists() {
            log::info!("Opening root ERA: {}", root_era_path.display());
            Some(IndexedEra::open(&root_era_path)?)
        } else {
            None
        };

        Ok(Self {
            override_dir: None,
            scenario_era,
            root_era,
        })
    }

    /// Create an asset source for root.era only (no scenario ERA).
    ///
    /// Use this when loading global game data (XMB files, etc.) that doesn't
    /// depend on a specific scenario.
    pub fn root_only() -> Result<Self, AssetError> {
        if !is_valid_game_dir() {
            if std::env::var(GAME_DIR_ENV_VAR).is_err() {
                return Err(AssetError::GameDirNotSet(GAME_DIR_ENV_VAR));
            }
            return Err(AssetError::InvalidGameDir(game_dir().clone()));
        }

        let root_era_path = era_path("root");

        let root_era = if root_era_path.exists() {
            log::info!("Opening root ERA: {}", root_era_path.display());
            Some(IndexedEra::open(&root_era_path)?)
        } else {
            return Err(AssetError::NotFound("root.era not found".to_string()));
        };

        Ok(Self {
            override_dir: None,
            scenario_era: None,
            root_era,
        })
    }

    /// Set an override directory for local file loading.
    ///
    /// Files in this directory take priority over ERA archives.
    pub fn with_override_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.override_dir = Some(dir.into());
        self
    }

    /// Read raw bytes for an asset path.
    ///
    /// Searches in order: override directory → scenario ERA → root ERA.
    ///
    /// Path format: `"art/terrain/stone_df.ddx"` (forward or back slashes accepted).
    pub fn read(&mut self, path: &str) -> Result<Vec<u8>, AssetError> {
        // Normalize path for comparison (lowercase, backslashes)
        let normalized = path.replace('/', "\\").to_lowercase();

        // 1. Try override directory
        if let Some(override_dir) = &self.override_dir {
            // Convert to forward slashes for filesystem
            let local_path = override_dir.join(path.replace('\\', "/"));
            if local_path.exists() {
                log::debug!("Loading from override: {}", local_path.display());
                return Ok(fs::read(&local_path)?);
            }
        }

        // 2. Try scenario ERA
        if let Some(era) = &mut self.scenario_era
            && let Some(data) = era.read(&normalized)
        {
            log::debug!("Loading from scenario ERA: {}", path);
            return Ok(data);
        }

        // 3. Try root ERA
        if let Some(era) = &mut self.root_era
            && let Some(data) = era.read(&normalized)
        {
            log::debug!("Loading from root ERA: {}", path);
            return Ok(data);
        }

        Err(AssetError::NotFound(path.to_string()))
    }

    /// Check if an asset exists without loading it.
    ///
    /// Searches in order: override directory → scenario ERA → root ERA.
    pub fn exists(&self, path: &str) -> bool {
        let normalized = path.replace('/', "\\").to_lowercase();

        // Check override directory
        if let Some(override_dir) = &self.override_dir {
            let local_path = override_dir.join(path.replace('\\', "/"));
            if local_path.exists() {
                return true;
            }
        }

        // Check scenario ERA
        if let Some(era) = &self.scenario_era
            && era.contains(&normalized)
        {
            return true;
        }

        // Check root ERA
        if let Some(era) = &self.root_era
            && era.contains(&normalized)
        {
            return true;
        }

        false
    }

    /// List all assets matching a predicate.
    ///
    /// Returns normalized paths (lowercase, backslashes).
    /// Note: Only searches ERA archives, not override directory.
    pub fn list<F>(&self, predicate: F) -> Vec<String>
    where
        F: Fn(&str) -> bool,
    {
        let mut results = Vec::new();

        // Collect from scenario ERA
        if let Some(era) = &self.scenario_era {
            for path in era.index.keys() {
                if predicate(path) && !results.contains(path) {
                    results.push(path.clone());
                }
            }
        }

        // Collect from root ERA (avoiding duplicates)
        if let Some(era) = &self.root_era {
            for path in era.index.keys() {
                if predicate(path) && !results.contains(path) {
                    results.push(path.clone());
                }
            }
        }

        results
    }
}
