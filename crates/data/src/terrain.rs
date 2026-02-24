//! Terrain data loading for Halo Wars scenarios.
//!
//! Provides [`Terrain`] struct containing XTD (geometry) and XTT (texture) data.
//! Load terrain through [`Scenario::load_terrain()`](crate::Scenario::load_terrain).
//!
//! # Example
//!
//! ```ignore
//! use data::Scenario;
//!
//! let scenario = Scenario::load("blood_gulch")?;
//! let terrain = scenario.load_terrain()?;
//!
//! // Access terrain data
//! println!("Terrain vertices: {}x{}", terrain.xtd.header.num_x_verts, terrain.xtd.header.num_x_verts);
//! if let Some(xtt) = &terrain.xtt {
//!     println!("Terrain textures: {}", xtt.header.num_active_textures);
//! }
//! ```

use crate::assets::{AssetError, AssetSource};
use crate::xtd::{XtdFile, XtdReader};
use crate::xtt::{XttFile, XttReader};
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when loading terrain data.
#[derive(Debug, Error)]
pub enum TerrainError {
    #[error("Game directory is invalid (root.era not found): {0}")]
    InvalidGameDir(PathBuf),

    #[error("XTD file not found: {0}")]
    XtdNotFound(String),

    #[error("Asset loading error: {0}")]
    Asset(#[from] AssetError),

    #[error("Failed to parse XTD: {0}")]
    XtdParse(#[from] crate::xtd::Error),

    #[error("Failed to parse XTT: {0}")]
    XttParse(#[from] crate::xtt::Error),
}

/// Loaded terrain data for a scenario.
///
/// Contains the raw XTD (geometry) and XTT (texture) data.
/// For rendering, use the loading functions in [`render::terrain`] to
/// extract textures and splat data.
pub struct Terrain {
    /// Terrain geometry and height data.
    pub xtd: XtdFile,
    /// Terrain texturing data (optional - some scenarios may not have it).
    pub xtt: Option<XttFile>,
}

impl Terrain {
    /// Load terrain for a scenario by name.
    ///
    /// Requires `OPENENSEMBLE_GAME_DIR` to be set to the Halo Wars installation.
    /// Uses AssetSource which checks override directory first, then ERA archives.
    pub(crate) fn load(scenario_name: &str) -> Result<Self, TerrainError> {
        log::info!("Loading terrain for scenario: {}", scenario_name);

        let source = AssetSource::for_scenario(scenario_name)?;

        // Find and load XTD file
        let xtd_files = source.list(|path| path.ends_with(".xtd"));
        let xtd_path = xtd_files
            .first()
            .ok_or_else(|| TerrainError::XtdNotFound(format!("{}.xtd", scenario_name)))?;

        let xtd_data = source.read(xtd_path)?;
        let xtd = XtdReader::read(&xtd_data)?;
        log::info!(
            "Loaded XTD: {}x{} vertices",
            xtd.header.num_x_verts,
            xtd.header.num_x_verts
        );

        // Find and load XTT file (optional)
        let xtt_files = source.list(|path| path.ends_with(".xtt"));
        let xtt = if let Some(xtt_path) = xtt_files.first() {
            match source.read(xtt_path) {
                Ok(data) => match XttReader::read(&data) {
                    Ok(xtt) => {
                        log::info!(
                            "Loaded XTT: {} textures, {} chunks",
                            xtt.header.num_active_textures,
                            xtt.linkers.len()
                        );
                        Some(xtt)
                    }
                    Err(e) => {
                        log::warn!("Failed to parse XTT: {}", e);
                        None
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read XTT: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self { xtd, xtt })
    }
}
