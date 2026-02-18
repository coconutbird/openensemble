//! Archive manager for loading game data from ERA files.
//!
//! ERA (Ensemble Resource Archive) files are the main archive format used by Halo Wars.
//! This module provides utilities to load scenarios and other game data from these archives.

use crate::scenario::{Scenario, ScenarioError, ScenarioLoader};
use data::era::{EraArchive, Error as EraError};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur when loading from archives.
#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("ERA archive error: {0}")]
    Era(#[from] EraError),
    #[error("Scenario loading error: {0}")]
    Scenario(#[from] ScenarioError),
    #[error("File not found in archive: {0}")]
    FileNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Archive manager for loading game data from ERA files.
///
/// This mirrors the original BArchiveManager from the game, providing
/// methods to load scenarios and other game data from ERA archives.
pub struct ArchiveManager;

impl ArchiveManager {
    /// Load a scenario from an ERA archive.
    ///
    /// The scenario file is expected to be named `<map_name>.scn` inside the archive.
    ///
    /// # Arguments
    /// * `era_path` - Path to the ERA archive file (e.g., "blood_gulch.era")
    ///
    /// # Returns
    /// The loaded scenario, or an error if loading fails.
    ///
    /// # Example
    /// ```ignore
    /// let scenario = ArchiveManager::load_scenario_from_era("blood_gulch.era")?;
    /// println!("Loaded scenario with {} players", scenario.players.len());
    /// ```
    pub fn load_scenario_from_era<P: AsRef<Path>>(era_path: P) -> Result<Scenario, ArchiveError> {
        let era_path = era_path.as_ref();

        // Open the ERA archive
        let mut archive = EraArchive::open(era_path)?;

        // Find the .scn.xmb file in the archive
        // The scenario file is typically named after the map (e.g., "blood_gulch.scn.xmb")
        let scn_index = archive
            .iter()
            .enumerate()
            .find(|(_, entry)| {
                entry.filename.as_ref().is_some_and(|name| {
                    name.ends_with(".scn.xmb") || name.ends_with(".scn")
                })
            })
            .map(|(i, _)| i)
            .ok_or_else(|| {
                ArchiveError::FileNotFound(format!(
                    "No .scn or .scn.xmb file found in archive: {}",
                    era_path.display()
                ))
            })?;

        // Read and decompress the scenario file
        let scn_data = archive.read_entry(scn_index)?;

        // Parse the XMB data
        let scenario = ScenarioLoader::load_xmb_bytes(&scn_data)?;

        Ok(scenario)
    }

    /// List all files in an ERA archive.
    ///
    /// Useful for debugging and exploring archive contents.
    pub fn list_era_contents<P: AsRef<Path>>(era_path: P) -> Result<Vec<String>, ArchiveError> {
        let archive = EraArchive::open(era_path)?;
        Ok(archive
            .iter()
            .filter_map(|e| e.filename.clone())
            .collect())
    }

    /// Load a specific file from an ERA archive by name.
    ///
    /// # Arguments
    /// * `era_path` - Path to the ERA archive
    /// * `filename` - Name of the file to extract (case-insensitive)
    ///
    /// # Returns
    /// The raw bytes of the file, or an error if not found.
    pub fn load_file_from_era<P: AsRef<Path>>(
        era_path: P,
        filename: &str,
    ) -> Result<Vec<u8>, ArchiveError> {
        let mut archive = EraArchive::open(era_path.as_ref())?;

        // Find the file (case-insensitive)
        let filename_lower = filename.to_lowercase();
        let index = archive
            .iter()
            .enumerate()
            .find(|(_, entry)| {
                entry
                    .filename
                    .as_ref()
                    .is_some_and(|name| name.to_lowercase() == filename_lower)
            })
            .map(|(i, _)| i)
            .ok_or_else(|| ArchiveError::FileNotFound(filename.to_string()))?;

        Ok(archive.read_entry(index)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests require actual game files and are ignored by default
    // Run with: cargo test --package sim -- archive --ignored

    #[test]
    #[ignore = "requires game files"]
    fn test_list_era_contents() {
        let era_path = r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE\blood_gulch.era";
        let contents = ArchiveManager::list_era_contents(era_path).unwrap();
        println!("Files in blood_gulch.era:");
        for file in &contents {
            println!("  {}", file);
        }
        assert!(!contents.is_empty());
    }

    #[test]
    #[ignore = "requires game files"]
    fn test_load_scenario_from_era() {
        let era_path = r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE\blood_gulch.era";
        let scenario = ArchiveManager::load_scenario_from_era(era_path).unwrap();

        println!("=== Loaded Scenario ===");
        println!("Name: {:?}", scenario.name);

        println!("\n--- Positions ({}) ---", scenario.positions.len());
        for pos in &scenario.positions {
            println!(
                "  Position {}: ({:.1}, {:.1}, {:.1})",
                pos.number, pos.position.x, pos.position.y, pos.position.z
            );
        }

        println!("\n--- Players ({}) ---", scenario.players.len());
        for (i, player) in scenario.players.iter().enumerate() {
            println!(
                "  [{}] {} - Civ: {:?}, Team: {:?}",
                i, player.name, player.civ_id, player.team_id
            );
        }

        println!("\n--- Objects ({}) ---", scenario.objects.len());
        // Show first 10 objects
        for obj in scenario.objects.iter().take(10) {
            println!(
                "  {} (player {}) at ({:.1}, {:.1}, {:.1})",
                obj.proto_name, obj.player_id, obj.position.x, obj.position.y, obj.position.z
            );
        }
        if scenario.objects.len() > 10 {
            println!("  ... and {} more", scenario.objects.len() - 10);
        }

        // Verify we loaded something
        assert!(!scenario.players.is_empty(), "Should have players");
        assert!(!scenario.objects.is_empty(), "Should have objects");
    }
}

