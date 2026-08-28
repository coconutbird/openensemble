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
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
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
    #[must_use]
    pub fn get_entity_id(&self, scenario_id: i32) -> Option<EntityId> {
        self.scenario_id_to_entity_id.get(&scenario_id).copied()
    }
}

/// Load a scenario into a new [`World`], creating players and entities.
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
#[must_use]
pub fn load_scenario_into_world(scenario: &ScenarioData, db: &Database) -> LoadedScenario {
    let mut world = World::new();
    let mut scenario_id_to_entity_id = HashMap::new();
    let players = scenario.players.as_ref().map_or(&[][..], |w| &w.entries);
    let objects = scenario.objects.as_ref().map_or(&[][..], |w| &w.entries);
    let player_count =
        u8::try_from(players.len().min(crate::world::MAX_PLAYERS)).unwrap_or(u8::MAX);
    world.init_players(player_count);
    configure_players(&mut world, players, db);

    for obj in objects {
        let Some(entity_id) = create_scenario_object(&mut world, obj, db) else {
            continue;
        };
        if obj.id >= 0 {
            scenario_id_to_entity_id.insert(obj.id, entity_id);
        }
    }

    LoadedScenario {
        world,
        scenario_id_to_entity_id,
    }
}

fn configure_players(world: &mut World, players: &[ScenarioPlayer], db: &Database) {
    for (index, scenario_player) in players.iter().take(crate::world::MAX_PLAYERS).enumerate() {
        let player_id = u8::try_from(index + 1).unwrap_or(u8::MAX);
        let Some(player) = world.get_player_mut(player_id) else {
            continue;
        };
        player.name.clone_from(&scenario_player.name);
        player.civ_id = find_name_index(&db.civs, &scenario_player.civ, |civ| &civ.name);
        player.leader_id =
            find_name_index(&db.leaders, &scenario_player.leader1, |leader| &leader.name);
        player.team_id = u8::try_from(scenario_player.team).unwrap_or_default();
        player.player_type = if scenario_player.controllable {
            PlayerType::Human
        } else {
            PlayerType::ComputerAi
        };
        if scenario_player.supplies != 0.0 || scenario_player.power != 0.0 {
            player.resources.set(0, scenario_player.supplies);
            player.resources.set(1, scenario_player.power);
        }
    }
}

fn find_name_index<T>(values: &[T], name: &str, key: impl Fn(&T) -> &str) -> i32 {
    values
        .iter()
        .position(|value| key(value).eq_ignore_ascii_case(name))
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1)
}

fn create_scenario_object(
    world: &mut World,
    object: &ScenarioObject,
    db: &Database,
) -> Option<EntityId> {
    if object.is_squad {
        return Some(create_scenario_squad(world, object, db));
    }
    create_scenario_unit(world, object, db)
}

fn create_scenario_squad(world: &mut World, object: &ScenarioObject, db: &Database) -> EntityId {
    let player_id = u8::try_from(object.player).unwrap_or_default();
    let position = scenario_position(object);
    let squad_id = world.create_squad_at(player_id, position);
    let proto_name = object.proto_name.trim();
    let proto = find_proto_squad(db, proto_name);
    let forward = scenario_forward(object);
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad.base.set_forward(forward);
        squad.proto_squad_id = proto.map_or(-1, |(index, squad)| database_id(squad.dbid, index));
    }
    if let Some((_, proto)) = proto {
        create_squad_members(world, squad_id, player_id, position, proto, db);
    }
    squad_id
}

fn create_squad_members(
    world: &mut World,
    squad_id: EntityId,
    player_id: u8,
    position: Vec3,
    proto: &ProtoSquad,
    db: &Database,
) {
    let Some(units) = &proto.units else {
        return;
    };
    for entry in &units.entries {
        for _ in 0..entry.count.max(0) {
            let unit_id = world.create_unit_at(player_id, position);
            configure_unit(world, unit_id, entry.proto_object.trim(), db);
            let attached = world.attach_unit_to_squad(unit_id, squad_id);
            debug_assert!(attached, "new squad member should attach");
        }
    }
}

fn create_scenario_unit(
    world: &mut World,
    object: &ScenarioObject,
    db: &Database,
) -> Option<EntityId> {
    let proto_name = object.proto_name.trim();
    let (proto_index, proto) = find_proto_object(db, proto_name)?;
    let player_id = u8::try_from(object.player).unwrap_or_default();
    let position = scenario_position(object);
    let kind = classify_proto_object(proto)?;
    let unit_id = match kind {
        PlacedUnitKind::Mobile => world.create_unit_at(player_id, position),
        PlacedUnitKind::Building => world.create_building_at(player_id, position),
    };
    configure_unit_from_proto(world, unit_id, proto_name, proto_index, proto);
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.base.set_forward(scenario_forward(object));
    }
    if kind == PlacedUnitKind::Building && creates_base(proto) {
        let _base_id = world.register_base(unit_id);
    }
    Some(unit_id)
}

fn configure_unit(world: &mut World, unit_id: EntityId, proto_name: &str, db: &Database) {
    let Some((proto_index, proto)) = find_proto_object(db, proto_name) else {
        if let Some(unit) = world.get_unit_mut(unit_id) {
            proto_name.clone_into(&mut unit.proto_object_name);
        }
        return;
    };
    configure_unit_from_proto(world, unit_id, proto_name, proto_index, proto);
}

fn configure_unit_from_proto(
    world: &mut World,
    unit_id: EntityId,
    proto_name: &str,
    proto_index: usize,
    proto: &ProtoObject,
) {
    let Some(unit) = world.get_unit_mut(unit_id) else {
        return;
    };
    unit.proto_object_id = database_id(proto.dbid, proto_index);
    proto_name.clone_into(&mut unit.proto_object_name);
    if let Some(hitpoints) = proto.hitpoints {
        unit.set_max_hitpoints(hitpoints);
    }
    if !unit.is_building()
        && let Some(speed) = proto.max_velocity.or(proto.velocity)
        && speed.is_finite()
        && speed >= 0.0
    {
        unit.speed = speed;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlacedUnitKind {
    Mobile,
    Building,
}

fn classify_proto_object(proto: &ProtoObject) -> Option<PlacedUnitKind> {
    if proto
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Building"))
        || proto
            .select_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("Building"))
        || creates_base(proto)
    {
        return Some(PlacedUnitKind::Building);
    }
    proto
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Unit"))
        .then_some(PlacedUnitKind::Mobile)
}

fn creates_base(proto: &ProtoObject) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("KBCreatesBase"))
}

fn find_proto_object<'a>(db: &'a Database, name: &str) -> Option<(usize, &'a ProtoObject)> {
    db.objects
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name))
}

fn find_proto_squad<'a>(db: &'a Database, name: &str) -> Option<(usize, &'a ProtoSquad)> {
    db.squads
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name))
}

fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn scenario_position(object: &ScenarioObject) -> Vec3 {
    let [x, y, z] = object.position_vec3();
    Vec3::new(x, y, z)
}

fn scenario_forward(object: &ScenarioObject) -> Vec3 {
    let [x, y, z] = object.forward_vec3();
    Vec3::new(x, y, z)
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
        assert_eq!(loaded.world.squads.len(), squad_count);

        // Check scenario ID mapping
        let entity_id = loaded.get_entity_id(0).unwrap();
        let squad = loaded.world.get_squad(entity_id).unwrap();
        assert!((squad.position().x - 100.0).abs() < 0.01);
    }

    #[test]
    fn loads_squad_members_buildings_and_base_anchors() {
        use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

        let scenario = ScenarioData::from_xml_str(
            r#"<Scenario>
                <Players><Player Name="P1" Team="1" /></Players>
                <Objects>
                    <Object IsSquad="true" Player="1" ID="10">marine_squad</Object>
                    <Object Player="1" ID="20" Position="5,0,7">unsc_base</Object>
                </Objects>
            </Scenario>"#,
        )
        .unwrap();
        let mut db = Database::new();
        db.objects.push(ProtoObject {
            name: "marine".to_owned(),
            dbid: Some(101),
            object_class: Some("Unit".to_owned()),
            hitpoints: Some(75.0),
            ..ProtoObject::default()
        });
        db.objects.push(ProtoObject {
            name: "unsc_base".to_owned(),
            dbid: Some(202),
            object_class: Some("Building".to_owned()),
            flags: vec!["KBCreatesBase".to_owned()],
            hitpoints: Some(1_000.0),
            ..ProtoObject::default()
        });
        db.squads.push(ProtoSquad {
            name: "marine_squad".to_owned(),
            dbid: Some(303),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "marine".to_owned(),
                    count: 2,
                    role: None,
                }],
            }),
            ..ProtoSquad::default()
        });

        let loaded = load_scenario_into_world(&scenario, &db);
        let squad_id = loaded.get_entity_id(10).unwrap();
        let building_id = loaded.get_entity_id(20).unwrap();
        let squad = loaded.world.get_squad(squad_id).unwrap();
        let building = loaded.world.get_building(building_id).unwrap();

        assert_eq!(squad.proto_squad_id, 303);
        assert_eq!(squad.unit_ids.len(), 2);
        assert_eq!(building.proto_object_id, 202);
        assert!((building.hitpoints - 1_000.0).abs() < f32::EPSILON);
        assert_eq!(loaded.world.bases().count(), 1);
        assert!(building.base_id.is_some());
    }
}
