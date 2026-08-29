use super::*;
use pipeline::database::hw1::{GameData, ProtoObject};

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: REVEALER_PROTO_NAME.to_owned(),
            dbid: Some(13),
            los: Some(1.0),
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            minimum_revealer_size: Some(4.0),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

#[test]
fn revealer_uses_first_team_player_and_scenario_database_scalars() {
    let database = database();
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 3;
    world.get_player_mut(2).unwrap().team_id = 3;

    let id = world
        .create_revealer(&database, 3, Vec3::new(10.0, 0.0, 20.0), 2.0, None)
        .unwrap();
    let object = world.get_object(id).unwrap();

    assert_eq!(id.class(), Some(crate::EntityClass::Object));
    assert_eq!(object.base.player_id, 1);
    assert_eq!(object.proto_object_id, 13);
    assert!((object.revealer().unwrap().line_of_sight_scalar() - 4.0).abs() < f32::EPSILON);
    assert!(world.is_position_revealed_to_team(3, Vec3::new(14.0, 100.0, 20.0)));
    assert!(!world.is_position_revealed_to_team(2, Vec3::new(10.0, 0.0, 20.0)));
}

#[test]
fn timed_revealer_expires_at_absolute_world_time_and_reuses_its_slot() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.advance_time(500);
    let old = world
        .create_revealer(&database, 1, Vec3::ZERO, 10.0, Some(100))
        .unwrap();

    world.advance_time(99);
    world.update_entities(0.05);
    assert!(world.get_revealer(old).is_some());
    world.advance_time(1);
    world.update_entities(0.05);
    assert!(world.get_revealer(old).is_none());

    let new = world
        .create_revealer(&database, 1, Vec3::ZERO, -1.0, None)
        .unwrap();
    assert_eq!(new.pool_index(), old.pool_index());
    assert_ne!(new.generation(), old.generation());
    assert!(world.is_position_revealed_to_team(1, Vec3::splat(1_000_000.0)));
}

#[test]
fn missing_team_or_revealer_prototype_fails_without_allocating() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);

    assert!(
        world
            .create_revealer(&database, 7, Vec3::ZERO, 5.0, None)
            .is_none()
    );
    assert!(
        world
            .create_revealer(&Database::default(), 0, Vec3::ZERO, 5.0, None)
            .is_none()
    );
    assert!(world.objects.is_empty());
}

#[test]
fn revealer_kill_is_immediate_and_state_participates_in_checksums() {
    let database = database();
    let mut first = World::new();
    first.init_players(1);
    first.get_player_mut(1).unwrap().team_id = 1;
    let mut second = World::new();
    second.init_players(1);
    second.get_player_mut(1).unwrap().team_id = 1;
    let first_id = first
        .create_revealer(&database, 1, Vec3::ZERO, 5.0, None)
        .unwrap();
    let second_id = second
        .create_revealer(&database, 1, Vec3::ZERO, 5.0, None)
        .unwrap();

    assert_eq!(first_id, second_id);
    assert_eq!(first.checksum(), second.checksum());
    first.update_entities(0.05);
    assert_ne!(first.checksum(), second.checksum());
    second.update_entities(0.05);
    assert_eq!(first.checksum(), second.checksum());
    assert!(first.kill_entity(first_id, false));
    assert!(first.get_object(first_id).is_none());
    assert_ne!(first.checksum(), second.checksum());
}

#[test]
fn fog_switch_controls_global_visibility_and_checksum_state() {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    let friendly = world.create_unit_at(1, Vec3::new(1.0, 0.0, 1.0));
    let enemy = world.create_unit_at(2, Vec3::new(100.0, 0.0, 100.0));
    let fog_checksum = world.checksum();

    assert!(world.fog_of_war_enabled());
    assert!(world.is_entity_visible_to_team(1, friendly));
    assert!(!world.is_entity_visible_to_team(1, enemy));
    assert!(!world.is_position_revealed_to_team(1, Vec3::splat(10_000.0)));

    world.set_fog_of_war_enabled(false);
    assert!(!world.fog_of_war_enabled());
    assert!(world.black_map_is_cleared());
    assert!(world.is_entity_visible_to_team(1, enemy));
    assert!(world.is_position_revealed_to_team(1, Vec3::splat(10_000.0)));
    assert_ne!(world.checksum(), fog_checksum);

    world.set_fog_of_war_enabled(true);
    assert_ne!(world.checksum(), fog_checksum);
    assert!(world.black_map_is_cleared());
    assert!(!world.is_entity_visible_to_team(1, enemy));
    world.reset();
    assert!(world.fog_of_war_enabled());
    assert!(!world.black_map_is_cleared());
}

#[test]
fn whole_map_exploration_is_authoritative_and_checksum_visible() {
    let mut world = World::new();
    let initial_checksum = world.checksum();

    world.clear_black_map();
    assert!(world.black_map_is_cleared());
    assert_ne!(world.checksum(), initial_checksum);

    world.reset_black_map();
    assert!(!world.black_map_is_cleared());
    assert_eq!(world.checksum(), initial_checksum);

    world.set_fog_of_war_enabled(false);
    world.reset_black_map();
    assert!(world.black_map_is_cleared());
}

#[test]
fn revealer_visibility_is_the_renderer_facing_enemy_visibility_source() {
    let database = database();
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    let enemy = world.create_unit_at(2, Vec3::new(8.0, 0.0, 0.0));

    assert!(!world.is_entity_visible_to_team(1, enemy));
    world
        .create_revealer(&database, 1, Vec3::ZERO, 10.0, None)
        .expect("team revealer");
    assert!(world.is_entity_visible_to_team(1, enemy));
}
