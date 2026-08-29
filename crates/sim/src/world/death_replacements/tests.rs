use super::*;
use crate::entities::{SquadMode, UnitState};
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::scenario::configure_unit_from_proto;
use glam::Vec3;
use pipeline::database::hw1::{Database, GameData, ProtoObject, Squad as ProtoSquad};

#[test]
fn regular_death_transforms_the_same_entity_and_retains_its_squad() {
    let database = database();
    let mut world = test_world();
    let (squad_id, source_id) = spawn_source(&mut world, &database, "source", 1);
    let squad = world.get_squad_mut(squad_id).unwrap();
    squad.mode = SquadMode::Passive;
    squad.proto_squad_name = "source".to_owned();
    squad.proto_squad_id = 10;
    world
        .get_unit_mut(source_id)
        .unwrap()
        .ammunition
        .set_current(5.0);
    let original_position = world.get_unit(source_id).unwrap().base.position;
    let original_forward = world.get_unit(source_id).unwrap().base.forward;

    assert!(world.kill_squad(squad_id, false));
    update(&mut world, &database);

    let replacement = world.get_unit(source_id).expect("same replacement ID");
    assert_eq!(replacement.proto_object_name, "replacement");
    assert_eq!(replacement.proto_object_id, 11);
    assert_eq!(replacement.state, UnitState::Idle);
    assert!(!replacement.is_alive());
    assert!(replacement.is_static_death_replacement());
    assert_eq!(replacement.hitpoints.to_bits(), 250.0_f32.to_bits());
    assert_eq!(replacement.shields.current.to_bits(), 30.0_f32.to_bits());
    assert_eq!(
        replacement.ammunition.current().to_bits(),
        25.0_f32.to_bits()
    );
    assert_eq!(replacement.base.position, original_position);
    assert_eq!(replacement.base.forward, original_forward);
    assert_eq!(replacement.squad_id, Some(squad_id));
    assert_eq!(
        replacement
            .object_state
            .scripted_animation()
            .map(crate::ScriptedAnimation::animation_type),
        Some("Idle")
    );
    let squad = world.get_squad(squad_id).expect("retained parent squad");
    assert!(squad.is_alive());
    assert_eq!(squad.unit_ids, vec![source_id]);
    assert_eq!(squad.proto_squad_name, "replacement");
}

#[test]
fn replacement_is_one_shot_deterministic_and_only_immediate_destroy_removes_it() {
    let database = database();
    let mut first = test_world();
    let mut second = test_world();
    let first_id = spawn_source(&mut first, &database, "source", 1).1;
    let second_id = spawn_source(&mut second, &database, "source", 1).1;
    assert_eq!(first_id, second_id);
    for (world, unit_id) in [(&mut first, first_id), (&mut second, second_id)] {
        assert!(world.kill_unit(unit_id, false));
        update(world, &database);
        update(world, &database);
    }

    assert_eq!(first.checksum(), second.checksum());
    assert!(!first.kill_unit(first_id, false));
    assert!(first.get_unit(first_id).is_some());
    assert!(first.kill_unit(first_id, true));
    assert!(first.get_unit(first_id).is_none());
}

#[test]
fn target_force_to_gaia_transfers_the_single_member_squad_and_normalizes_mode() {
    let mut database = database();
    database.objects[1]
        .flags
        .push("ForceToGaiaPlayer".to_owned());
    let mut world = test_world();
    let (squad_id, source_id) = spawn_source(&mut world, &database, "source", 1);
    world.get_squad_mut(squad_id).unwrap().mode = SquadMode::Cover;

    assert!(world.kill_unit(source_id, false));
    update(&mut world, &database);

    assert_eq!(world.entity_owner(source_id), Some(GAIA_PLAYER));
    assert_eq!(world.entity_owner(squad_id), Some(GAIA_PLAYER));
    assert_eq!(world.get_squad(squad_id).unwrap().mode, SquadMode::Normal);
}

#[test]
fn shatter_only_replacement_without_runtime_shatter_follows_ordinary_death() {
    let mut database = database();
    database.objects[0]
        .flags
        .push("ShatterDeathReplacement".to_owned());
    let mut world = test_world();
    let (squad_id, source_id) = spawn_source(&mut world, &database, "source", 1);

    assert!(world.kill_unit(source_id, false));
    update(&mut world, &database);

    assert!(world.get_unit(source_id).is_none());
    assert!(world.get_squad(squad_id).is_none());
}

#[test]
fn frozen_shatter_replacement_transforms_but_continues_normal_death_cleanup() {
    let mut database = database();
    database.objects[0]
        .flags
        .push("ShatterDeathReplacement".to_owned());
    database.game_data = Some(GameData {
        default_cryo_points: Some(100.0),
        time_frozen_to_thaw: Some(9.0),
        time_freezing_to_thaw: Some(3.0),
        ..GameData::default()
    });
    database.squads.push(ProtoSquad {
        name: "source".to_owned(),
        ..ProtoSquad::default()
    });
    let mut world = test_world();
    let (squad_id, source_id) = spawn_source(&mut world, &database, "source", 1);
    world.get_squad_mut(squad_id).unwrap().proto_squad_name = "source".to_owned();
    assert!(world.add_squad_cryo(squad_id, 100.0, &database));
    assert!(world.get_unit(source_id).unwrap().is_shatter_on_death());

    assert!(world.kill_unit(source_id, false));
    world.update_cryo(10.0);
    assert!(
        world.get_unit(source_id).unwrap().is_shatter_on_death(),
        "death captures the frozen flag before the owning squad thaws"
    );
    world.resolve_dead_unit_death_replacements(&database, None);

    let replacement = world
        .get_unit(source_id)
        .expect("shatter target remains observable during death resolution");
    assert_eq!(replacement.proto_object_name, "replacement");
    assert!(!replacement.is_alive());
    assert!(!replacement.is_static_death_replacement());
    assert!(replacement.is_shatter_on_death());
    update(&mut world, &database);
    assert!(world.get_unit(source_id).is_none());
    assert!(world.get_squad(squad_id).is_none());
}

#[test]
fn damaged_replacement_stays_invulnerable_until_direct_repair_completes() {
    let mut database = database();
    database.objects[1]
        .flags
        .push("DamagedDeathReplacement".to_owned());
    let mut world = test_world();
    let source_id = spawn_source(&mut world, &database, "source", 1).1;

    assert!(world.kill_unit(source_id, false));
    update(&mut world, &database);

    let replacement = world.get_unit(source_id).unwrap();
    assert_eq!(replacement.hitpoints.to_bits(), 1.0_f32.to_bits());
    assert!(replacement.is_death_replacement_healing());
    assert!(replacement.is_invulnerable());
    assert!(world.repair_unit(source_id, 249.0, 0.0));
    let replacement = world.get_unit(source_id).unwrap();
    assert_eq!(replacement.hitpoints.to_bits(), 250.0_f32.to_bits());
    assert!(!replacement.is_death_replacement_healing());
    assert!(!replacement.is_invulnerable());
    assert!(replacement.is_static_death_replacement());
}

#[test]
fn self_replacement_is_retained_but_immediate_source_destruction_bypasses_it() {
    let database = database();
    let mut retained_world = test_world();
    let retained = spawn_source(&mut retained_world, &database, "self_replacing", 1).1;
    assert!(retained_world.kill_unit(retained, false));
    update(&mut retained_world, &database);
    assert!(
        retained_world
            .get_unit(retained)
            .is_some_and(crate::entities::Unit::is_static_death_replacement)
    );
    assert_eq!(
        retained_world.get_unit(retained).unwrap().proto_object_name,
        "self_replacing"
    );

    let mut destroyed_world = test_world();
    let destroyed = spawn_source(&mut destroyed_world, &database, "source", 1).1;
    assert!(destroyed_world.kill_unit(destroyed, true));
    update(&mut destroyed_world, &database);
    assert!(destroyed_world.get_unit(destroyed).is_none());
}

fn database() -> Database {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "source".to_owned(),
            dbid: Some(10),
            object_class: Some("Unit".to_owned()),
            hitpoints: Some(100.0),
            ammo_max: Some(20.0),
            flags: vec!["StartAtMaxAmmo".to_owned()],
            death_replacement: Some("replacement".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "replacement".to_owned(),
            dbid: Some(11),
            object_class: Some("Building".to_owned()),
            damage_type: Some("Shielded".to_owned()),
            hitpoints: Some(250.0),
            shieldpoints: Some(30.0),
            ammo_max: Some(100.0),
            obstruction_radius_x: Some(3.0),
            obstruction_radius_y: Some(4.0),
            obstruction_radius_z: Some(5.0),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "self_replacing".to_owned(),
            dbid: Some(12),
            object_class: Some("Building".to_owned()),
            hitpoints: Some(75.0),
            death_replacement: Some("self_replacing".to_owned()),
            ..ProtoObject::default()
        },
    ]);
    database
}

fn test_world() -> World {
    let mut world = World::new();
    world.init_players(1);
    world
}

fn spawn_source(
    world: &mut World,
    database: &Database,
    proto_name: &str,
    player_id: u8,
) -> (EntityId, EntityId) {
    let (index, proto) = database
        .objects
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name == proto_name)
        .unwrap();
    let position = Vec3::new(7.0, 2.0, 9.0);
    let squad_id = world.create_squad_at(player_id, position);
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .base
        .set_forward(Vec3::X);
    let unit_id = if proto
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Building"))
    {
        world.create_building_at(player_id, position)
    } else {
        world.create_unit_at(player_id, position)
    };
    configure_unit_from_proto(world, unit_id, proto_name, index, proto);
    world
        .get_unit_mut(unit_id)
        .unwrap()
        .base
        .set_forward(Vec3::X);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

fn update(world: &mut World, database: &Database) {
    world.update_entities_with_database_and_gameplay(0.05, database, &GameplayCatalog::default());
}
