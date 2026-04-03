//! GameDatabase - Comprehensive loader for all game data from ERA archives.
//!
//! This module loads all XMB files from root.era and provides both typed access
//! to known data types (objects, squads, techs, etc.) and raw XMB access for
//! everything else.
//!
//! # Example
//! ```ignore
//! use data::GameDatabase;
//!
//! let db = GameDatabase::load()?;
//! println!("Loaded {} objects", db.proto.objects.len());
//! println!("Loaded {} XMB files total", db.raw_xmb.len());
//! ```

mod parse_data;
mod parse_proto;
pub mod types;

use crate::assets::{AssetError, AssetSource};
use crate::proto::ProtoDatabase;
use crate::xmb::{Document, Error as XmbError, Reader};
use std::collections::HashMap;
use thiserror::Error;

pub use types::*;

/// Errors that can occur when loading the game database.
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Asset loading error: {0}")]
    Asset(#[from] AssetError),
    #[error("XMB parse error: {0}")]
    Xmb(#[from] XmbError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Parse error in {file}: {message}")]
    ParseError { file: String, message: String },
}

/// Comprehensive game database containing all loaded data.
#[derive(Debug, Default)]
pub struct GameDatabase {
    /// Proto objects, squads, and techs
    pub proto: ProtoDatabase,
    /// Civilizations by ID
    pub civs: HashMap<u32, Civilization>,
    /// Civilizations by name
    pub civs_by_name: HashMap<String, u32>,
    /// Leaders by ID
    pub leaders: HashMap<u32, Leader>,
    /// Leaders by name
    pub leaders_by_name: HashMap<String, u32>,
    /// Powers/leader abilities by ID
    pub powers: HashMap<u32, Power>,
    /// Powers by name
    pub powers_by_name: HashMap<String, u32>,
    /// Abilities by ID
    pub abilities: HashMap<u32, Ability>,
    /// Abilities by name
    pub abilities_by_name: HashMap<String, u32>,
    /// Weapon types by ID
    pub weapon_types: HashMap<u32, WeaponType>,
    /// Weapon types by name
    pub weapon_types_by_name: HashMap<String, u32>,
    /// Game modes by ID
    pub game_modes: HashMap<u32, GameMode>,
    /// Game modes by name
    pub game_modes_by_name: HashMap<String, u32>,
    /// Damage types by ID
    pub damage_types: HashMap<u32, DamageType>,
    /// Damage types by name
    pub damage_types_by_name: HashMap<String, u32>,
    /// Global game data constants (gamedata.xml)
    pub game_data: GameData,
    /// Object type groupings (objecttypes.xml) by ID
    pub object_types: HashMap<u32, ObjectTypeEntry>,
    /// Object type groupings by name
    pub object_types_by_name: HashMap<String, u32>,
    /// Terrain tile types (terrainTileTypes.xml) by ID
    pub terrain_tile_types: HashMap<u32, TerrainTileType>,
    /// Terrain tile types by name
    pub terrain_tile_types_by_name: HashMap<String, u32>,
    /// Player color data (playercolors.xml) — sets (spc/skirmish) + per-civ overrides
    pub player_color_data: PlayerColorData,
    /// Raw XMB data for all loaded files (by filename)
    pub raw_xmb: HashMap<String, Document>,
    /// List of all loaded XMB file paths
    pub loaded_files: Vec<String>,
}

impl GameDatabase {
    /// Create a new empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the game database using AssetSource.
    pub fn load() -> Result<Self, DatabaseError> {
        log::info!("Loading game database via AssetSource");

        let source = AssetSource::root_only()?;
        let mut db = GameDatabase::new();

        let xmb_files = source.list(|path| path.ends_with(".xmb"));
        log::info!("Found {} XMB files in root.era", xmb_files.len());

        for filename in xmb_files {
            match source.read(&filename) {
                Ok(data) => {
                    if let Err(e) = db.load_xmb_data(&filename, &data) {
                        log::warn!("Failed to parse {}: {}", filename, e);
                    }
                }
                Err(e) => {
                    log::warn!("Failed to read {}: {}", filename, e);
                }
            }
        }

        log::info!(
            "Loaded database: {} objects, {} squads, {} techs, {} civs, {} leaders",
            db.proto.objects.len(),
            db.proto.squads.len(),
            db.proto.techs.len(),
            db.civs.len(),
            db.leaders.len()
        );

        Ok(db)
    }

    /// Load XMB data from bytes and parse it.
    fn load_xmb_data(&mut self, filename: &str, data: &[u8]) -> Result<(), DatabaseError> {
        let xmb = Reader::read(data)?;

        self.loaded_files.push(filename.to_string());

        // Parse known file types
        // Note: update files (e.g. objects_update.xml) are handled by the same
        // parsers as the base files — they append/merge into the same collections.
        let filename_lower = filename.to_lowercase();
        if filename_lower.contains("objects.xml") && !filename_lower.contains("objecttypes") {
            self.parse_objects(&xmb, filename)?;
        } else if filename_lower.contains("squads.xml") {
            self.parse_squads(&xmb, filename)?;
        } else if filename_lower.contains("techs.xml") {
            self.parse_techs(&xmb, filename)?;
        } else if filename_lower.contains("civs.xml") {
            self.parse_civs(&xmb, filename)?;
        } else if filename_lower.contains("leaders.xml") {
            self.parse_leaders(&xmb, filename)?;
        } else if filename_lower.contains("powers.xml") {
            self.parse_powers(&xmb, filename)?;
        } else if filename_lower.contains("abilities.xml") {
            self.parse_abilities(&xmb, filename)?;
        } else if filename_lower.contains("weapontypes.xml") {
            self.parse_weapon_types(&xmb, filename)?;
        } else if filename_lower.contains("gamemodes.xml") {
            self.parse_game_modes(&xmb, filename)?;
        } else if filename_lower.contains("damagetypes.xml") {
            self.parse_damage_types(&xmb, filename)?;
        } else if filename_lower.contains("gamedata.xml") {
            self.parse_game_data(&xmb, filename)?;
        } else if filename_lower.contains("objecttypes.xml") {
            self.parse_object_types(&xmb, filename)?;
        } else if filename_lower.contains("terraintiletypes.xml") {
            self.parse_terrain_tile_types(&xmb, filename)?;
        } else if filename_lower.contains("playercolors.xml") {
            self.parse_player_colors(&xmb, filename)?;
        }

        // Always store raw XMB for any file
        self.raw_xmb.insert(filename.to_string(), xmb);

        Ok(())
    }

    /// Get a raw XMB file by path (case-insensitive search).
    pub fn get_xmb(&self, path: &str) -> Option<&Document> {
        let path_lower = path.to_lowercase();
        self.raw_xmb
            .iter()
            .find(|(k, _)| k.to_lowercase().contains(&path_lower))
            .map(|(_, v)| v)
    }

    /// Get civilization by ID.
    pub fn get_civ(&self, id: u32) -> Option<&Civilization> {
        self.civs.get(&id)
    }

    /// Get civilization by name.
    pub fn get_civ_by_name(&self, name: &str) -> Option<&Civilization> {
        self.civs_by_name.get(name).and_then(|id| self.civs.get(id))
    }

    /// Get leader by ID.
    pub fn get_leader(&self, id: u32) -> Option<&Leader> {
        self.leaders.get(&id)
    }

    /// Get leader by name.
    pub fn get_leader_by_name(&self, name: &str) -> Option<&Leader> {
        self.leaders_by_name
            .get(name)
            .and_then(|id| self.leaders.get(id))
    }

    // --- ID resolution helpers ---
    // These mirror the original engine's BDatabase__getProtoObjectID, etc.
    // They return -1 when the name is not found, matching the engine's sentinel.

    /// Resolve a proto object name to its ID, or -1 if not found.
    pub(crate) fn resolve_proto_object_id(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        self.proto
            .objects_by_name
            .get(name)
            .map(|&id| id as i32)
            .unwrap_or(-1)
    }

    /// Resolve a tech name to its ID, or -1 if not found.
    pub(crate) fn resolve_tech_id(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        self.proto
            .techs_by_name
            .get(name)
            .map(|&id| id as i32)
            .unwrap_or(-1)
    }

    /// Resolve a localized string ID. For now, stores the raw string hash/ID
    /// as parsed from the XML. Returns -1 if empty.
    pub(crate) fn resolve_loc_string_id(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        // The original engine resolves via BLocString__parseID then
        // BDatabase__getLocStringID. We store the raw numeric ID for now.
        name.parse::<i32>().unwrap_or(-1)
    }

    /// Resolve a proto squad name to its ID, or -1 if not found.
    pub(crate) fn resolve_proto_squad_id(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        self.proto
            .squads_by_name
            .get(name)
            .map(|&id| id as i32)
            .unwrap_or(-1)
    }

    /// Resolve a power name to its ID, or -1 if not found.
    pub(crate) fn resolve_power_id(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        self.powers_by_name
            .get(name)
            .map(|&id| id as i32)
            .unwrap_or(-1)
    }

    /// Resolve a civ name to its ID, or -1 if not found.
    pub(crate) fn resolve_civ_id(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        self.civs_by_name
            .get(name)
            .map(|&id| id as i32)
            .unwrap_or(-1)
    }

    /// Resolve a resource type name (e.g. "Supplies", "Power") to its ID, or -1.
    /// Uses the same resource string table as the engine (qword_14151C1F0).
    pub(crate) fn resolve_resource_type_id(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        // Hardcoded resource types matching the engine's resource string table order
        match name {
            "Supplies" => 0,
            "Power" => 1,
            "LeaderPowerCharge" => 2,
            _ => -1,
        }
    }

    /// Resolve a pop type name to its ID, or -1.
    /// Uses the same pop type lookup as the engine (sub_140215D90).
    pub(crate) fn resolve_pop_type_id(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        // Pop types are stored in the game_data.pops array by parse order
        for pop in &self.game_data.pops {
            if pop.name.eq_ignore_ascii_case(name) {
                return pop.id as i32;
            }
        }
        -1
    }
}
