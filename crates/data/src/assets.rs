//! Unified asset loading from local files or ERA archives.
//!
//! [`AssetSource`] provides a single interface for loading assets with automatic
//! fallback through multiple ERA archives, matching the original game's
//! `BArchiveManager` load order.
//!
//! ## ERA archive priority (highest to lowest)
//!
//! 1. **Override directory** - Local files for development/modding
//! 2. **Scenario ERA** - Scenario-specific assets (e.g., blood_gulch.era)
//! 3. **scenarioshared.era** - Assets shared across scenarios
//! 4. **root_update.era** - Patch/update overrides for root.era
//! 5. **root.era** - Global game assets
//! 6. **locale_update.era** - Patch/update overrides for locale.era
//! 7. **locale.era** - Localized content
//!
//! This matches the original `BArchiveManager::reloadRootArchive` and
//! `BArchiveManager::beginScenarioPrefetch` load chain from the game binary.
//!
//! # Example
//!
//! ```ignore
//! use data::assets::AssetSource;
//!
//! // Create asset source for a scenario (loads full ERA chain)
//! let source = AssetSource::for_scenario("blood_gulch")?;
//!
//! // Read raw bytes
//! let bytes = source.read("art/terrain/stone_df.ddx")?;
//!
//! // Parallel reading for multiple files
//! let paths = vec!["art/terrain/grass_df.ddx", "art/terrain/rock_df.ddx"];
//! let data = source.read_parallel(&paths);
//!
//! // With override directory for modding
//! let source = AssetSource::for_scenario("blood_gulch")?
//!     .with_override_dir("mods/my_mod");
//! ```

use crate::paths::{GAME_DIR_ENV_VAR, era_path, game_dir, is_valid_game_dir};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

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
///
/// Opens the ERA file with decryption and builds a path→index map.
/// The inner `Reader` is wrapped in a `Mutex` because `read_entry`
/// requires `&mut self` (it seeks within the file).
struct IndexedEra {
    reader: Mutex<era::Reader<era::DecryptReader<std::fs::File>>>,
    /// Maps normalized path (lowercase, backslashes) to entry index.
    index: HashMap<String, usize>,
}

impl IndexedEra {
    fn open(path: &Path) -> Result<Self, AssetError> {
        let file = std::fs::File::open(path)?;
        let keys = era::TeaKeys::default_archive_keys();
        let reader = era::Reader::from_encrypted(file, keys)?;
        let index = reader
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                e.filename
                    .as_ref()
                    .map(|n| (n.replace('/', "\\").to_lowercase(), i))
            })
            .collect();
        Ok(Self {
            reader: Mutex::new(reader),
            index,
        })
    }

    /// Read a single entry.
    fn read(&self, normalized_path: &str) -> Option<Vec<u8>> {
        let idx = self.index.get(normalized_path)?;
        let mut reader = self.reader.lock().ok()?;
        reader.read_entry(*idx).ok()
    }

    /// Read multiple entries.
    fn read_many(&self, normalized_paths: &[&str]) -> Vec<Option<Vec<u8>>> {
        let mut reader = self.reader.lock().unwrap();
        normalized_paths
            .iter()
            .map(|p| {
                self.index
                    .get(*p)
                    .and_then(|&idx| reader.read_entry(idx).ok())
            })
            .collect()
    }

    fn contains(&self, normalized_path: &str) -> bool {
        self.index.contains_key(normalized_path)
    }
}

/// Unified asset source for loading from overrides or ERA archives.
///
/// Maintains an ordered list of ERA archives matching the original game's
/// `BArchiveManager` load chain. Archives earlier in the list have higher
/// priority (later-loaded archives override earlier ones in the original game,
/// and we store them in search order: highest priority first).
pub struct AssetSource {
    /// Local directory to check first (for development/modding).
    override_dir: Option<PathBuf>,
    /// ERA archives in priority order (highest priority first).
    /// Matches BArchiveManager: scenario > scenarioshared > root_update > root > locale_update > locale.
    eras: Vec<IndexedEra>,
}

/// Names and labels for ERA archives, in priority order (highest first).
/// The original game loads them in reverse order (locale first, scenario last),
/// but searches them last-loaded-first, which is equivalent to our ordering.
const ERA_NAMES_ROOT: &[(&str, &str)] = &[
    ("root_update", "root_update.era (patch)"),
    ("root", "root.era"),
    ("locale_update", "locale_update.era (patch)"),
    ("locale", "locale.era"),
];

const ERA_NAMES_SCENARIO: &[(&str, &str)] = &[
    ("scenarioshared", "scenarioshared.era"),
];

impl AssetSource {
    /// Validate the game directory is set and valid.
    fn validate_game_dir() -> Result<(), AssetError> {
        if !is_valid_game_dir() {
            if std::env::var(GAME_DIR_ENV_VAR).is_err() {
                return Err(AssetError::GameDirNotSet(GAME_DIR_ENV_VAR));
            }
            return Err(AssetError::InvalidGameDir(game_dir().clone()));
        }
        Ok(())
    }

    /// Try to open an ERA archive, returning None if not found on disk.
    fn try_open_era(name: &str, label: &str) -> Result<Option<IndexedEra>, AssetError> {
        let path = era_path(name);
        if path.exists() {
            log::info!("Opening {}: {}", label, path.display());
            Ok(Some(IndexedEra::open(&path)?))
        } else {
            log::debug!("{} not found, skipping", label);
            Ok(None)
        }
    }

    /// Load the root ERA chain (root, root_update, locale, locale_update).
    ///
    /// Returns archives in priority order (root_update before root, etc).
    fn load_root_eras() -> Result<Vec<IndexedEra>, AssetError> {
        let mut eras = Vec::new();
        for &(name, label) in ERA_NAMES_ROOT {
            if let Some(era) = Self::try_open_era(name, label)? {
                eras.push(era);
            }
        }
        if eras.is_empty() {
            return Err(AssetError::NotFound("root.era not found".to_string()));
        }
        Ok(eras)
    }

    /// Create an asset source for a scenario.
    ///
    /// Opens the full ERA chain matching BArchiveManager:
    /// scenario ERA → scenarioshared.era → root_update.era → root.era →
    /// locale_update.era → locale.era
    pub fn for_scenario(scenario_name: &str) -> Result<Self, AssetError> {
        Self::validate_game_dir()?;

        let mut eras = Vec::new();

        // 1. Scenario-specific ERA (highest priority)
        let scenario_era_path = era_path(scenario_name);
        if scenario_era_path.exists() {
            log::info!("Opening scenario ERA: {}", scenario_era_path.display());
            eras.push(IndexedEra::open(&scenario_era_path)?);
        } else {
            log::warn!("Scenario ERA not found: {}", scenario_era_path.display());
        }

        // 2. scenarioshared.era
        for &(name, label) in ERA_NAMES_SCENARIO {
            if let Some(era) = Self::try_open_era(name, label)? {
                eras.push(era);
            }
        }

        // 3. Root chain (root_update, root, locale_update, locale)
        eras.extend(Self::load_root_eras()?);

        Ok(Self {
            override_dir: None,
            eras,
        })
    }

    /// Create an asset source for the root ERA chain only (no scenario ERA).
    ///
    /// Loads: root_update.era → root.era → locale_update.era → locale.era
    ///
    /// Use this when loading global game data (XMB files, etc.) that doesn't
    /// depend on a specific scenario.
    pub fn root_only() -> Result<Self, AssetError> {
        Self::validate_game_dir()?;

        let eras = Self::load_root_eras()?;

        Ok(Self {
            override_dir: None,
            eras,
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
    /// Searches in priority order: override directory → ERA chain
    /// (scenario → scenarioshared → root_update → root → locale_update → locale).
    ///
    /// Path format: `"art/terrain/stone_df.ddx"` (forward or back slashes accepted).
    pub fn read(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let normalized = path.replace('/', "\\").to_lowercase();

        // 1. Try override directory
        if let Some(override_dir) = &self.override_dir {
            let local_path = override_dir.join(path.replace('\\', "/"));
            if local_path.exists() {
                log::debug!("Loading from override: {}", local_path.display());
                return Ok(fs::read(&local_path)?);
            }
        }

        // 2. Try ERA chain in priority order
        for era in &self.eras {
            if let Some(data) = era.read(&normalized) {
                log::debug!("Loading from ERA: {}", path);
                return Ok(data);
            }
        }

        Err(AssetError::NotFound(path.to_string()))
    }

    /// Read multiple assets in parallel.
    ///
    /// Returns a Vec with the same length as `paths`, where each element is
    /// `Some(data)` if the asset was found, or `None` if not found.
    pub fn read_parallel(&self, paths: &[&str]) -> Vec<Option<Vec<u8>>> {
        let normalized: Vec<String> = paths
            .iter()
            .map(|p| p.replace('/', "\\").to_lowercase())
            .collect();
        let normalized_refs: Vec<&str> = normalized.iter().map(|s| s.as_str()).collect();

        // Check override directory first
        let mut results: Vec<Option<Vec<u8>>> = vec![None; paths.len()];
        let mut remaining_indices: Vec<usize> = Vec::new();

        if let Some(override_dir) = &self.override_dir {
            for (i, path) in paths.iter().enumerate() {
                let local_path = override_dir.join(path.replace('\\', "/"));
                if local_path.exists()
                    && let Ok(data) = fs::read(&local_path)
                {
                    log::debug!("Loading from override: {}", local_path.display());
                    results[i] = Some(data);
                    continue;
                }
                remaining_indices.push(i);
            }
        } else {
            remaining_indices = (0..paths.len()).collect();
        }

        if remaining_indices.is_empty() {
            return results;
        }

        // Walk the ERA chain in priority order, resolving remaining assets
        for era in &self.eras {
            if remaining_indices.is_empty() {
                break;
            }

            let era_paths: Vec<&str> = remaining_indices
                .iter()
                .map(|&i| normalized_refs[i])
                .collect();
            let era_results = era.read_many(&era_paths);

            let mut still_needed = Vec::new();
            for (j, &orig_idx) in remaining_indices.iter().enumerate() {
                if let Some(data) = era_results[j].clone() {
                    results[orig_idx] = Some(data);
                } else {
                    still_needed.push(orig_idx);
                }
            }
            remaining_indices = still_needed;
        }

        results
    }

    /// Check if an asset exists without loading it.
    ///
    /// Searches in priority order: override directory → ERA chain.
    pub fn exists(&self, path: &str) -> bool {
        let normalized = path.replace('/', "\\").to_lowercase();

        if let Some(override_dir) = &self.override_dir {
            let local_path = override_dir.join(path.replace('\\', "/"));
            if local_path.exists() {
                return true;
            }
        }

        self.eras.iter().any(|era| era.contains(&normalized))
    }

    /// List all assets matching a predicate.
    ///
    /// Returns normalized paths (lowercase, backslashes).
    /// Collects from all ERA archives, avoiding duplicates.
    /// Note: Only searches ERA archives, not override directory.
    pub fn list<F>(&self, predicate: F) -> Vec<String>
    where
        F: Fn(&str) -> bool,
    {
        let mut results = Vec::new();

        for era in &self.eras {
            for path in era.index.keys() {
                if predicate(path) && !results.contains(path) {
                    results.push(path.clone());
                }
            }
        }

        results
    }
}
