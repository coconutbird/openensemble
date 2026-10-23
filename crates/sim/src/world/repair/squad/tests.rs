//! Work order 4 (Repair): `BSquadActionRepair` (`BSquad_executeOrder` @ `0x140399780` case 4).

use super::*;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use glam::Vec3;
use pipeline::database::hw1::leaders::Leader;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{ProtoObject, Squad as ProtoSquad};

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "repair_unit".to_owned(),
            dbid: Some(10),
            hitpoints: Some(100.0),
            object_types: vec!["Military".to_owned()],
            ..ProtoObject::default()
        }],
        squads: vec![ProtoSquad {
            name: "repair_squad".to_owned(),
            dbid: Some(20),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "repair_unit".to_owned(),
                    count: 2,
                    ..UnitEntry::default()
                }],
            }),
            ..ProtoSquad::default()
        }],
        leaders: vec![Leader {
            name: "repair_leader".to_owned(),
            repair_time: Some(10.0),
            ..Leader::default()
        }],
        ..Database::default()
    }
}

/// Player 1 with the 10-second leader repair time and one squad of two members.
fn world_with_squad(database: &Database) -> (World, EntityId) {
    let mut world = World::new();
    world.init_players(2);
    world.configure_prototype_catalogs(database);
    world.get_player_mut(1).unwrap().leader_id = 0;
    let prototype_id = squad_prototype_id(database, "repair_squad").unwrap();
    let squad_id = spawn_squad_at(&mut world, database, 1, prototype_id, Vec3::ZERO, Vec3::Z)
        .expect("the squad spawns");
    (world, squad_id)
}

fn repair_action(world: &World, squad_id: EntityId) -> &SquadRepairAction {
    match world.get_squad(squad_id).unwrap().work_action.as_ref() {
        Some(SquadWorkAction::Repair(action)) => action,
        other => panic!("expected a repair action, found {other:?}"),
    }
}

fn hitpoints(world: &World, unit_id: EntityId) -> f32 {
    world.get_unit(unit_id).unwrap().hitpoints
}

#[test]
fn repair_records_the_damaged_and_missing_members_and_splits_the_repair_time() {
    let database = database();
    let (mut world, squad_id) = world_with_squad(&database);
    let members = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert!(world.damage_unit_direct(members[0], 50.0, 0.0));
    let _removed = world.remove_unit(members[1]).unwrap();

    assert!(world.issue_squad_repair_order(1, squad_id, &database));

    let action = repair_action(&world, squad_id);
    assert_eq!(
        action.members,
        vec![RepairMember {
            unit_id: members[0],
            start_hitpoints: 50.0,
        }]
    );
    assert_eq!(action.reinforcements, vec!["repair_unit".to_owned()]);
    assert_eq!(action.repair_time.to_bits(), 10.0_f32.to_bits());
    // 50 damaged hitpoints against 100 missing hitpoints: a third of the time heals.
    assert_eq!(
        action.repair_percent.to_bits(),
        (50.0_f32 / 150.0).to_bits()
    );
    assert!(world.get_squad(squad_id).unwrap().is_self_repairing());
}

#[test]
fn repair_heals_first_then_reinforces_at_the_end_of_the_repair_time() {
    let database = database();
    let (mut world, squad_id) = world_with_squad(&database);
    let members = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert!(world.damage_unit_direct(members[0], 50.0, 0.0));
    let _removed = world.remove_unit(members[1]).unwrap();
    assert!(world.issue_squad_repair_order(1, squad_id, &database));

    // pct 0.1 of a repair part of 1/3: 30 percent of the 50 damaged hitpoints.
    world.update_squad_work_actions(1.0, &database);
    assert!((hitpoints(&world, members[0]) - 65.0).abs() < 1.0e-3);
    assert_eq!(world.get_squad(squad_id).unwrap().unit_ids.len(), 1);

    world.update_squad_work_actions(3.0, &database);
    assert_eq!(hitpoints(&world, members[0]).to_bits(), 100.0_f32.to_bits());
    assert_eq!(world.get_squad(squad_id).unwrap().unit_ids.len(), 1);
    assert!(world.get_squad(squad_id).unwrap().is_self_repairing());

    world.update_squad_work_actions(6.0, &database);
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.unit_ids.len(), 2);
    assert!(squad.work_action.is_none());
}

#[test]
fn repair_is_refused_while_the_squad_repairs_itself() {
    let database = database();
    let (mut world, squad_id) = world_with_squad(&database);
    let members = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert!(world.damage_unit_direct(members[0], 50.0, 0.0));
    assert!(world.issue_squad_repair_order(1, squad_id, &database));
    world.update_squad_work_actions(1.0, &database);
    let before = repair_action(&world, squad_id).clone();

    assert!(!world.issue_squad_repair_order(1, squad_id, &database));

    assert_eq!(repair_action(&world, squad_id), &before);
}

#[test]
fn repair_is_refused_for_another_player_squad() {
    let database = database();
    let (mut world, squad_id) = world_with_squad(&database);

    assert!(!world.issue_squad_repair_order(2, squad_id, &database));

    assert!(world.get_squad(squad_id).unwrap().work_action.is_none());
}

#[test]
fn repair_stops_when_the_squad_takes_damage() {
    let database = database();
    let (mut world, squad_id) = world_with_squad(&database);
    let members = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert!(world.damage_unit_direct(members[0], 50.0, 0.0));
    assert!(world.issue_squad_repair_order(1, squad_id, &database));
    let damaged_time = repair_action(&world, squad_id).damaged_time;
    world.get_squad_mut(squad_id).unwrap().last_damaged_time = damaged_time + 1;

    world.update_squad_work_actions(1.0, &database);

    assert!(world.get_squad(squad_id).unwrap().work_action.is_none());
    assert_eq!(hitpoints(&world, members[0]).to_bits(), 50.0_f32.to_bits());
}
