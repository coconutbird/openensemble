use super::*;

#[test]
fn xsd_height_queries_reproduce_retail_blocked_grid_and_clamping() {
    let mut terrain = terrain(9, 16, 2.0);
    set_height(&mut terrain, 3, 5, 42.5);
    set_height(&mut terrain, 8, 8, -7.25);
    let terrain = TerrainSimulation::new(terrain);

    let interpolated = terrain
        .height(Vec3::new(6.9, 999.0, 10.1), false)
        .expect("interpolated terrain height");
    assert!((interpolated - 22.206_25).abs() < 0.000_1);
    assert_eq!(terrain.height(Vec3::new(99.0, 0.0, 99.0), false), None);
    assert_eq!(
        terrain.height(Vec3::new(99.0, 0.0, 99.0), true),
        Some(-7.25)
    );
}

#[test]
fn xsd_height_queries_read_exact_grid_points() {
    let mut terrain = terrain(8, 8, 1.0);
    set_height(&mut terrain, 7, 2, 6.5);
    let terrain = TerrainSimulation::new(terrain);

    assert_eq!(terrain.height(Vec3::new(7.0, 0.0, 2.0), false), Some(6.5));
}

#[test]
fn logical_fingerprint_is_independent_of_xsd_byte_order() {
    let mut little = terrain(8, 8, 2.0);
    set_height(&mut little, 3, 4, 11.5);
    let big = xsd::Terrain {
        byte_order: xsd::ByteOrder::Big,
        ..little.clone()
    };

    let little = TerrainSimulation::new(little);
    let big = TerrainSimulation::new(big);

    assert_eq!(little.fingerprint, big.fingerprint);
}

#[test]
fn xsd_surface_queries_use_retail_row_major_clamping() {
    let mut terrain = terrain(8, 8, 1.0);
    terrain.data_axis = 4;
    terrain.data_tile_scale = 2.0;
    terrain.tile_types = Some((0_u8..16).collect());
    let terrain = TerrainSimulation::new(terrain);

    assert_eq!(terrain.surface_type(Vec3::new(5.9, 20.0, 3.9)), Some(6));
    assert_eq!(terrain.surface_type(Vec3::new(-1.9, 0.0, -1.9)), Some(0));
    assert_eq!(terrain.surface_type(Vec3::new(500.0, 0.0, 500.0)), Some(15));
    assert_eq!(terrain.surface_type(Vec3::splat(f32::NAN)), None);
}

#[test]
fn xsd_surface_query_is_absent_without_tile_types() {
    let terrain = TerrainSimulation::new(terrain(8, 8, 1.0));

    assert_eq!(terrain.surface_type(Vec3::ZERO), None);
}

#[test]
fn projectile_segment_finds_the_retail_triangle_surface() {
    let mut terrain = terrain(8, 8, 2.0);
    set_height(&mut terrain, 1, 1, 2.0);
    let terrain = TerrainSimulation::new(terrain);

    let impact = terrain
        .projectile_segment_intersection(Vec3::new(2.0, 4.0, 2.0), Vec3::new(2.0, -1.0, 2.0))
        .expect("vertical segment should strike the raised vertex");
    assert!((impact.y - 2.0).abs() < 0.000_1);
}

#[test]
fn projectile_fast_check_matches_retail_clearance_threshold() {
    let terrain = TerrainSimulation::new(terrain(8, 8, 1.0));

    assert!(
        terrain
            .projectile_segment_intersection(Vec3::new(1.0, 0.5, 1.0), Vec3::new(2.0, 0.5, 1.0),)
            .is_none()
    );
    assert_eq!(
        terrain
            .projectile_segment_intersection(Vec3::new(1.0, 0.2, 1.0), Vec3::new(2.0, 0.1, 1.0),),
        Some(Vec3::new(2.0, 0.0, 1.0))
    );
}

fn terrain(height_axis: usize, cache_axis: usize, scale: f32) -> xsd::Terrain {
    xsd::Terrain {
        byte_order: xsd::ByteOrder::Little,
        data_axis: 8,
        data_tile_scale: 1.0,
        height_axis,
        cache_axis,
        height_tile_scale: scale,
        heights: vec![0.0; cache_axis * cache_axis],
        tile_types: None,
    }
}

fn set_height(terrain: &mut xsd::Terrain, x: usize, z: usize, height: f32) {
    let index = terrain.height_index(x, z).expect("test sample in grid");
    terrain.heights[index] = height;
}
