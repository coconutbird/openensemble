use super::*;

fn state(grid: &VisibilityGrid, team_id: TeamId, x: usize, z: usize) -> TileVisibility {
    grid.tile_visibility(team_id, x, z)
        .expect("tile on the map")
}

#[test]
fn new_grid_is_black_for_every_team() {
    let grid = VisibilityGrid::new(8, 2.0);

    assert_eq!(grid.axis(), 8);
    assert_eq!(grid.visibility_bits(7, 7), Some(0));
    assert_eq!(grid.visibility_bits(8, 0), None);
    assert_eq!(state(&grid, 1, 3, 3), TileVisibility::Unexplored);
    assert!(!grid.is_fully_explored(1));
}

#[test]
fn explore_sets_both_bits_and_unexplore_leaves_the_tile_explored() {
    let mut grid = VisibilityGrid::new(16, 1.0);

    grid.explore_circle(8, 8, 2, 1);
    assert_eq!(grid.visibility_bits(8, 8), Some(0x2_0002));
    assert_eq!(state(&grid, 1, 8, 6), TileVisibility::Visible);
    // The retail circle covers `-z..z`: row `8 + 2` is outside.
    assert_eq!(state(&grid, 1, 8, 10), TileVisibility::Unexplored);
    assert_eq!(state(&grid, 2, 8, 8), TileVisibility::Unexplored);

    grid.unexplore_circle(8, 8, 2, 1);
    assert_eq!(grid.visibility_bits(8, 8), Some(0x2));
    assert_eq!(state(&grid, 1, 8, 8), TileVisibility::Fogged);
}

#[test]
fn overlapping_circles_count_references() {
    let mut grid = VisibilityGrid::new(16, 1.0);

    grid.explore_circle(8, 8, 3, 2);
    grid.explore_circle(9, 8, 3, 2);
    grid.unexplore_circle(8, 8, 3, 2);

    assert_eq!(state(&grid, 2, 9, 8), TileVisibility::Visible);
    assert_eq!(state(&grid, 2, 5, 8), TileVisibility::Fogged);
    // An unexplore of a fogged tile does nothing (retail bails out), so the tile does not
    // turn black.
    grid.unexplore_circle(8, 8, 3, 2);
    assert_eq!(state(&grid, 2, 5, 8), TileVisibility::Fogged);
    assert_eq!(state(&grid, 2, 12, 8), TileVisibility::Visible);
}

#[test]
fn circle_is_clipped_at_the_map_edge() {
    let mut grid = VisibilityGrid::new(4, 1.0);

    grid.explore_circle(0, 0, 3, 1);
    grid.explore_circle(-100, i32::MAX, 3, 1);

    assert_eq!(state(&grid, 1, 0, 0), TileVisibility::Visible);
    assert_eq!(state(&grid, 1, 2, 1), TileVisibility::Visible);
    assert_eq!(state(&grid, 1, 2, 2), TileVisibility::Unexplored);
    assert_eq!(state(&grid, 1, 3, 3), TileVisibility::Unexplored);
}

#[test]
fn reset_black_map_blackens_only_fogged_tiles() {
    let mut grid = VisibilityGrid::new(16, 1.0);
    grid.explore_circle(4, 4, 2, 1);
    grid.explore_circle(12, 12, 2, 1);
    grid.unexplore_circle(4, 4, 2, 1);

    grid.reset_black_map(1);

    assert_eq!(state(&grid, 1, 4, 4), TileVisibility::Unexplored);
    assert_eq!(state(&grid, 1, 12, 12), TileVisibility::Visible);
    grid.unexplore_circle(12, 12, 2, 1);
    assert_eq!(state(&grid, 1, 12, 12), TileVisibility::Fogged);
}

#[test]
fn explore_and_unexplore_all_clears_the_black_map_and_keeps_counts() {
    let mut grid = VisibilityGrid::new(8, 1.0);
    grid.explore_circle(4, 4, 1, 3);

    grid.explore_and_unexplore_all(3);

    assert!(grid.is_fully_explored(3));
    assert_eq!(state(&grid, 3, 0, 0), TileVisibility::Fogged);
    assert_eq!(state(&grid, 3, 4, 4), TileVisibility::Visible);
    grid.unexplore_circle(4, 4, 1, 3);
    assert_eq!(state(&grid, 3, 4, 4), TileVisibility::Fogged);
}

#[test]
fn explore_all_then_unexplore_all_is_the_fog_switch() {
    let mut grid = VisibilityGrid::new(8, 1.0);
    grid.explore_circle(4, 4, 1, 1);

    grid.explore_all(1);
    assert_eq!(state(&grid, 1, 7, 7), TileVisibility::Visible);
    grid.unexplore_all(1);

    assert_eq!(state(&grid, 1, 7, 7), TileVisibility::Fogged);
    assert_eq!(state(&grid, 1, 4, 4), TileVisibility::Visible);
}

#[test]
fn teams_outside_the_retail_team_count_are_ignored() {
    let mut grid = VisibilityGrid::new(8, 1.0);
    let team = TeamId::try_from(MAX_TEAMS).unwrap();

    grid.explore_circle(4, 4, 2, team);
    grid.explore_all(team);

    assert_eq!(grid.visibility_bits(4, 4), Some(0));
    assert_eq!(state(&grid, team, 4, 4), TileVisibility::Unexplored);
}

#[test]
fn positions_truncate_toward_zero_like_retail() {
    let mut grid = VisibilityGrid::new(8, 2.0);
    grid.explore_circle(0, 0, 1, 1);

    assert_eq!(grid.tile_at(Vec3::new(-0.5, 0.0, 1.9)), Some((0, 0)));
    assert_eq!(grid.tile_at(Vec3::new(-2.0, 0.0, 0.0)), None);
    assert_eq!(grid.tile_at(Vec3::new(16.0, 0.0, 0.0)), None);
    assert_eq!(grid.tile_at(Vec3::new(15.9, 0.0, 3.0)), Some((7, 1)));
    assert!(grid.is_position_visible(1, Vec3::new(1.0, 0.0, -1.0)));
    assert!(grid.is_position_explored(1, Vec3::new(1.0, 0.0, -1.0)));
    assert!(!grid.is_position_visible(1, Vec3::new(1.0, 0.0, 2.0)));
}

#[test]
fn box_samples_are_clamped_to_the_map() {
    let mut grid = VisibilityGrid::new(8, 1.0);
    grid.explore_circle(7, 7, 1, 2);

    let far = Vec3::splat(1_000.0);
    assert_eq!(grid.bits_in_box(far, far) & 0x4_0000, 0x4_0000);
    assert_eq!(
        grid.bits_in_box(Vec3::new(6.5, 0.0, 5.5), Vec3::new(7.2, 0.0, 6.2)) & 0x4_0000,
        0x4_0000
    );
    assert_eq!(grid.bits_in_box(Vec3::splat(-9.0), Vec3::ZERO), 0);
}
