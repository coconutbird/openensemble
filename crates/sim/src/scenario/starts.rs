//! Deterministic player-start assignment and initial base creation.

use std::collections::BTreeMap;

use glam::Vec3;
use pipeline::database::hw1::{Database, Leader, ProtoObject};

use super::{
    PlacedUnitKind, ScenarioData, ScenarioPosition, ScenarioPositionAxes, classify_proto_object,
    configure_unit_from_proto, creates_base, find_proto_object,
};
use crate::entities::BaseId;
use crate::player::PlayerId;
use crate::world::World;

const SKIRMISH_EMPTY_BASE_OBJECT: &str = "SkirmishEmptyBaseObject";

pub(super) fn create_initial_bases(
    world: &mut World,
    scenario: &ScenarioData,
    database: &Database,
    player_count: u8,
    max_players: Option<u32>,
) -> BTreeMap<PlayerId, BaseId> {
    let axes = ScenarioPositionAxes::infer(scenario, max_players.or(Some(u32::from(player_count))));
    let starts = assign_player_starts(scenario, player_count, max_players);
    let mut base_ids = BTreeMap::new();

    for (player_id, start) in starts {
        if let Some(base_id) = existing_player_base(world, player_id) {
            base_ids.insert(player_id, base_id);
            continue;
        }
        let Some(base_id) = create_initial_base(world, database, player_id, start, axes) else {
            continue;
        };
        base_ids.insert(player_id, base_id);
    }

    base_ids
}

fn assign_player_starts(
    scenario: &ScenarioData,
    player_count: u8,
    max_players: Option<u32>,
) -> BTreeMap<PlayerId, &ScenarioPosition> {
    let mut assignments = BTreeMap::new();
    let valid_starts = scenario
        .positions()
        .iter()
        .filter(|start| valid_start_number(start.number, max_players))
        .collect::<Vec<_>>();

    for start in &valid_starts {
        let Ok(player_id) = PlayerId::try_from(start.player) else {
            continue;
        };
        if player_id > 0 && player_id <= player_count {
            assignments.entry(player_id).or_insert(*start);
        }
    }

    let mut available_players = (1..=player_count)
        .filter(|id| !assignments.contains_key(id))
        .collect::<Vec<_>>()
        .into_iter();
    for start in valid_starts.into_iter().filter(|start| start.player <= 0) {
        let Some(player_id) = available_players.next() else {
            break;
        };
        assignments.insert(player_id, start);
    }

    assignments
}

fn valid_start_number(number: i32, max_players: Option<u32>) -> bool {
    u32::try_from(number)
        .is_ok_and(|number| number > 0 && max_players.is_none_or(|maximum| number <= maximum))
}

fn existing_player_base(world: &World, player_id: PlayerId) -> Option<BaseId> {
    world
        .bases()
        .find_map(|(base_id, base)| (base.player_id == player_id).then_some(*base_id))
}

fn create_initial_base(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    start: &ScenarioPosition,
    axes: ScenarioPositionAxes,
) -> Option<BaseId> {
    let (proto_index, proto) = initial_base_proto(world, database, player_id)?;
    let position = Vec3::from_array(axes.position_to_world(start.position_vec3()));
    let forward = Vec3::from_array(axes.direction_to_world(start.forward_vec3()));
    if !position.is_finite() || !forward.is_finite() {
        return None;
    }

    let anchor_id = world.create_building_at(player_id, position);
    configure_unit_from_proto(world, anchor_id, proto.name.trim(), proto_index, proto);
    super::population::apply_object_population(world, anchor_id, database, proto);
    if let Some(anchor) = world.get_building_mut(anchor_id) {
        anchor.base.set_forward(forward);
    }
    world.register_base(anchor_id)
}

fn initial_base_proto<'a>(
    world: &World,
    database: &'a Database,
    player_id: PlayerId,
) -> Option<(usize, &'a ProtoObject)> {
    let leader = world
        .get_player(player_id)
        .and_then(|player| usize::try_from(player.leader_id).ok())
        .and_then(|leader_id| database.leaders.get(leader_id));
    leader
        .and_then(|leader| leader_base_proto(leader, database))
        .or_else(|| mapped_empty_base_proto(database))
}

fn leader_base_proto<'a>(
    leader: &Leader,
    database: &'a Database,
) -> Option<(usize, &'a ProtoObject)> {
    base_candidates(leader, true)
        .find_map(|name| find_building_proto(database, name, true))
        .or_else(|| {
            base_candidates(leader, true)
                .find_map(|name| find_building_proto(database, name, false))
        })
        .or_else(|| {
            base_candidates(leader, false)
                .find_map(|name| find_building_proto(database, name, false))
        })
}

fn base_candidates(leader: &Leader, build_targets: bool) -> impl Iterator<Item = &str> {
    leader.starting_units.iter().filter_map(move |unit| {
        if build_targets {
            unit.build_other.as_deref()
        } else {
            Some(unit.proto_object.as_str())
        }
    })
}

fn find_building_proto<'a>(
    database: &'a Database,
    name: &str,
    must_create_base: bool,
) -> Option<(usize, &'a ProtoObject)> {
    let resolved = find_proto_object(database, name.trim())?;
    (classify_proto_object(resolved.1) == Some(PlacedUnitKind::Building)
        && (!must_create_base || creates_base(resolved.1)))
    .then_some(resolved)
}

fn mapped_empty_base_proto(database: &Database) -> Option<(usize, &ProtoObject)> {
    let proto_name = database
        .game_data
        .as_ref()?
        .code_proto_objects
        .as_ref()?
        .entries
        .iter()
        .find(|mapping| {
            mapping
                .object_type
                .eq_ignore_ascii_case(SKIRMISH_EMPTY_BASE_OBJECT)
        })?
        .proto_name
        .trim();
    find_building_proto(database, proto_name, false)
}

#[cfg(test)]
mod tests {
    use pipeline::database::hw1::gamedata::{CodeProtoObject, CodeProtoObjectsWrapper};
    use pipeline::database::hw1::leaders::StartingUnit;
    use pipeline::database::hw1::{Database, GameData, Leader, ProtoObject};

    use super::super::load_scenario_into_world;
    use super::*;

    #[test]
    fn creates_completed_leader_base_at_assigned_start() {
        let scenario = ScenarioData::from_xml_str(
            r#"<Scenario>
                <Positions>
                    <Position Number="1" Position="12,3,34" Forward="1,0,0" />
                </Positions>
                <Players>
                    <Player Name="P1" Leader1="Cutter" Team="1" />
                </Players>
            </Scenario>"#,
        )
        .expect("valid scenario");
        let mut database = database_with_empty_base_mapping();
        database.objects.push(ProtoObject {
            name: "unsc_main_base".to_owned(),
            dbid: Some(200),
            object_class: Some("Building".to_owned()),
            flags: vec!["KBCreatesBase".to_owned()],
            ..ProtoObject::default()
        });
        database.leaders.push(Leader {
            name: "Cutter".to_owned(),
            starting_units: vec![StartingUnit {
                proto_object: "empty_base".to_owned(),
                build_other: Some("unsc_main_base".to_owned()),
                ..StartingUnit::default()
            }],
            ..Leader::default()
        });

        let loaded = load_scenario_into_world(&scenario, &database);
        let base_id = loaded.get_initial_base_id(1).expect("player base");
        let base = loaded.world.get_base(base_id).expect("registered base");
        let anchor = loaded
            .world
            .get_building(base.anchor_building_id)
            .expect("base anchor");

        assert_eq!(base.player_id, 1);
        assert_eq!(anchor.proto_object_name, "unsc_main_base");
        assert_eq!(anchor.proto_object_id, 200);
        assert!(
            base.position
                .abs_diff_eq(Vec3::new(12.0, 3.0, 34.0), 1.0e-6)
        );
        assert!(anchor.base.forward.abs_diff_eq(Vec3::X, 1.0e-6));
    }

    #[test]
    fn falls_back_to_game_data_empty_base_mapping() {
        let scenario = ScenarioData::from_xml_str(
            r#"<Scenario>
                <Positions><Position Number="1" Position="4,0,8" /></Positions>
                <Players><Player Name="P1" Team="1" /></Players>
            </Scenario>"#,
        )
        .expect("valid scenario");
        let database = database_with_empty_base_mapping();

        let loaded = load_scenario_into_world(&scenario, &database);
        let base_id = loaded.get_initial_base_id(1).expect("fallback base");
        let base = loaded.world.get_base(base_id).expect("registered base");
        let anchor = loaded
            .world
            .get_building(base.anchor_building_id)
            .expect("base anchor");

        assert_eq!(anchor.proto_object_name, "empty_base");
        assert_eq!(loaded.world.bases().count(), 1);
    }

    fn database_with_empty_base_mapping() -> Database {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "empty_base".to_owned(),
            dbid: Some(100),
            object_class: Some("Building".to_owned()),
            ..ProtoObject::default()
        });
        database.game_data = Some(GameData {
            code_proto_objects: Some(CodeProtoObjectsWrapper {
                entries: vec![CodeProtoObject {
                    object_type: SKIRMISH_EMPTY_BASE_OBJECT.to_owned(),
                    proto_name: "empty_base".to_owned(),
                }],
            }),
            ..GameData::default()
        });
        database
    }
}
