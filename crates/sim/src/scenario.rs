//! Scenario loading for Halo Wars maps.
//!
//! Parses `.scn` XMB files (or XML for testing) and populates World state with players and entities.
//! Based on BScenario from the original source.
//!
//! XMB files are ECF containers with compressed binary XML data inside.
//! The `xmb` crate (from ensemble-rs) handles parsing these files.

use crate::entity_id::EntityId;
use crate::player::{CivId, LeaderId, PlayerId, PlayerType, TeamId};
use crate::world::World;
use data::xmb::{Node, XmbData, XmbReader};
use glam::Vec3;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};
use thiserror::Error;

/// Scenario loading errors.
#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("XMB parsing error: {0}")]
    Xmb(#[from] data::xmb::Error),
    #[error("Invalid attribute: {0}")]
    InvalidAttribute(String),
    #[error("Missing required attribute: {0}")]
    MissingAttribute(String),
    #[error("Invalid position format: {0}")]
    InvalidPosition(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Player spawn position from scenario.
#[derive(Debug, Clone, Default)]
pub struct ScenarioPosition {
    /// Position number (1-based).
    pub number: i32,
    /// World position.
    pub position: Vec3,
    /// Forward direction.
    pub forward: Vec3,
    /// Player ID assigned to this position.
    pub player_id: PlayerId,
}

/// Player definition from scenario.
#[derive(Debug, Clone, Default)]
pub struct ScenarioPlayer {
    /// Player name.
    pub name: String,
    /// Civilization ID.
    pub civ_id: CivId,
    /// Leader ID.
    pub leader_id: LeaderId,
    /// Team ID.
    pub team_id: TeamId,
    /// Player color index.
    pub color: i32,
    /// Whether to use starting units.
    pub use_starting_units: bool,
    /// Whether player is controllable.
    pub controllable: bool,
    /// Whether to use default resources.
    pub default_resources: bool,
    /// Starting resources (supplies, power, etc).
    pub resources: [f32; 4],
    /// Position index.
    pub position_index: i32,
    /// Spawn position.
    pub position: Vec3,
    /// Forward direction.
    pub forward: Vec3,
}

/// Object/entity definition from scenario.
#[derive(Debug, Clone, Default)]
pub struct ScenarioObject {
    /// Proto name (e.g., "unsc_inf_marine_01").
    pub proto_name: String,
    /// Whether this is a squad (vs object).
    pub is_squad: bool,
    /// Player ID (0 = Gaia).
    pub player_id: PlayerId,
    /// Scenario ID (for trigger references).
    pub scenario_id: i32,
    /// World position.
    pub position: Vec3,
    /// Forward direction.
    pub forward: Vec3,
    /// Right direction.
    pub right: Vec3,
}

/// Loaded scenario data.
#[derive(Debug, Clone, Default)]
pub struct Scenario {
    /// Scenario name.
    pub name: String,
    /// Player positions.
    pub positions: Vec<ScenarioPosition>,
    /// Player definitions.
    pub players: Vec<ScenarioPlayer>,
    /// Objects/entities.
    pub objects: Vec<ScenarioObject>,
    /// Mapping from scenario ID to entity ID (populated after loading into world).
    pub scenario_id_to_entity_id: HashMap<i32, EntityId>,
}

/// Scenario loader.
pub struct ScenarioLoader;

impl ScenarioLoader {
    /// Load a scenario from an XMB file reader.
    pub fn load_xmb<R: Read + Seek>(reader: R) -> Result<Scenario, ScenarioError> {
        let xmb_data = XmbReader::read(reader)?;
        Self::parse_xmb_data(&xmb_data)
    }

    /// Load a scenario from XMB bytes.
    pub fn load_xmb_bytes(bytes: &[u8]) -> Result<Scenario, ScenarioError> {
        let cursor = Cursor::new(bytes);
        Self::load_xmb(cursor)
    }

    /// Load a scenario from XML string (for testing/debugging).
    pub fn load_from_xml_str(xml: &str) -> Result<Scenario, ScenarioError> {
        let xmb_data = XmbData::from_xml(xml)?;
        Self::parse_xmb_data(&xmb_data)
    }

    /// Parse XMB data into a Scenario.
    fn parse_xmb_data(xmb: &XmbData) -> Result<Scenario, ScenarioError> {
        let mut scenario = Scenario::default();
        if let Some(root) = xmb.root() {
            Self::parse_node(root, &mut scenario)?;
        }
        Ok(scenario)
    }

    /// Recursively parse nodes from the XMB tree.
    fn parse_node(node: &Node, scenario: &mut Scenario) -> Result<(), ScenarioError> {
        match node.name.as_str() {
            "Scenario" => {
                for child in &node.children {
                    Self::parse_node(child, scenario)?;
                }
            }
            "Positions" => {
                for child in &node.children {
                    if child.name == "Position" {
                        let pos = Self::parse_position_node(child)?;
                        scenario.positions.push(pos);
                    }
                }
            }
            "Players" => {
                for child in &node.children {
                    if child.name == "Player" {
                        let player = Self::parse_player_node(child)?;
                        scenario.players.push(player);
                    }
                }
            }
            "Objects" => {
                for child in &node.children {
                    if child.name == "Object" {
                        let obj = Self::parse_object_node(child)?;
                        scenario.objects.push(obj);
                    }
                }
            }
            _ => {
                // Recurse into other nodes (e.g., root node)
                for child in &node.children {
                    Self::parse_node(child, scenario)?;
                }
            }
        }
        Ok(())
    }

    fn parse_position_node(node: &Node) -> Result<ScenarioPosition, ScenarioError> {
        let mut pos = ScenarioPosition {
            forward: Vec3::Z,
            ..Default::default()
        };

        for attr in &node.attributes {
            let value = attr.value_string();
            match attr.name.as_str() {
                "Number" => pos.number = value.parse().unwrap_or(-1),
                "Position" => pos.position = Self::parse_vec3(&value)?,
                "Forward" => pos.forward = Self::parse_vec3(&value)?,
                "Player" => pos.player_id = value.parse().unwrap_or(0),
                _ => {}
            }
        }

        Ok(pos)
    }

    fn parse_player_node(node: &Node) -> Result<ScenarioPlayer, ScenarioError> {
        let mut player = ScenarioPlayer {
            civ_id: 1,
            leader_id: -1,
            controllable: true,
            default_resources: true,
            forward: Vec3::Z,
            position: Vec3::ZERO,
            position_index: -1,
            ..Default::default()
        };

        for attr in &node.attributes {
            let value = attr.value_string();
            match attr.name.as_str() {
                "Name" => player.name = value,
                "Civ" => player.civ_id = Self::civ_name_to_id(&value),
                "Leader" | "Leader1" => player.leader_id = Self::leader_name_to_id(&value),
                "Team" => player.team_id = value.parse().unwrap_or(0),
                "Color" => player.color = value.parse().unwrap_or(-1),
                "UseStartingUnits" => player.use_starting_units = value == "true",
                "Controllable" => player.controllable = value == "true",
                "DefaultResources" => player.default_resources = value == "true",
                "Position" => player.position = Self::parse_vec3(&value).unwrap_or(Vec3::ZERO),
                "Forward" => player.forward = Self::parse_vec3(&value).unwrap_or(Vec3::Z),
                _ => {}
            }
        }

        Ok(player)
    }

    fn parse_object_node(node: &Node) -> Result<ScenarioObject, ScenarioError> {
        let mut obj = ScenarioObject {
            forward: Vec3::Z,
            right: Vec3::X,
            scenario_id: -1,
            ..Default::default()
        };

        for attr in &node.attributes {
            let value = attr.value_string();
            match attr.name.as_str() {
                "IsSquad" => obj.is_squad = value == "true",
                "Player" => obj.player_id = value.parse().unwrap_or(0),
                "ID" => obj.scenario_id = value.parse().unwrap_or(-1),
                "Position" => obj.position = Self::parse_vec3(&value)?,
                "Forward" => obj.forward = Self::parse_vec3(&value).unwrap_or(Vec3::Z),
                "Right" => obj.right = Self::parse_vec3(&value).unwrap_or(Vec3::X),
                _ => {}
            }
        }

        // Get proto name from text content
        obj.proto_name = node.text_string().trim().to_string();

        // If no IsSquad attribute, try to detect from proto name
        if obj.proto_name.contains("_sqd_") || obj.proto_name.ends_with("_squad") {
            obj.is_squad = true;
        }

        Ok(obj)
    }

    /// Parse a "x,y,z" string into Vec3.
    pub fn parse_vec3(s: &str) -> Result<Vec3, ScenarioError> {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 3 {
            return Err(ScenarioError::InvalidPosition(s.to_string()));
        }

        let x = parts[0]
            .trim()
            .parse::<f32>()
            .map_err(|_| ScenarioError::InvalidPosition(s.to_string()))?;
        let y = parts[1]
            .trim()
            .parse::<f32>()
            .map_err(|_| ScenarioError::InvalidPosition(s.to_string()))?;
        let z = parts[2]
            .trim()
            .parse::<f32>()
            .map_err(|_| ScenarioError::InvalidPosition(s.to_string()))?;

        Ok(Vec3::new(x, y, z))
    }

    /// Convert civilization name to ID.
    fn civ_name_to_id(name: &str) -> CivId {
        match name.to_lowercase().as_str() {
            "unsc" => 1,
            "covenant" => 2,
            _ => 1, // Default to UNSC
        }
    }

    /// Convert leader name to ID.
    /// TODO: Load from database
    fn leader_name_to_id(name: &str) -> LeaderId {
        match name.to_lowercase().as_str() {
            "cutter" => 1,
            "anders" => 2,
            "forge" => 3,
            "arbiter" => 4,
            "prophet" => 5,
            "brute" => 6,
            _ => -1,
        }
    }
}

impl Scenario {
    /// Load this scenario into a World, creating players and entities.
    ///
    /// Returns the populated World and updates scenario_id_to_entity_id mapping.
    pub fn load_into_world(&mut self) -> World {
        let mut world = World::new();

        // Initialize players
        // Player 0 is always Gaia (created by init_players)
        let player_count = self.players.len() as u8;
        world.init_players(player_count);

        // Configure players from scenario data
        for (i, scenario_player) in self.players.iter().enumerate() {
            let player_id = (i + 1) as PlayerId; // +1 because Gaia is 0
            if let Some(player) = world.get_player_mut(player_id) {
                player.name = scenario_player.name.clone();
                player.civ_id = scenario_player.civ_id;
                player.leader_id = scenario_player.leader_id;
                player.team_id = scenario_player.team_id;
                player.player_type = if scenario_player.controllable {
                    PlayerType::Human
                } else {
                    PlayerType::ComputerAi
                };

                // Set starting resources if not using defaults
                if !scenario_player.default_resources {
                    for (j, &amount) in scenario_player.resources.iter().enumerate() {
                        player.resources.set(j, amount);
                    }
                }
            }
        }

        // Create entities from scenario objects
        for obj in &self.objects {
            if obj.is_squad {
                // Create squad
                let entity_id = world.create_squad_at(obj.player_id, obj.position);

                // Set proto ID and forward direction
                if let Some(squad) = world.get_squad_mut(entity_id) {
                    squad.base.set_forward(obj.forward);
                    // TODO: Look up proto_squad_id from database by name
                    // squad.proto_squad_id = database.get_proto_squad(&obj.proto_name);
                }

                // Map scenario ID to entity ID
                if obj.scenario_id >= 0 {
                    self.scenario_id_to_entity_id.insert(obj.scenario_id, entity_id);
                }
            }
            // TODO: Handle non-squad objects (buildings, props, etc.)
        }

        world
    }

    /// Get entity ID from scenario object ID.
    pub fn get_entity_id(&self, scenario_id: i32) -> Option<EntityId> {
        self.scenario_id_to_entity_id.get(&scenario_id).copied()
    }

    /// Get the number of squads in the scenario.
    pub fn squad_count(&self) -> usize {
        self.objects.iter().filter(|o| o.is_squad).count()
    }

    /// Get the number of objects (non-squads) in the scenario.
    pub fn object_count(&self) -> usize {
        self.objects.iter().filter(|o| !o.is_squad).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SCENARIO: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Scenario>
    <Positions>
        <Position Number="1" Position="100.0,0.0,100.0" Forward="0.0,0.0,1.0" />
        <Position Number="2" Position="200.0,0.0,200.0" Forward="0.0,0.0,-1.0" />
    </Positions>
    <Players>
        <Player Name="Player1" Civ="UNSC" Leader="Cutter" Team="1" Color="0" />
        <Player Name="Player2" Civ="Covenant" Leader="Arbiter" Team="2" Color="1" />
    </Players>
    <Objects>
        <Object IsSquad="true" Player="1" ID="0" Position="100.0,0.0,100.0" Forward="0.0,0.0,1.0">
            unsc_inf_marine_01
        </Object>
        <Object IsSquad="true" Player="1" ID="1" Position="110.0,0.0,100.0">
            unsc_inf_marine_01
        </Object>
        <Object IsSquad="true" Player="2" ID="2" Position="200.0,0.0,200.0">
            cov_inf_grunt_01
        </Object>
    </Objects>
</Scenario>"#;

    #[test]
    fn test_parse_scenario() {
        let scenario = ScenarioLoader::load_from_xml_str(SAMPLE_SCENARIO).unwrap();

        // Check positions
        assert_eq!(scenario.positions.len(), 2);
        assert_eq!(scenario.positions[0].number, 1);
        assert!((scenario.positions[0].position.x - 100.0).abs() < 0.01);

        // Check players
        assert_eq!(scenario.players.len(), 2);
        assert_eq!(scenario.players[0].name, "Player1");
        assert_eq!(scenario.players[0].civ_id, 1); // UNSC
        assert_eq!(scenario.players[1].name, "Player2");
        assert_eq!(scenario.players[1].civ_id, 2); // Covenant

        // Check objects
        assert_eq!(scenario.objects.len(), 3);
        assert!(scenario.objects[0].is_squad);
        assert_eq!(scenario.objects[0].player_id, 1);
        assert_eq!(scenario.objects[0].proto_name, "unsc_inf_marine_01");
    }

    #[test]
    fn test_load_into_world() {
        let mut scenario = ScenarioLoader::load_from_xml_str(SAMPLE_SCENARIO).unwrap();
        let world = scenario.load_into_world();

        // Check players (Gaia + 2 players)
        assert_eq!(world.player_count(), 3);

        // Check player 1
        let p1 = world.get_player(1).unwrap();
        assert_eq!(p1.name, "Player1");
        assert_eq!(p1.civ_id, 1);
        assert_eq!(p1.team_id, 1);

        // Check player 2
        let p2 = world.get_player(2).unwrap();
        assert_eq!(p2.name, "Player2");
        assert_eq!(p2.civ_id, 2);
        assert_eq!(p2.team_id, 2);

        // Check squads were created
        assert_eq!(scenario.squad_count(), 3);

        // Check scenario ID mapping
        let entity_id = scenario.get_entity_id(0).unwrap();
        let squad = world.get_squad(entity_id).unwrap();
        assert!((squad.position().x - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_vec3() {
        let v = ScenarioLoader::parse_vec3("1.5,2.5,3.5").unwrap();
        assert!((v.x - 1.5).abs() < 0.01);
        assert!((v.y - 2.5).abs() < 0.01);
        assert!((v.z - 3.5).abs() < 0.01);
    }

    #[test]
    fn test_empty_scenario() {
        let xml = r#"<?xml version="1.0"?><Scenario></Scenario>"#;
        let scenario = ScenarioLoader::load_from_xml_str(xml).unwrap();
        assert!(scenario.players.is_empty());
        assert!(scenario.objects.is_empty());
    }
}

