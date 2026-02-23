//! Scenario-based terrain loading.
//!
//! Provides unified loading of XTD (terrain geometry) and XTT (terrain textures)
//! from either local files or ERA archives via [`AssetSource`].
//!
//! # Loading Priority
//!
//! 1. **Local files** - Checks for extracted files in the game directory first
//!    (useful for development and modding)
//! 2. **AssetSource** - Falls back to loading from ERA archives

use crate::assets::{AssetError, AssetSource};
use crate::paths::{GAME_DIR_ENV_VAR, era_path, game_dir, is_valid_game_dir};
use crate::xtd::{XtdFile, XtdReader};
use crate::xtt::{XttFile, XttReader};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur when loading terrain data.
#[derive(Debug, Error)]
pub enum TerrainError {
    #[error("Game directory not configured. Set {0} environment variable.")]
    GameDirNotSet(&'static str),

    #[error("Game directory is invalid (root.era not found): {0}")]
    InvalidGameDir(PathBuf),

    #[error("XTD file not found: {0}")]
    XtdNotFound(String),

    #[error("XTT file not found: {0}")]
    XttNotFound(String),

    #[error("Asset loading error: {0}")]
    Asset(#[from] AssetError),

    #[error("Failed to parse XTD: {0}")]
    XtdParse(#[from] crate::xtd::Error),

    #[error("Failed to parse XTT: {0}")]
    XttParse(#[from] crate::xtt::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Loaded terrain data for a scenario.
pub struct ScenarioTerrain {
    /// Terrain geometry and height data
    pub xtd: XtdFile,
    /// Terrain texturing data (optional - some scenarios may not have it)
    pub xtt: Option<XttFile>,
    /// The scenario name this terrain was loaded from
    pub scenario_name: String,
    /// Path to the ERA file (for loading textures later)
    pub era_path: PathBuf,
}

impl ScenarioTerrain {
    /// Load terrain for a scenario by name.
    ///
    /// Requires `OPENENSEMBLE_GAME_DIR` to be set to the Halo Wars installation.
    ///
    /// Tries to load from local files first, then falls back to ERA archive.
    pub fn load(scenario_name: &str) -> Result<Self, TerrainError> {
        // Validate game directory is set and valid
        if !is_valid_game_dir() {
            if std::env::var(GAME_DIR_ENV_VAR).is_err() {
                return Err(TerrainError::GameDirNotSet(GAME_DIR_ENV_VAR));
            }
            return Err(TerrainError::InvalidGameDir(game_dir().clone()));
        }

        let era_file_path = era_path(scenario_name);

        // Try loading from local files first
        if let Some(terrain) = Self::try_load_from_files(scenario_name, &era_file_path)? {
            return Ok(terrain);
        }

        // Fall back to ERA archive
        Self::load_from_era(scenario_name, &era_file_path)
    }

    /// Try to load terrain from local extracted files.
    ///
    /// Returns `Ok(None)` if local files don't exist.
    fn try_load_from_files(
        scenario_name: &str,
        era_file_path: &Path,
    ) -> Result<Option<Self>, TerrainError> {
        // Common locations for extracted files (relative to game dir)
        let possible_paths = [
            // Same directory as ERA, extracted
            era_file_path.with_extension("xtd"),
            // Scenario subdirectory structure
            game_dir().join(format!(
                "scenario/skirmish/design/{}/{}.xtd",
                scenario_name, scenario_name
            )),
        ];

        for xtd_path in &possible_paths {
            if xtd_path.exists() {
                log::info!("Loading XTD from local file: {}", xtd_path.display());

                let xtd_data = std::fs::read(xtd_path)?;
                let xtd = XtdReader::read(&xtd_data)?;

                // Try to load XTT from same location
                let xtt_path = xtd_path.with_extension("xtt");
                let xtt = if xtt_path.exists() {
                    log::info!("Loading XTT from local file: {}", xtt_path.display());
                    let xtt_data = std::fs::read(&xtt_path)?;
                    Some(XttReader::read(&xtt_data)?)
                } else {
                    log::info!("No local XTT file found");
                    None
                };

                return Ok(Some(Self {
                    xtd,
                    xtt,
                    scenario_name: scenario_name.to_string(),
                    era_path: era_file_path.to_path_buf(),
                }));
            }
        }

        Ok(None)
    }

    /// Load terrain from ERA archive using AssetSource.
    fn load_from_era(scenario_name: &str, era_file_path: &Path) -> Result<Self, TerrainError> {
        log::info!("Loading terrain via AssetSource for: {}", scenario_name);

        let mut source = AssetSource::for_scenario(scenario_name)?;

        // Find XTD file in archive (by extension)
        let xtd_files = source.list(|path| path.ends_with(".xtd"));
        let xtd_path = xtd_files
            .first()
            .ok_or_else(|| TerrainError::XtdNotFound(format!("{}.xtd", scenario_name)))?;

        let xtd_data = source.read(xtd_path)?;
        let xtd = XtdReader::read(&xtd_data)?;
        log::info!(
            "Loaded XTD from ERA: {}x{} vertices",
            xtd.header.num_x_verts,
            xtd.header.num_x_verts
        );

        // Find XTT file in archive (optional)
        let xtt_files = source.list(|path| path.ends_with(".xtt"));
        let xtt = if let Some(xtt_path) = xtt_files.first() {
            match source.read(xtt_path) {
                Ok(data) => match XttReader::read(&data) {
                    Ok(xtt) => {
                        log::info!(
                            "Loaded XTT from ERA: {} textures, {} chunks",
                            xtt.header.num_active_textures,
                            xtt.linkers.len()
                        );
                        Some(xtt)
                    }
                    Err(e) => {
                        log::warn!("Failed to parse XTT from ERA: {}", e);
                        None
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read XTT from ERA: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            xtd,
            xtt,
            scenario_name: scenario_name.to_string(),
            era_path: era_file_path.to_path_buf(),
        })
    }

    /// Get the path to the scenario's ERA archive for loading additional assets.
    #[deprecated(note = "Use AssetSource::for_scenario() instead for loading assets")]
    pub fn era_path(&self) -> &PathBuf {
        &self.era_path
    }
}
