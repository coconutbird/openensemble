use super::*;
use crate::world::TileVisibility;
use crate::world::terrain::sampled_test_terrain;

/// A 64 by 64 tile map with a data tile scale of 2.
fn world_with_map(teams: &[TeamId]) -> World {
    let mut world = World::new();
    world.configure_terrain_simulation(sampled_test_terrain(8, 2.0, |_, _| 0.0));
    world.init_players(u8::try_from(teams.len()).unwrap());
    for (index, team_id) in teams.iter().enumerate() {
        let player = u8::try_from(index + 1).unwrap();
        world.get_player_mut(player).unwrap().team_id = *team_id;
    }
    world
}

fn unit_with_line_of_sight(world: &mut World, player: u8, position: Vec3, los: f32) -> EntityId {
    let id = world.create_unit_at(player, position);
    world
        .get_unit_mut(id)
        .unwrap()
        .configure_authored_line_of_sight(los);
    id
}

fn tile(world: &World, team_id: TeamId, x: usize, z: usize) -> TileVisibility {
    world
        .visibility_grid()
        .unwrap()
        .tile_visibility(team_id, x, z)
        .unwrap()
}

#[test]
fn map_uses_the_sim_data_tiles() {
    let world = world_with_map(&[1]);
    let grid = world.visibility_grid().unwrap();

    assert_eq!(grid.axis(), 64);
    assert!((grid.tile_scale() - 2.0).abs() < f32::EPSILON);
    assert!(World::new().visibility_grid().is_none());
}

#[test]
fn unit_explores_a_circle_of_truncated_los_tiles_at_its_tile() {
    let mut world = world_with_map(&[1]);
    // 9.9 / 2 = 4.95 tiles truncates to 4; position 41 / 2 = 20.5 truncates to tile 20.
    unit_with_line_of_sight(&mut world, 1, Vec3::new(41.0, 0.0, 41.0), 9.9);

    assert_eq!(tile(&world, 1, 20, 20), TileVisibility::Unexplored);
    world.update_entities(0.05);

    assert_eq!(tile(&world, 1, 20, 16), TileVisibility::Visible);
    assert_eq!(tile(&world, 1, 20, 23), TileVisibility::Visible);
    assert_eq!(tile(&world, 1, 20, 24), TileVisibility::Unexplored);
    assert_eq!(tile(&world, 1, 25, 20), TileVisibility::Unexplored);
}

#[test]
fn moving_unit_leaves_explored_fog_behind() {
    let mut world = world_with_map(&[1]);
    let unit = unit_with_line_of_sight(&mut world, 1, Vec3::new(20.0, 0.0, 20.0), 4.0);
    world.update_entities(0.05);
    assert_eq!(tile(&world, 1, 10, 10), TileVisibility::Visible);

    world.get_unit_mut(unit).unwrap().base.position = Vec3::new(80.0, 0.0, 80.0);
    world.update_entities(0.05);

    assert_eq!(tile(&world, 1, 10, 10), TileVisibility::Fogged);
    assert_eq!(tile(&world, 1, 40, 40), TileVisibility::Visible);
}

#[test]
fn dead_and_garrisoned_units_have_no_sight() {
    let mut world = world_with_map(&[1]);
    let dead = unit_with_line_of_sight(&mut world, 1, Vec3::new(20.0, 0.0, 20.0), 4.0);
    let inside = unit_with_line_of_sight(&mut world, 1, Vec3::new(80.0, 0.0, 80.0), 4.0);
    world.update_entities(0.05);

    assert!(world.kill_entity(dead, true));
    world
        .get_unit_mut(inside)
        .unwrap()
        .garrison
        .set_container(Some(dead));
    world.update_entities(0.05);

    assert_eq!(tile(&world, 1, 10, 10), TileVisibility::Fogged);
    assert_eq!(tile(&world, 1, 40, 40), TileVisibility::Fogged);
}

#[test]
fn units_of_one_team_share_sight_and_other_teams_do_not() {
    let mut world = world_with_map(&[1, 1, 2]);
    unit_with_line_of_sight(&mut world, 2, Vec3::new(20.0, 0.0, 20.0), 4.0);
    world.update_entities(0.05);

    assert_eq!(tile(&world, 1, 10, 10), TileVisibility::Visible);
    assert_eq!(tile(&world, 2, 10, 10), TileVisibility::Unexplored);
}

#[test]
fn los_scalar_scales_the_tiles_with_a_minimum_of_one() {
    let mut world = world_with_map(&[1]);
    let unit = unit_with_line_of_sight(&mut world, 1, Vec3::new(41.0, 0.0, 41.0), 10.0);
    world.update_entities(0.05);
    assert_eq!(tile(&world, 1, 20, 15), TileVisibility::Visible);

    world.get_unit_mut(unit).unwrap().line_of_sight_scalar = 0.5;
    world.update_entities(0.05);
    assert_eq!(tile(&world, 1, 20, 15), TileVisibility::Fogged);
    assert_eq!(tile(&world, 1, 20, 18), TileVisibility::Visible);

    world.get_unit_mut(unit).unwrap().line_of_sight_scalar = 0.01;
    world.update_entities(0.05);
    assert_eq!(tile(&world, 1, 20, 18), TileVisibility::Fogged);
    assert_eq!(tile(&world, 1, 20, 19), TileVisibility::Visible);
    assert_eq!(tile(&world, 1, 20, 20), TileVisibility::Visible);
}

#[test]
fn sim_line_of_sight_follows_the_retail_truncation_rules() {
    assert_eq!(proto_sim_line_of_sight(0.0, 0.5), 0);
    assert_eq!(proto_sim_line_of_sight(1.0, 0.5), 1);
    assert_eq!(proto_sim_line_of_sight(9.9, 0.5), 4);
    // A technology change of `LOS` has no minimum of 1.
    assert_eq!(unit_proto_sim_line_of_sight(10.0, 1.0, 0.5), 0);
    assert_eq!(unit_proto_sim_line_of_sight(1.0, 1.0, 0.5), 1);
    assert_eq!(scaled_sim_line_of_sight(10.0, 0.01, 0.5, false), 1);
    assert_eq!(scaled_sim_line_of_sight(10.0, 0.01, 0.5, true), WHOLE_MAP);
    assert_eq!(scaled_sim_line_of_sight(0.0, 4.0, 0.5, true), 0);
    assert_eq!(scaled_sim_line_of_sight(1.0, 9.0, 0.5, true), 4);
}

#[test]
fn whole_map_revealer_moves_to_the_center_and_unexplores_on_removal() {
    let mut world = world_with_map(&[1]);
    let revealer = Revealer::new(1, Revealer::GLOBAL_LINE_OF_SIGHT, 1.0, None);
    let id = world.objects.allocate_id();
    world.objects.insert(
        id,
        crate::entities::Object::new_revealer(
            id,
            1,
            Vec3::new(3.0, 0.0, 5.0),
            0,
            "sys_revealer".to_owned(),
            revealer,
        ),
    );
    world.update_entities(0.05);

    assert!(world.visibility_grid().unwrap().is_fully_explored(1));
    assert_eq!(tile(&world, 1, 63, 0), TileVisibility::Visible);
    assert_eq!(
        world.get_object(id).unwrap().base.position,
        Vec3::new(64.0, 0.0, 64.0)
    );

    assert!(world.remove_object(id).is_some());
    world.update_entities(0.05);
    assert_eq!(tile(&world, 1, 63, 0), TileVisibility::Fogged);
}

#[test]
fn owner_change_moves_the_circle_to_the_new_team() {
    let mut world = world_with_map(&[1, 2]);
    let unit = unit_with_line_of_sight(&mut world, 1, Vec3::new(20.0, 0.0, 20.0), 4.0);
    world.update_entities(0.05);

    world.get_unit_mut(unit).unwrap().base.player_id = 2;
    world.update_entities(0.05);

    assert_eq!(tile(&world, 1, 10, 10), TileVisibility::Fogged);
    assert_eq!(tile(&world, 2, 10, 10), TileVisibility::Visible);
}

#[test]
fn sight_marks_and_tiles_are_in_the_checksum() {
    let mut first = world_with_map(&[1]);
    let mut second = world_with_map(&[1]);
    unit_with_line_of_sight(&mut first, 1, Vec3::new(20.0, 0.0, 20.0), 4.0);
    unit_with_line_of_sight(&mut second, 1, Vec3::new(20.0, 0.0, 20.0), 4.0);
    assert_eq!(first.checksum(), second.checksum());

    let before = first.checksum();
    first.update_line_of_sight();
    assert_ne!(first.checksum(), before);
    second.update_line_of_sight();
    assert_eq!(first.checksum(), second.checksum());
}

#[test]
fn map_built_with_fog_disabled_is_fully_visible() {
    let mut world = World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.set_fog_of_war_enabled(false);
    world.configure_terrain_simulation(sampled_test_terrain(1, 1.0, |_, _| 0.0));

    assert_eq!(tile(&world, 1, 7, 7), TileVisibility::Visible);
    world.set_fog_of_war_enabled(true);
    assert_eq!(tile(&world, 1, 7, 7), TileVisibility::Fogged);
}
