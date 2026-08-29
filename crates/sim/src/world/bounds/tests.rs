use super::*;

#[test]
fn playable_bounds_sort_and_clamp_to_loaded_terrain() {
    let mut world = World::new();
    assert!(world.configure_terrain_bounds(Vec3::ZERO, Vec3::new(100.0, 8.0, 80.0)));
    assert!(
        world.set_playable_bounds(Vec3::new(120.0, -50.0, 70.0), Vec3::new(-10.0, 500.0, 20.0),)
    );

    let bounds = world.playable_bounds().expect("narrowed bounds");
    assert_close(bounds.min_x(), 0.0);
    assert_close(bounds.min_z(), 20.0);
    assert_close(bounds.max_x(), 100.0);
    assert_close(bounds.max_z(), 70.0);
    assert!(!world.is_outside_playable_bounds(Vec3::new(50.0, 999.0, 20.0), false));
    assert!(world.is_outside_playable_bounds(Vec3::new(50.0, 0.0, 19.0), false));
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < f32::EPSILON);
}

#[test]
fn full_bounds_disable_subset_but_forced_queries_still_use_terrain() {
    let mut world = World::new();
    assert!(world.configure_terrain_bounds(Vec3::ZERO, Vec3::new(64.0, 4.0, 64.0)));
    assert!(world.set_playable_bounds(Vec3::new(64.0, 50.0, 64.0), Vec3::new(0.0, -50.0, 0.0),));

    assert_eq!(world.playable_bounds(), None);
    assert_eq!(world.effective_playable_bounds(), world.terrain_bounds());
    assert!(!world.is_outside_playable_bounds(Vec3::new(65.0, 0.0, 2.0), false));
    assert!(world.is_outside_playable_bounds(Vec3::new(65.0, 0.0, 2.0), true));
}

#[test]
fn invalid_bounds_are_atomic_and_authoritative_state_changes_checksum() {
    let mut world = World::new();
    let before = world.checksum();
    assert!(!world.set_playable_bounds(Vec3::NAN, Vec3::ZERO));
    assert_eq!(world.checksum(), before);

    assert!(world.set_playable_bounds(Vec3::ZERO, Vec3::splat(10.0)));
    assert_ne!(world.checksum(), before);
}
