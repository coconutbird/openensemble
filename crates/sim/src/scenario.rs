//! Scenario world loading for Halo Wars maps.
//!
//! This module provides functionality to load scenario data into the simulation world.
//! The scenario data itself is parsed by the `data` crate; this module handles
//! the simulation-specific logic of creating players and entities.
//!
//! # Example
//!
//! ```ignore
//! use data::{Scenario, ScenarioLoader};
//! use sim::scenario::ScenarioWorld;
//!
//! // Parse scenario from data crate
//! let scenario = ScenarioLoader::load_xmb_bytes(&scenario_data)?;
//!
//! // Load into simulation world
//! let (world, entity_map) = ScenarioWorld::load_into_world(&scenario);
//! ```

use crate::entity_id::EntityId;
use crate::player::PlayerType;
use crate::world::World;
use std::collections::HashMap;

// Re-export scenario types from data crate for convenience
pub use data::{
    Scenario, ScenarioError, ScenarioLoader, ScenarioObject, ScenarioPlayer, ScenarioPosition,
};

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
/// use data::ScenarioLoader;
/// use sim::scenario::load_scenario_into_world;
///
/// let scenario = ScenarioLoader::load_xmb_bytes(&data)?;
/// let loaded = load_scenario_into_world(&scenario);
/// println!("Created {} players", loaded.world.player_count());
/// ```
pub fn load_scenario_into_world(scenario: &Scenario) -> LoadedScenario {
    let mut world = World::new();
    let mut scenario_id_to_entity_id = HashMap::new();

    // Initialize players
    // Player 0 is always Gaia (created by init_players)
    let player_count = scenario.players.len() as u8;
    world.init_players(player_count);

    // Configure players from scenario data
    for (i, scenario_player) in scenario.players.iter().enumerate() {
        let player_id = (i + 1) as u8; // +1 because Gaia is 0
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
    for obj in &scenario.objects {
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
                scenario_id_to_entity_id.insert(obj.scenario_id, entity_id);
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
    fn test_load_into_world() {
        let scenario = ScenarioLoader::load_from_xml_str(SAMPLE_SCENARIO).unwrap();
        let loaded = load_scenario_into_world(&scenario);

        // Check players (Gaia + 2 players)
        assert_eq!(loaded.world.player_count(), 3);

        // Check player 1
        let p1 = loaded.world.get_player(1).unwrap();
        assert_eq!(p1.name, "Player1");
        assert_eq!(p1.civ_id, 1);
        assert_eq!(p1.team_id, 1);

        // Check player 2
        let p2 = loaded.world.get_player(2).unwrap();
        assert_eq!(p2.name, "Player2");
        assert_eq!(p2.civ_id, 2);
        assert_eq!(p2.team_id, 2);

        // Check squads were created
        assert_eq!(scenario.squad_count(), 3);

        // Check scenario ID mapping
        let entity_id = loaded.get_entity_id(0).unwrap();
        let squad = loaded.world.get_squad(entity_id).unwrap();
        assert!((squad.position().x - 100.0).abs() < 0.01);
    }
}
