use super::*;
use crate::world::terrain::sampled_test_terrain;

const DT: f32 = 0.05;
const RAMP_SLOPE: f32 = 0.25;

fn ramp_world() -> World {
    let mut world = World::new();
    world.configure_terrain_simulation(sampled_test_terrain(8, 1.0, |x, _| RAMP_SLOPE * x));
    world
}

fn ramp_normal() -> Vec3 {
    Vec3::new(-RAMP_SLOPE, 1.0, 0.0).normalize()
}

#[test]
fn resting_unit_takes_the_terrain_height_and_stays_upright() {
    let mut world = ramp_world();
    let unit_id = world.create_unit_at(1, Vec3::new(20.0, 50.0, 30.0));

    world.update_entities(DT);

    let unit = world.get_unit(unit_id).unwrap();
    assert!((unit.base.position.y - RAMP_SLOPE * 20.0).abs() < 0.000_1);
    assert_eq!(unit.base.up, Vec3::Y);
}

#[test]
fn resting_vehicle_orients_with_the_terrain() {
    let mut world = ramp_world();
    let unit_id = world.create_unit_at(1, Vec3::new(20.0, 50.0, 30.0));
    world
        .get_unit_mut(unit_id)
        .unwrap()
        .configure_ground_tie(false, true, false);

    world.update_entities(DT);

    let unit = world.get_unit(unit_id).unwrap();
    assert!((unit.base.position.y - RAMP_SLOPE * 20.0).abs() < 0.000_1);
    assert!(unit.base.up.distance(ramp_normal()) < 0.000_1);
}

#[test]
fn filtered_orientation_snaps_from_upright_then_blends() {
    let mut world = ramp_world();
    let unit_id = world.create_unit_at(1, Vec3::new(20.0, 0.0, 30.0));
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.configure_ground_tie(false, true, false);
    unit.base.up = Vec3::new(0.0, 1.0, 0.2).normalize();
    let tilted = unit.base.up;

    world.update_entities(DT);

    let up = world.get_unit(unit_id).unwrap().base.up;
    assert!(up.angle_between(tilted) > 0.0);
    assert!(up.angle_between(tilted) < up.angle_between(ramp_normal()));
}

#[test]
fn unit_without_ground_tie_keeps_its_height() {
    let mut world = ramp_world();
    let unit_id = world.create_unit_at(1, Vec3::new(20.0, 50.0, 30.0));
    world
        .get_unit_mut(unit_id)
        .unwrap()
        .set_ties_to_ground(false);

    world.update_entities(DT);

    assert_eq!(world.get_unit(unit_id).unwrap().base.position.y, 50.0);
}
