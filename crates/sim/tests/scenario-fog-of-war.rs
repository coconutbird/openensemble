//! Checks the retail visibility map of a 1v1 Blood Gulch skirmish after its first updates.

use glam::Vec3;
use sim::TileVisibility;

fn unit_position(world: &sim::World, proto: &str, player: u8) -> Vec3 {
    world
        .units
        .iter()
        .find(|(_, unit)| unit.proto_object_name == proto && unit.base.player_id == player)
        .map(|(_, unit)| unit.base.position)
        .expect("starting unit")
}

fn count_tiles(grid: &sim::VisibilityGrid, team_id: u8, state: TileVisibility) -> usize {
    (0..grid.axis())
        .flat_map(|x| (0..grid.axis()).map(move |z| (x, z)))
        .filter(|&(x, z)| grid.tile_visibility(team_id, x, z) == Some(state))
        .count()
}

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-fog-of-war -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_starting_bases_see_their_surroundings_and_not_the_enemy() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let database = &loaded.content.database;
    let mut clock = sim::Simulation::new();
    for _ in 0..20 {
        clock.tick_with_scenario(&mut loaded.simulation, database);
    }

    let world = &loaded.simulation.world;
    assert!(world.fog_of_war_enabled());
    let grid = world
        .visibility_grid()
        .expect("the scenario has sim terrain");
    let team = |player: u8| world.get_player(player).unwrap().team_id;
    let (first, second) = (team(1), team(2));
    assert_ne!(first, second);
    let warthog = unit_position(world, "unsc_veh_warthog_01", 1);
    let ghost = unit_position(world, "cov_veh_ghost_01", 2);
    let hq = unit_position(world, "unsc_bldg_command_03", 1);
    let tile = |team_id: u8, position: Vec3| {
        let (x, z) = grid.tile_at(position).expect("unit on the map");
        grid.tile_visibility(team_id, x, z).unwrap()
    };

    // Each base sees the tiles of its starting units, and the enemy base stays black.
    for position in [warthog, hq, warthog + Vec3::new(4.0, 0.0, 4.0)] {
        assert_eq!(tile(first, position), TileVisibility::Visible);
    }
    assert_eq!(tile(second, ghost), TileVisibility::Visible);
    assert_eq!(tile(first, ghost), TileVisibility::Unexplored);
    assert_eq!(tile(second, warthog), TileVisibility::Unexplored);
    assert!(!world.black_map_is_cleared());

    // The sight circles are finite: the team sees part of the map and has explored nothing
    // that it does not see yet.
    let visible = count_tiles(grid, first, TileVisibility::Visible);
    let fogged = count_tiles(grid, first, TileVisibility::Fogged);
    let total = grid.axis() * grid.axis();
    println!(
        "team {first}: {visible} visible and {fogged} fogged tiles of {total} ({} by {}, scale {})",
        grid.axis(),
        grid.axis(),
        grid.tile_scale()
    );
    assert!(visible > 100, "{visible} visible tiles");
    assert!(visible * 4 < total, "{visible} of {total} tiles visible");

    let warthog_id = world
        .units
        .iter()
        .find(|(_, unit)| unit.proto_object_name == "unsc_veh_warthog_01")
        .map(|(id, _)| id)
        .unwrap();
    assert!(world.is_entity_visible_to_team(first, warthog_id));
    assert!(!world.is_entity_visible_to_team(second, warthog_id));
}
