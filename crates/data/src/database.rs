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
//! let db = GameDatabase::load_from_era("root.era")?;
//! println!("Loaded {} objects", db.proto.objects.len());
//! println!("Loaded {} XMB files total", db.raw_xmb.len());
//! ```

use crate::era::{EraArchive, Error as EraError};
use crate::proto::{
    ObjectType, ProtoDatabase, ProtoFlags, ProtoId, ProtoObject, ProtoSquad, ProtoTech,
    ResourceCost, SquadUnit, TechEffect,
};
use crate::xmb::{Error as XmbError, Node, XmbData, XmbReader};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur when loading the game database.
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("ERA archive error: {0}")]
    Era(#[from] EraError),
    #[error("XMB parse error: {0}")]
    Xmb(#[from] XmbError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File not found in archive: {0}")]
    FileNotFound(String),
    #[error("Parse error in {file}: {message}")]
    ParseError { file: String, message: String },
}

/// Civilization definition.
#[derive(Debug, Clone, Default)]
pub struct Civilization {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub command_ack_object: String,
    pub rally_point_object: String,
    pub local_object: String,
    pub transport_proto: String,
    pub leaders: Vec<String>,
}

/// Leader definition.
#[derive(Debug, Clone, Default)]
pub struct Leader {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub civ: String,
    pub icon: String,
    pub leader_powers: Vec<String>,
    pub starting_unit: String,
    pub starting_squad: String,
    pub tech: String,
}

/// Power/ability definition.
#[derive(Debug, Clone, Default)]
pub struct Power {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub icon: String,
    pub tech_prereq: String,
    pub power_type: String,
    pub auto_recharge: f32,
    pub use_limit: i32,
}

/// Ability definition.
#[derive(Debug, Clone, Default)]
pub struct Ability {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub ability_type: String,
    pub recover_time: f32,
    pub movement_modifier: f32,
}

/// Weapon type definition.
#[derive(Debug, Clone, Default)]
pub struct WeaponType {
    pub id: u32,
    pub name: String,
    pub damage_per_second: f32,
    pub dps_variance: f32,
    pub max_range: f32,
    pub accuracy: f32,
    pub moving_accuracy: f32,
    pub visual: String,
}

/// Game mode definition.
#[derive(Debug, Clone, Default)]
pub struct GameMode {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub game_type: String,
    pub max_players: u32,
    pub starting_resources: ResourceCost,
}

/// Damage type definition.
#[derive(Debug, Clone, Default)]
pub struct DamageType {
    pub id: u32,
    pub name: String,
    pub shielded: bool,
    pub attenuates: bool,
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
    /// Raw XMB data for all loaded files (by filename)
    pub raw_xmb: HashMap<String, XmbData>,
    /// List of all loaded XMB file paths
    pub loaded_files: Vec<String>,
}

impl GameDatabase {
    /// Create a new empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the game database from root.era.
    ///
    /// This loads ALL .xmb files from the archive, parsing known types
    /// into their structured forms while keeping raw XMB data for everything.
    pub fn load_from_era<P: AsRef<Path>>(era_path: P) -> Result<Self, DatabaseError> {
        let era_path = era_path.as_ref();
        log::info!("Loading game database from: {}", era_path.display());

        let mut archive = EraArchive::open(era_path)?;
        let mut db = GameDatabase::new();

        // Collect all XMB file indices
        let xmb_entries: Vec<(usize, String)> = archive
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                entry.filename.as_ref().and_then(|name| {
                    if name.to_lowercase().ends_with(".xmb") {
                        Some((i, name.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect();

        log::info!("Found {} XMB files in archive", xmb_entries.len());

        // Load all XMB files
        for (index, filename) in xmb_entries {
            match archive.read_entry(index) {
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

    /// Load the game database from the game directory.
    ///
    /// Uses `OPENENSEMBLE_GAME_DIR` environment variable or falls back to
    /// the current working directory. Loads `root.era` from that location.
    pub fn load_from_game_dir() -> Result<Self, DatabaseError> {
        let root_era = crate::paths::era_path("root");
        Self::load_from_era(root_era)
    }

    /// Load XMB data from bytes and parse it.
    fn load_xmb_data(&mut self, filename: &str, data: &[u8]) -> Result<(), DatabaseError> {
        let cursor = Cursor::new(data);
        let xmb = XmbReader::read(cursor)?;

        self.loaded_files.push(filename.to_string());

        // Parse known file types
        let filename_lower = filename.to_lowercase();
        if filename_lower.contains("objects.xml") {
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
        }

        // Always store raw XMB for any file
        self.raw_xmb.insert(filename.to_string(), xmb);

        Ok(())
    }

    /// Get a raw XMB file by path (case-insensitive search).
    pub fn get_xmb(&self, path: &str) -> Option<&XmbData> {
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

    // ========================================================================
    // Parsing functions for known XMB file types
    // ========================================================================

    fn parse_objects(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: ProtoId = 1;

        for node in &root.children {
            if node.name == "Object" {
                let obj = Self::parse_proto_object(node, &mut id_counter);
                self.proto.add_object(obj);
            }
        }

        Ok(())
    }

    fn parse_proto_object(node: &Node, id_counter: &mut ProtoId) -> ProtoObject {
        let mut obj = ProtoObject {
            id: *id_counter,
            ..Default::default()
        };
        *id_counter += 1;

        // Get name from attribute or child
        if let Some(name_attr) = node.get_attribute("name") {
            obj.name = name_attr.value_string();
        }

        for child in &node.children {
            match child.name.as_str() {
                "Name" => obj.name = child.text_string(),
                "DisplayNameID" => obj.display_name = child.text_string(),
                "Hitpoints" => obj.hitpoints = child.text_string().parse().unwrap_or(0.0),
                "ShieldPoints" => obj.shield_points = child.text_string().parse().unwrap_or(0.0),
                "MaxVelocity" => obj.movement_speed = child.text_string().parse().unwrap_or(0.0),
                "BuildPoints" => obj.build_time = child.text_string().parse().unwrap_or(0.0),
                "Cost" => Self::parse_cost(child, &mut obj.cost),
                "PopCap" => obj.population_cost = child.text_string().parse().unwrap_or(0),
                "PopMax" => obj.population_capacity = child.text_string().parse().unwrap_or(0),
                "ObjectClass" => obj.object_type = Self::parse_object_class(&child.text_string()),
                "Flag" => Self::parse_flag(child, &mut obj.flags),
                _ => {}
            }
        }

        obj
    }

    fn parse_cost(node: &Node, cost: &mut ResourceCost) {
        for attr in &node.attributes {
            match attr.name.as_str() {
                "Supplies" | "supplies" => {
                    cost.supplies = attr.value_string().parse().unwrap_or(0.0)
                }
                "Power" | "power" => cost.power = attr.value_string().parse().unwrap_or(0.0),
                "Pop" | "pop" | "Population" => {
                    cost.population = attr.value_string().parse().unwrap_or(0)
                }
                _ => {}
            }
        }
        // Also check text content for simple cost values
        let text = node.text_string();
        if !text.is_empty()
            && let Ok(v) = text.parse::<f32>()
        {
            cost.supplies = v;
        }
    }

    fn parse_object_class(class: &str) -> ObjectType {
        match class.to_lowercase().as_str() {
            "unit" | "infantry" | "vehicle" | "aircraft" => ObjectType::Unit,
            "building" | "structure" => ObjectType::Building,
            "projectile" => ObjectType::Projectile,
            "effect" => ObjectType::Effect,
            "resource" | "supplycrate" => ObjectType::Resource,
            _ => ObjectType::Unit,
        }
    }

    fn parse_flag(node: &Node, flags: &mut ProtoFlags) {
        let flag_name = node.text_string();
        match flag_name.to_lowercase().as_str() {
            "canattack" | "attackable" => flags.can_attack = true,
            "mobile" | "canmove" => flags.can_move = true,
            "flying" | "flyer" => flags.flying = true,
            "infantry" => flags.infantry = true,
            "vehicle" => flags.vehicle = true,
            "aircraft" => flags.aircraft = true,
            "building" | "isbuilding" => flags.building = true,
            "hero" => flags.hero = true,
            _ => {}
        }
    }

    fn parse_squads(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: ProtoId = 1;

        for node in &root.children {
            if node.name == "Squad" {
                let squad = Self::parse_proto_squad(node, &mut id_counter);
                self.proto.add_squad(squad);
            }
        }

        Ok(())
    }

    fn parse_proto_squad(node: &Node, id_counter: &mut ProtoId) -> ProtoSquad {
        let mut squad = ProtoSquad {
            id: *id_counter,
            ..Default::default()
        };
        *id_counter += 1;

        if let Some(name_attr) = node.get_attribute("name") {
            squad.name = name_attr.value_string();
        }

        for child in &node.children {
            match child.name.as_str() {
                "Name" => squad.name = child.text_string(),
                "FormationType" => squad.formation = child.text_string(),
                "Unit" => {
                    let mut unit = SquadUnit::default();
                    if let Some(count) = child.get_attribute("count") {
                        unit.count = count.value_string().parse().unwrap_or(1);
                    }
                    // Unit proto name is in the text - store as 0 for now, resolve later
                    let _proto_name = child.text_string();
                    unit.proto_id = 0; // Will be resolved later
                    squad.units.push(unit);
                }
                _ => {}
            }
        }

        squad
    }

    fn parse_techs(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: ProtoId = 1;

        for node in &root.children {
            if node.name == "Tech" {
                let tech = Self::parse_proto_tech(node, &mut id_counter);
                self.proto.add_tech(tech);
            }
        }

        Ok(())
    }

    fn parse_proto_tech(node: &Node, id_counter: &mut ProtoId) -> ProtoTech {
        let mut tech = ProtoTech {
            id: *id_counter,
            ..Default::default()
        };
        *id_counter += 1;

        if let Some(name_attr) = node.get_attribute("name") {
            tech.name = name_attr.value_string();
        }

        for child in &node.children {
            match child.name.as_str() {
                "Name" => tech.name = child.text_string(),
                "DisplayNameID" => tech.display_name = child.text_string(),
                "ResearchPoints" => tech.research_time = child.text_string().parse().unwrap_or(0.0),
                "Cost" => Self::parse_cost(child, &mut tech.cost),
                "Effect" => {
                    let effect = Self::parse_tech_effect(child);
                    tech.effects.push(effect);
                }
                _ => {}
            }
        }

        tech
    }

    fn parse_tech_effect(node: &Node) -> TechEffect {
        let effect_type = node
            .get_attribute("type")
            .map(|a| a.value_string())
            .unwrap_or_default();
        let target = node
            .get_attribute("target")
            .map(|a| a.value_string())
            .unwrap_or_default();
        let amount: f32 = node
            .get_attribute("amount")
            .and_then(|a| a.value_string().parse().ok())
            .unwrap_or(0.0);

        match effect_type.to_lowercase().as_str() {
            "modifypercent" | "percent" => TechEffect::ModifyPercent {
                stat: target,
                amount,
            },
            "modifyabsolute" | "absolute" | "modify" => TechEffect::ModifyAbsolute {
                stat: target,
                amount,
            },
            "enableability" | "ability" => TechEffect::EnableAbility { ability: target },
            "unlock" => TechEffect::Unlock { proto_name: target },
            // Default to percent modifier
            _ => TechEffect::ModifyPercent {
                stat: target,
                amount,
            },
        }
    }

    fn parse_civs(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 1;

        for node in &root.children {
            if node.name == "Civ" {
                let mut civ = Civilization {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    civ.name = name_attr.value_string();
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => civ.name = child.text_string(),
                        "DisplayNameID" => civ.display_name = child.text_string(),
                        "CommandAckObject" => civ.command_ack_object = child.text_string(),
                        "RallyPointObject" => civ.rally_point_object = child.text_string(),
                        "LocalObject" => civ.local_object = child.text_string(),
                        "TransportProto" => civ.transport_proto = child.text_string(),
                        "Leader" => {
                            let text = child.text_string();
                            if !text.is_empty() {
                                civ.leaders.push(text);
                            }
                        }
                        _ => {}
                    }
                }

                let id = civ.id;
                let name = civ.name.clone();
                self.civs.insert(id, civ);
                self.civs_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    fn parse_leaders(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 1;

        for node in &root.children {
            if node.name == "Leader" {
                let mut leader = Leader {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    leader.name = name_attr.value_string();
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => leader.name = child.text_string(),
                        "DisplayNameID" => leader.display_name = child.text_string(),
                        "Civ" => leader.civ = child.text_string(),
                        "Icon" => leader.icon = child.text_string(),
                        "LeaderPower" | "Power" => {
                            let text = child.text_string();
                            if !text.is_empty() {
                                leader.leader_powers.push(text);
                            }
                        }
                        "StartingUnit" => leader.starting_unit = child.text_string(),
                        "StartingSquad" => leader.starting_squad = child.text_string(),
                        "Tech" => leader.tech = child.text_string(),
                        _ => {}
                    }
                }

                let id = leader.id;
                let name = leader.name.clone();
                self.leaders.insert(id, leader);
                self.leaders_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    fn parse_powers(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 1;

        for node in &root.children {
            if node.name == "Power" {
                let mut power = Power {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    power.name = name_attr.value_string();
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => power.name = child.text_string(),
                        "DisplayNameID" => power.display_name = child.text_string(),
                        "Icon" => power.icon = child.text_string(),
                        "TechPrereq" => power.tech_prereq = child.text_string(),
                        "PowerType" | "Type" => power.power_type = child.text_string(),
                        "AutoRecharge" => {
                            power.auto_recharge = child.text_string().parse().unwrap_or(0.0)
                        }
                        "UseLimit" => power.use_limit = child.text_string().parse().unwrap_or(-1),
                        _ => {}
                    }
                }

                let id = power.id;
                let name = power.name.clone();
                self.powers.insert(id, power);
                self.powers_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    fn parse_abilities(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 1;

        for node in &root.children {
            if node.name == "Ability" {
                let mut ability = Ability {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    ability.name = name_attr.value_string();
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => ability.name = child.text_string(),
                        "DisplayNameID" => ability.display_name = child.text_string(),
                        "Type" | "AbilityType" => ability.ability_type = child.text_string(),
                        "RecoverTime" => {
                            ability.recover_time = child.text_string().parse().unwrap_or(0.0)
                        }
                        "MovementModifier" => {
                            ability.movement_modifier = child.text_string().parse().unwrap_or(1.0)
                        }
                        _ => {}
                    }
                }

                let id = ability.id;
                let name = ability.name.clone();
                self.abilities.insert(id, ability);
                self.abilities_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    fn parse_weapon_types(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 1;

        for node in &root.children {
            if node.name == "WeaponType" || node.name == "Weapon" {
                let mut weapon = WeaponType {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    weapon.name = name_attr.value_string();
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => weapon.name = child.text_string(),
                        "DamagePerSecond" | "DPS" => {
                            weapon.damage_per_second = child.text_string().parse().unwrap_or(0.0)
                        }
                        "DPSVariance" => {
                            weapon.dps_variance = child.text_string().parse().unwrap_or(0.0)
                        }
                        "MaxRange" => weapon.max_range = child.text_string().parse().unwrap_or(0.0),
                        "Accuracy" => weapon.accuracy = child.text_string().parse().unwrap_or(1.0),
                        "MovingAccuracy" => {
                            weapon.moving_accuracy = child.text_string().parse().unwrap_or(1.0)
                        }
                        "Visual" => weapon.visual = child.text_string(),
                        _ => {}
                    }
                }

                let id = weapon.id;
                let name = weapon.name.clone();
                self.weapon_types.insert(id, weapon);
                self.weapon_types_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    fn parse_game_modes(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 1;

        for node in &root.children {
            if node.name == "GameMode" || node.name == "Mode" {
                let mut mode = GameMode {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    mode.name = name_attr.value_string();
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => mode.name = child.text_string(),
                        "DisplayNameID" => mode.display_name = child.text_string(),
                        "GameType" | "Type" => mode.game_type = child.text_string(),
                        "MaxPlayers" => mode.max_players = child.text_string().parse().unwrap_or(2),
                        "StartingResources" => {
                            Self::parse_cost(child, &mut mode.starting_resources)
                        }
                        _ => {}
                    }
                }

                let id = mode.id;
                let name = mode.name.clone();
                self.game_modes.insert(id, mode);
                self.game_modes_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    fn parse_damage_types(&mut self, xmb: &XmbData, _filename: &str) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 1;

        for node in &root.children {
            if node.name == "DamageType" {
                let mut dtype = DamageType {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    dtype.name = name_attr.value_string();
                }

                for child in &node.children {
                    let text = child.text_string();
                    match child.name.as_str() {
                        "Name" => dtype.name = text,
                        "Shielded" => dtype.shielded = text == "true" || text == "1",
                        "Attenuates" => dtype.attenuates = text == "true" || text == "1",
                        _ => {}
                    }
                }

                let id = dtype.id;
                let name = dtype.name.clone();
                self.damage_types.insert(id, dtype);
                self.damage_types_by_name.insert(name, id);
            }
        }

        Ok(())
    }
}
