//! Scenario world loading for Halo Wars maps.
//!
//! This module provides functionality to load scenario data into the simulation world.
//! The scenario data itself is parsed by the `pipeline` crate; this module handles
//! the simulation-specific logic of creating players and entities.
//!
//! # Example
//!
//! ```ignore
//! use pipeline::hw1::scenario::ScenarioData;
//! use pipeline::database::hw1::Database;
//! use sim::load_scenario_into_world;
//!
//! // Load scenario data from parsed SCN file
//! let scenario = ScenarioData::default();
//! let db = Database::new();
//! let loaded = load_scenario_into_world(&scenario, &db);
//! println!("Created {} players", loaded.world.player_count());
//! ```

use crate::entity_id::EntityId;
use crate::player::PlayerType;
use crate::world::World;
use pipeline::database::hw1::Database;
use std::collections::HashMap;

// Re-export scenario types from pipeline for convenience
pub use pipeline::hw1::scenario::{ScenarioData, ScenarioObject, ScenarioPlayer, ScenarioPosition};

/// Result of loading a scenario into a world.
///
/// Contains the world and the mapping from scenario IDs to entity IDs.
pub struct LoadedScenario {
    /// The populated simulation world.
    pub world: World,
    /// Mapping from scenario object IDs to simulation entity IDs.
    pub scenario_id_to_entity_id: HashMap<i32, EntityId>,
}

impl LoadedScenario {
    /// Get entity ID from scenario object ID.
    pub fn get_entity_id(&self, scenario_id: i32) -> Option<EntityId> {
        self.scenario_id_to_entity_id.get(&scenario_id).copied()
    }
}

/// Load a scenario into a new World, creating players and entities.
///
/// Returns a [`LoadedScenario`] containing the populated world and
/// a mapping from scenario object IDs to entity IDs.
///
/// # Example
///
/// ```ignore
/// use pipeline::hw1::scenario::ScenarioData;
/// use pipeline::database::hw1::Database;
/// use sim::load_scenario_into_world;
///
/// let scenario = ScenarioData::default();
/// let db = Database::new();
/// let loaded = load_scenario_into_world(&scenario, &db);
/// println!("Created {} players", loaded.world.player_count());
/// ```
pub fn load_scenario_into_world(scenario: &ScenarioData, db: &Database) -> LoadedScenario {
    let mut world = World::new();
    let mut scenario_id_to_entity_id = HashMap::new();

    let players = scenario
        .players
        .as_ref()
        .map_or(&[][..], |w| &w.entries);
    let objects = scenario
        .objects
        .as_ref()
        .map_or(&[][..], |w| &w.entries);

    // Initialize players
    // Player 0 is always Gaia (created by init_players)
    let player_count = players.len() as u8;
    world.init_players(player_count);

    // Configure players from scenario data
    for (i, scenario_player) in players.iter().enumerate() {
        let player_id = (i + 1) as u8; // +1 because Gaia is 0
        if let Some(player) = world.get_player_mut(player_id) {
            player.name = scenario_player.name.clone();

            // Resolve civ/leader names to IDs via database (0-based, parse order)
            player.civ_id = db
                .civs
                .iter()
                .position(|c| c.name == scenario_player.civ)
                .map(|id| id as i32)
                .unwrap_or(-1);
            player.leader_id = db
                .leaders
                .iter()
                .position(|l| l.name == scenario_player.leader1)
                .map(|id| id as i32)
                .unwrap_or(-1);

            player.team_id = scenario_player.team as u8;
            player.player_type = if scenario_player.controllable {
                PlayerType::Human
            } else {
                PlayerType::ComputerAi
            };

            // Set starting resources from scenario player fields
            if scenario_player.supplies != 0.0 || scenario_player.power != 0.0 {
                player.resources.set(0, scenario_player.supplies);
                player.resources.set(1, scenario_player.power);
            }
        }
    }

    // Create entities from scenario objects
    for obj in objects {
        if obj.is_squad {
            // Parse position from string "x,y,z" → glam::Vec3
            let pos = obj.position_vec3();
            let position = glam::Vec3::new(pos[0], pos[1], pos[2]);

            // Create squad
            let entity_id = world.create_squad_at(obj.player as u8, position);

            // Set forward direction
            if let Some(squad) = world.get_squad_mut(entity_id) {
                let fwd = obj.forward_vec3();
                squad.base.set_forward(glam::Vec3::new(fwd[0], fwd[1], fwd[2]));
                // TODO: Look up proto_squad_id from database by name
                // squad.proto_squad_id = database.get_proto_squad(&obj.proto_name);
            }

            // Map scenario ID to entity ID
            if obj.id >= 0 {
                scenario_id_to_entity_id.insert(obj.id, entity_id);
            }
        }
        // TODO: Handle non-squad objects (buildings, props, etc.)
    }

    LoadedScenario {
        world,
        scenario_id_to_entity_id,
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
        <Player Name="Player1" Civ="UNSC" Leader1="Cutter" Team="1" Color="0" />
        <Player Name="Player2" Civ="Covenant" Leader1="Arbiter" Team="2" Color="1" />
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
    fn test_load_into_world() {
        let scenario = ScenarioData::from_xml_str(SAMPLE_SCENARIO).unwrap();
        let db = Database::new(); // empty db — names won't resolve
        let loaded = load_scenario_into_world(&scenario, &db);

        // Check players (Gaia + 2 players)
        assert_eq!(loaded.world.player_count(), 3);

        // Check player 1
        let p1 = loaded.world.get_player(1).unwrap();
        assert_eq!(p1.name, "Player1");
        assert_eq!(p1.team_id, 1);
        // civ_id/leader_id are -1 since db is empty (no civs/leaders loaded)
        assert_eq!(p1.civ_id, -1);

        // Check player 2
        let p2 = loaded.world.get_player(2).unwrap();
        assert_eq!(p2.name, "Player2");
        assert_eq!(p2.team_id, 2);

        // Check squads were created (count objects with is_squad=true)
        let squad_count = scenario
            .objects
            .as_ref()
            .map_or(0, |o| o.entries.iter().filter(|e| e.is_squad).count());
        assert_eq!(squad_count, 3);

        // Check scenario ID mapping
        let entity_id = loaded.get_entity_id(0).unwrap();
        let squad = loaded.world.get_squad(entity_id).unwrap();
        assert!((squad.position().x - 100.0).abs() < 0.01);
    }
}
