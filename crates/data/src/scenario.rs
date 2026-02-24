//! Scenario data loading for Halo Wars maps.
//!
//! Parses `.scn` XMB files (or XML for testing) into scenario data structures.
//! Based on BScenario from the original source.
//!
//! XMB files are ECF containers with compressed binary XML data inside.
//! The `xmb` crate (from ensemble-rs) handles parsing these files.
//!
//! # Example
//!
//! ```ignore
//! use data::Scenario;
//!
//! let scenario = Scenario::load("blood_gulch")?;
//! println!("Players: {}", scenario.players.len());
//! println!("Objects: {}", scenario.objects.len());
//! ```

use crate::assets::{AssetError, AssetSource};
use crate::xmb::{Node, XmbData, XmbReader};
use glam::Vec3;
use std::io::{Cursor, Read, Seek};
use thiserror::Error;

/// Scenario loading errors.
#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("Asset loading error: {0}")]
    Asset(#[from] AssetError),
    #[error("XMB parsing error: {0}")]
    Xmb(#[from] crate::xmb::Error),
    #[error("Invalid attribute: {0}")]
    InvalidAttribute(String),
    #[error("Missing required attribute: {0}")]
    MissingAttribute(String),
    #[error("Invalid position format: {0}")]
    InvalidPosition(String),
    #[error("Scenario file not found: {0}")]
    NotFound(String),
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
    pub player_id: u8,
}

/// Player definition from scenario.
#[derive(Debug, Clone, Default)]
pub struct ScenarioPlayer {
    /// Player name.
    pub name: String,
    /// Civilization ID (1 = UNSC, 2 = Covenant).
    pub civ_id: i32,
    /// Leader ID.
    pub leader_id: i32,
    /// Team ID.
    pub team_id: u8,
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
    pub player_id: u8,
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
///
/// Contains all data parsed from a `.scn.xmb` file.
/// Use this with simulation code to populate a game world.
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
}

impl Scenario {
    /// Load a scenario by name from ERA archives.
    ///
    /// Uses `AssetSource` to find and load the `.scn.xmb` file for the given scenario.
    ///
    /// # Example
    /// ```ignore
    /// let scenario = Scenario::load("blood_gulch")?;
    /// println!("Players: {}", scenario.players.len());
    /// ```
    pub fn load(scenario_name: &str) -> Result<Self, ScenarioError> {
        log::info!("Loading scenario: {}", scenario_name);

        let mut source = AssetSource::for_scenario(scenario_name)?;

        // Find the .scn.xmb file
        let scn_files = source.list(|p| p.ends_with(".scn.xmb"));
        let scn_path = scn_files
            .first()
            .ok_or_else(|| ScenarioError::NotFound(format!("{}.scn.xmb", scenario_name)))?;

        let scn_data = source.read(scn_path)?;
        let mut scenario = ScenarioLoader::load_xmb_bytes(&scn_data)?;
        scenario.name = scenario_name.to_string();

        log::info!(
            "Loaded scenario '{}': {} players, {} positions, {} objects",
            scenario_name,
            scenario.players.len(),
            scenario.positions.len(),
            scenario.objects.len()
        );

        Ok(scenario)
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
    fn civ_name_to_id(name: &str) -> i32 {
        match name.to_lowercase().as_str() {
            "unsc" => 1,
            "covenant" => 2,
            _ => 1, // Default to UNSC
        }
    }

    /// Convert leader name to ID.
    /// TODO: Load from database
    fn leader_name_to_id(name: &str) -> i32 {
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
