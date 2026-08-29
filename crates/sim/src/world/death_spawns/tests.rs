use super::*;
use crate::gameplay::GameplayCatalog;
use crate::scenario::{create_object_from_prototype, create_squad_from_prototype};
use pipeline::database::hw1::GameData;
use pipeline::database::hw1::Tech;
use pipeline::database::hw1::gamedata::{CodeObjectType, CodeObjectTypesWrapper};
use pipeline::database::hw1::objects::DeathSpawnSquad;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::TacticData;
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper, TechEffect};

#[test]
fn regular_death_spawns_the_authored_squad_and_destroy_bypasses_it() {
    let database = death_spawn_database(None, Vec::new());
    let position = Vec3::new(12.0, 3.0, 7.0);
    let mut world = test_world(&database);
    let source = spawn_source(&mut world, &database, position);

    assert!(world.kill_unit(source, false));
    update(&mut world, &database);

    assert!(world.get_unit(source).is_none());
    let squad = only_spawned_squad(&world);
    assert_eq!(squad.base.player_id, 1);
    assert_eq!(squad.base.position, position);
    assert_eq!(squad.base.forward, Vec3::X);
    assert_eq!(squad.proto_squad_id, 20);
    assert_eq!(squad.proto_squad_name, "spawn_squad");
    assert_eq!(squad.unit_ids.len(), 2);
    assert!(squad.unit_ids.iter().all(|unit_id| {
        world.get_unit(*unit_id).is_some_and(|unit| {
            unit.base.player_id == 1 && unit.proto_object_name == "spawn_member"
        })
    }));

    let mut destroyed_world = test_world(&database);
    let destroyed = spawn_source(&mut destroyed_world, &database, position);
    assert!(destroyed_world.kill_unit(destroyed, true));
    update(&mut destroyed_world, &database);
    assert_eq!(destroyed_world.player_squad_count(1, None), 0);
}

#[test]
fn maximum_count_is_checked_against_the_dying_player_before_spawn() {
    let database = death_spawn_database(Some(1), Vec::new());
    let mut world = test_world(&database);
    let existing =
        create_squad_from_prototype(&mut world, 1, Vec3::ZERO, Vec3::Z, "spawn_squad", &database);
    let first_source = spawn_source(&mut world, &database, Vec3::X);

    assert!(world.kill_unit(first_source, false));
    update(&mut world, &database);
    assert_eq!(world.player_squad_count(1, Some(20)), 1);

    assert!(world.kill_squad(existing, true));
    let second_source = spawn_source(&mut world, &database, Vec3::Y);
    assert!(world.kill_unit(second_source, false));
    update(&mut world, &database);
    assert_eq!(world.player_squad_count(1, Some(20)), 1);
}

#[test]
fn target_member_force_to_gaia_changes_squad_and_member_ownership() {
    let database = death_spawn_database(None, vec!["ForceToGaiaPlayer".to_owned()]);
    let mut world = test_world(&database);
    let source = spawn_source(&mut world, &database, Vec3::new(2.0, 0.0, 4.0));

    assert!(world.kill_unit(source, false));
    update(&mut world, &database);

    let squad = only_spawned_squad(&world);
    assert_eq!(squad.base.player_id, GAIA_PLAYER);
    assert!(squad.unit_ids.iter().all(|unit_id| {
        world
            .get_unit(*unit_id)
            .is_some_and(|unit| unit.base.player_id == GAIA_PLAYER)
    }));
}

#[test]
fn check_position_attribute_presence_suppresses_out_of_terrain_spawn() {
    let mut database = death_spawn_database(None, Vec::new());
    database.objects[0]
        .death_spawn_squad
        .as_mut()
        .unwrap()
        .check_position = Some(false);
    let mut world = test_world(&database);
    assert!(world.configure_terrain_bounds(Vec3::ZERO, Vec3::splat(10.0)));
    let outside = spawn_source(&mut world, &database, Vec3::new(11.0, 0.0, 5.0));

    assert!(world.kill_unit(outside, false));
    update(&mut world, &database);
    assert_eq!(world.player_squad_count(1, None), 0);

    let inside = spawn_source(&mut world, &database, Vec3::new(5.0, 0.0, 5.0));
    assert!(world.kill_unit(inside, false));
    update(&mut world, &database);
    assert_eq!(world.player_squad_count(1, Some(20)), 1);
}

#[test]
fn death_spawn_technology_assignment_is_player_specific_and_reversible() {
    let mut database = death_spawn_database(None, Vec::new());
    database.objects[0].death_spawn_squad = None;
    database.techs.push(Tech {
        name: "skull_test".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![TechEffect {
                effect_type: "Data".to_owned(),
                subtype: Some("DeathSpawn".to_owned()),
                squad_name: Some("spawn_squad".to_owned()),
                target: Some(EffectTarget {
                    target_type: Some("ProtoUnit".to_owned()),
                    value: Some("death_source".to_owned()),
                }),
                ..TechEffect::default()
            }],
        }),
        ..Tech::default()
    });
    let mut world = test_world(&database);
    let inactive_source = spawn_source(&mut world, &database, Vec3::ZERO);
    assert!(world.kill_unit(inactive_source, false));
    update(&mut world, &database);
    assert_eq!(world.player_squad_count(1, None), 0);

    assert!(
        world
            .activate_technology(1, &database, "skull_test")
            .unwrap()
    );
    let active_source = spawn_source(&mut world, &database, Vec3::X);
    assert!(world.kill_unit(active_source, false));
    update(&mut world, &database);
    assert_eq!(world.player_squad_count(1, Some(20)), 1);

    assert!(
        world
            .deactivate_technology(1, &database, "skull_test")
            .unwrap()
    );
    let inactive_again = spawn_source(&mut world, &database, Vec3::Y);
    assert!(world.kill_unit(inactive_again, false));
    update(&mut world, &database);
    assert_eq!(world.player_squad_count(1, Some(20)), 1);
}

#[test]
fn death_spawn_resolution_is_deterministic_and_one_shot() {
    let database = death_spawn_database(None, Vec::new());
    let mut first = test_world(&database);
    let mut second = test_world(&database);
    for world in [&mut first, &mut second] {
        let source = spawn_source(world, &database, Vec3::new(3.0, 0.0, 4.0));
        assert!(world.kill_unit(source, false));
        update(world, &database);
    }

    assert_eq!(first.checksum(), second.checksum());
    assert_eq!(first.player_squad_count(1, Some(20)), 1);
    update(&mut first, &database);
    assert_eq!(first.player_squad_count(1, Some(20)), 1);
}

#[test]
fn hero_death_spawn_enters_the_existing_downed_hero_lifecycle() {
    let mut database = death_spawn_database(None, Vec::new());
    database.game_data = Some(GameData {
        code_object_types: Some(CodeObjectTypesWrapper {
            entries: vec![CodeObjectType {
                object_type: "HeroDeath".to_owned(),
                value: "_HeroDeath".to_owned(),
            }],
        }),
        hero_hp_regen_time: Some(90.0),
        hero_revival_distance: Some(15.0),
        hero_percent_hp_revival_threshhold: Some(0.5),
        ..GameData::default()
    });
    database.objects[1].object_types = vec!["_HeroDeath".to_owned()];
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [("spawn_member".to_owned(), TacticData::default())],
    );
    let mut world = test_world(&database);
    let source = spawn_source(&mut world, &database, Vec3::ZERO);
    assert!(world.kill_unit(source, false));

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);

    let squad_id = world
        .squads
        .iter()
        .next()
        .map(|(squad_id, _)| squad_id)
        .expect("hero death squad");
    assert!(world.is_squad_down(squad_id));
    assert!(world.get_squad(squad_id).is_some_and(Entity::is_alive));
    assert!(
        world
            .get_squad(squad_id)
            .unwrap()
            .unit_ids
            .iter()
            .all(|unit_id| {
                world.get_unit(*unit_id).is_some_and(|unit| {
                    unit.is_down() && (unit.hitpoints - 1.0).abs() <= f32::EPSILON
                })
            })
    );
}

fn death_spawn_database(maximum_count: Option<i32>, member_flags: Vec<String>) -> Database {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "death_source".to_owned(),
            dbid: Some(10),
            object_class: Some("Unit".to_owned()),
            hitpoints: Some(100.0),
            death_spawn_squad: Some(DeathSpawnSquad {
                proto_squad: "spawn_squad".to_owned(),
                check_position: None,
                max_population_count: maximum_count,
            }),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "spawn_member".to_owned(),
            dbid: Some(11),
            object_class: Some("Unit".to_owned()),
            hitpoints: Some(25.0),
            flags: member_flags,
            ..ProtoObject::default()
        },
    ]);
    database.squads.push(ProtoSquad {
        name: "spawn_squad".to_owned(),
        dbid: Some(20),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: "spawn_member".to_owned(),
                count: 2,
                role: Some("normal".to_owned()),
            }],
        }),
        ..ProtoSquad::default()
    });
    database
}

fn test_world(database: &Database) -> World {
    let mut world = World::new();
    world.configure_prototype_catalogs(database);
    world.init_players(1);
    world
}

fn spawn_source(world: &mut World, database: &Database, position: Vec3) -> EntityId {
    create_object_from_prototype(world, 1, position, Vec3::X, "death_source", database).unwrap()
}

fn update(world: &mut World, database: &Database) {
    world.update_entities_with_database_and_gameplay(0.05, database, &GameplayCatalog::default());
}

fn only_spawned_squad(world: &World) -> &crate::entities::Squad {
    let squads = world
        .squads
        .iter()
        .map(|(_, squad)| squad)
        .collect::<Vec<_>>();
    assert_eq!(squads.len(), 1);
    squads[0]
}
