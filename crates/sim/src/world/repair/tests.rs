use super::*;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use glam::Vec3;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[test]
fn combat_value_repair_heals_existing_members_then_reinforces_in_node_order() {
    let database = database();
    let mut world = world(&database);
    let squad_id = spawn(&mut world, &database, Vec3::ZERO);
    let original = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert!(world.damage_unit_direct(original[0], 50.0, 0.0));
    world.remove_unit(original[1]).unwrap();

    world.repair_squads_by_combat_value(&database, &[squad_id], 10.0, false, false);

    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(
        squad.unit_ids.len(),
        2,
        "retail ignores AllowReinforce here"
    );
    let hitpoints = squad
        .unit_ids
        .iter()
        .map(|unit_id| world.get_unit(*unit_id).unwrap().hitpoints)
        .collect::<Vec<_>>();
    assert_eq!(hitpoints, vec![100.0, 50.0]);
    assert_eq!(
        world.squad_hitpoint_fraction(squad_id, &database).to_bits(),
        0.75_f32.to_bits()
    );
}

#[test]
fn spread_repair_spills_excess_into_the_more_damaged_squad() {
    let database = database();
    let mut world = world(&database);
    let first = spawn(&mut world, &database, Vec3::ZERO);
    let second = spawn(&mut world, &database, Vec3::X);
    let first_unit = world.get_squad(first).unwrap().unit_ids[0];
    let second_unit = world.get_squad(second).unwrap().unit_ids[0];
    assert!(world.damage_unit_direct(first_unit, 99.0, 0.0));
    assert!(world.damage_unit_direct(second_unit, 10.0, 0.0));

    world.repair_squads_by_combat_value(&database, &[first, second], 10.0, true, true);

    assert_eq!(
        world.get_unit(first_unit).unwrap().hitpoints.to_bits(),
        91.0_f32.to_bits()
    );
    assert_eq!(
        world.get_unit(second_unit).unwrap().hitpoints.to_bits(),
        100.0_f32.to_bits()
    );
}

#[test]
fn authored_single_member_squads_are_not_reinforced() {
    let mut database = database();
    database.squads[0].units.as_mut().unwrap().entries[0].count = 1;
    let mut world = world(&database);
    let squad_id = spawn(&mut world, &database, Vec3::ZERO);
    let unit_id = world.get_squad(squad_id).unwrap().unit_ids[0];
    world.get_unit_mut(unit_id).unwrap().kill();

    world.repair_squads_by_combat_value(&database, &[squad_id], 20.0, false, true);

    assert_eq!(world.get_squad(squad_id).unwrap().unit_ids, vec![unit_id]);
    assert!(!world.get_unit(unit_id).unwrap().is_alive());
}

fn world(database: &Database) -> World {
    let mut world = World::new();
    world.init_players(1);
    world.configure_prototype_catalogs(database);
    world
}

fn spawn(world: &mut World, database: &Database, position: Vec3) -> EntityId {
    let prototype_id = squad_prototype_id(database, "repair_squad").unwrap();
    spawn_squad_at(world, database, 1, prototype_id, position, Vec3::Z).unwrap()
}

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "repair_unit".to_owned(),
            dbid: Some(10),
            hitpoints: Some(100.0),
            combat_value: Some(10.0),
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
        ..Database::default()
    }
}
