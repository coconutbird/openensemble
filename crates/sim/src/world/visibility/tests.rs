use super::*;
use crate::world::terrain::sampled_test_terrain;
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

/// A 128 by 128 tile map with a data tile scale of 1, and one player on each given team.
fn world_with_map(teams: &[TeamId]) -> World {
    let mut world = World::new();
    world.configure_terrain_simulation(sampled_test_terrain(16, 1.0, |_, _| 0.0));
    world.init_players(u8::try_from(teams.len()).unwrap());
    for (index, team_id) in teams.iter().enumerate() {
        let player = u8::try_from(index + 1).unwrap();
        world.get_player_mut(player).unwrap().team_id = *team_id;
    }
    world
}

#[test]
fn revealer_uses_first_team_player_and_scenario_database_scalars() {
    let database = database();
    let mut world = world_with_map(&[3, 3]);

    let id = world
        .create_revealer(&database, 3, Vec3::new(10.0, 0.0, 20.0), 2.0, None)
        .unwrap();
    let object = world.get_object(id).unwrap();

    assert_eq!(id.class(), Some(crate::EntityClass::Object));
    assert_eq!(object.base.player_id, 1);
    assert_eq!(object.proto_object_id, 0);
    assert!((object.revealer().unwrap().line_of_sight_scalar() - 4.0).abs() < f32::EPSILON);
    // The circle is marked on the next object update.
    assert!(!world.is_position_visible_to_team(3, Vec3::new(13.0, 100.0, 20.0)));
    world.update_entities(0.05);
    assert!(world.is_position_visible_to_team(3, Vec3::new(13.0, 100.0, 20.0)));
    assert!(!world.is_position_visible_to_team(3, Vec3::new(15.0, 0.0, 20.0)));
    assert!(!world.is_position_visible_to_team(2, Vec3::new(10.0, 0.0, 20.0)));
}

#[test]
fn timed_revealer_expires_at_absolute_world_time_and_reuses_its_slot() {
    let database = database();
    let mut world = world_with_map(&[1]);
    world.advance_time(500);
    let old = world
        .create_revealer(&database, 1, Vec3::new(30.0, 0.0, 30.0), 10.0, Some(100))
        .unwrap();

    world.advance_time(99);
    world.update_entities(0.05);
    assert!(world.get_revealer(old).is_some());
    assert!(world.is_position_visible_to_team(1, Vec3::new(30.0, 0.0, 30.0)));
    world.advance_time(1);
    world.update_entities(0.05);
    assert!(world.get_revealer(old).is_none());
    assert!(!world.is_position_visible_to_team(1, Vec3::new(30.0, 0.0, 30.0)));
    assert!(world.is_position_explored_to_team(1, Vec3::new(30.0, 0.0, 30.0)));

    let new = world
        .create_revealer(&database, 1, Vec3::ZERO, -1.0, None)
        .unwrap();
    assert_eq!(new.pool_index(), old.pool_index());
    assert_ne!(new.generation(), old.generation());
    world.update_entities(0.05);
    assert!(world.is_position_visible_to_team(1, Vec3::new(127.0, 0.0, 0.0)));
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
    let mut first = world_with_map(&[1]);
    let mut second = world_with_map(&[1]);
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
fn fog_switch_explores_and_unexplores_the_whole_map() {
    let mut world = world_with_map(&[1, 2]);
    let friendly = world.create_unit_at(1, Vec3::new(1.0, 0.0, 1.0));
    let enemy = world.create_unit_at(2, Vec3::new(100.0, 0.0, 100.0));
    world.update_entities(0.05);
    let fog_checksum = world.checksum();

    assert!(world.fog_of_war_enabled());
    assert!(world.is_entity_visible_to_team(1, friendly));
    assert!(!world.is_entity_visible_to_team(1, enemy));
    assert!(!world.is_position_visible_to_team(1, Vec3::splat(50.0)));

    world.set_fog_of_war_enabled(false);
    assert!(!world.fog_of_war_enabled());
    assert!(world.black_map_is_cleared());
    assert!(world.is_entity_visible_to_team(1, enemy));
    assert!(world.is_position_visible_to_team(1, Vec3::splat(50.0)));
    assert_ne!(world.checksum(), fog_checksum);

    world.set_fog_of_war_enabled(true);
    assert_ne!(world.checksum(), fog_checksum);
    assert!(world.black_map_is_cleared());
    assert!(!world.is_entity_visible_to_team(1, enemy));
    assert!(world.is_position_explored_to_team(1, Vec3::splat(50.0)));
    world.reset();
    assert!(world.fog_of_war_enabled());
    assert!(!world.black_map_is_cleared());
}

#[test]
fn ordinary_unit_los_is_team_shared_and_uses_live_radius_scalars() {
    let mut world = world_with_map(&[1, 1, 2]);
    let observer = world.create_unit_at(2, Vec3::new(40.0, 0.0, 40.0));
    world
        .get_unit_mut(observer)
        .unwrap()
        .configure_authored_line_of_sight(10.0);
    let enemy = world.create_unit_at(3, Vec3::new(52.0, 0.0, 40.0));
    world.get_unit_mut(enemy).unwrap().obstruction_half_extents = Vec3::splat(1.0);
    world.update_entities(0.05);

    // The circle of 10 tiles ends at tile 50; the sim box of the enemy starts at tile 51.
    assert!(!world.is_entity_visible_to_team(1, enemy));
    world.get_unit_mut(enemy).unwrap().obstruction_half_extents = Vec3::splat(2.0);
    assert!(world.is_entity_visible_to_team(1, enemy));
    assert!(!world.is_position_visible_to_team(1, Vec3::new(52.0, 0.0, 40.0)));

    world.get_unit_mut(observer).unwrap().line_of_sight_scalar = 0.5;
    world.update_entities(0.05);
    assert!(!world.is_entity_visible_to_team(1, enemy));
}

#[test]
fn whole_map_exploration_is_authoritative_and_checksum_visible() {
    let mut world = world_with_map(&[1]);
    let initial_checksum = world.checksum();

    world.clear_black_map();
    assert!(world.black_map_is_cleared());
    assert!(!world.is_position_visible_to_team(1, Vec3::splat(5.0)));
    assert_ne!(world.checksum(), initial_checksum);

    world.reset_black_map();
    assert!(!world.black_map_is_cleared());
    assert_eq!(world.checksum(), initial_checksum);

    world.set_fog_of_war_enabled(false);
    world.reset_black_map();
    assert!(world.black_map_is_cleared());
}

#[test]
fn reset_black_map_keeps_the_tiles_that_a_unit_sees() {
    let mut world = world_with_map(&[1]);
    let unit = world.create_unit_at(1, Vec3::new(20.0, 0.0, 20.0));
    world
        .get_unit_mut(unit)
        .unwrap()
        .configure_authored_line_of_sight(5.0);
    world.update_entities(0.05);
    world.clear_black_map();

    world.reset_black_map();

    assert!(world.is_position_visible_to_team(1, Vec3::new(20.0, 0.0, 20.0)));
    assert!(!world.is_position_explored_to_team(1, Vec3::new(90.0, 0.0, 90.0)));
}

#[test]
fn revealer_visibility_is_the_renderer_facing_enemy_visibility_source() {
    let database = database();
    let mut world = world_with_map(&[1, 2]);
    let enemy = world.create_unit_at(2, Vec3::new(8.0, 0.0, 0.0));
    world.update_entities(0.05);

    assert!(!world.is_entity_visible_to_team(1, enemy));
    world
        .create_revealer(&database, 1, Vec3::ZERO, 10.0, None)
        .expect("team revealer");
    world.update_entities(0.05);
    assert!(world.is_entity_visible_to_team(1, enemy));
}

#[test]
fn world_without_sim_terrain_sees_only_its_own_entities() {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    let friendly = world.create_unit_at(1, Vec3::ZERO);
    let enemy = world.create_unit_at(2, Vec3::X);
    world.update_entities(0.05);

    assert!(world.visibility_grid().is_none());
    assert!(world.is_entity_visible_to_team(1, friendly));
    assert!(!world.is_entity_visible_to_team(1, enemy));
    world.set_fog_of_war_enabled(false);
    assert!(world.is_entity_visible_to_team(1, enemy));
}
