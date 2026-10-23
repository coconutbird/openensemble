use super::*;
use crate::entities::units::warthog::WarthogUnitSpec;
use crate::world::terrain::sampled_test_terrain;

const DT: f32 = 0.05;
const HILL_START: f32 = 30.0;
const HILL_LENGTH: f32 = 60.0;
const HILL_HEIGHT: f32 = 8.0;
const RAMP_SLOPE: f32 = 0.25;

/// A smooth hill across the X axis between `HILL_START` and `HILL_START + HILL_LENGTH`.
fn hill(x: f32, _z: f32) -> f32 {
    let fraction = (x - HILL_START) / HILL_LENGTH;
    if (0.0..=1.0).contains(&fraction) {
        HILL_HEIGHT * (std::f32::consts::PI * fraction).sin().powi(2)
    } else {
        0.0
    }
}

fn hill_world() -> World {
    let mut world = World::new();
    world.configure_terrain_simulation(sampled_test_terrain(16, 1.0, hill));
    world
}

fn ramp_world() -> World {
    let mut world = World::new();
    world.configure_terrain_simulation(sampled_test_terrain(8, 1.0, |x, _| RAMP_SLOPE * x));
    world
}

fn ramp_normal() -> Vec3 {
    Vec3::new(-RAMP_SLOPE, 1.0, 0.0).normalize()
}

fn spawn_warthog(world: &mut World, position: Vec3) -> EntityId {
    let unit_id = world.create_unit_at(1, position);
    let unit = world.get_unit_mut(unit_id).expect("the new Warthog");
    unit.physics = Some(WarthogUnitSpec::default().physics_body(position.y));
    unit_id
}

#[test]
fn physics_vehicle_climbs_and_descends_the_terrain() {
    let mut world = hill_world();
    let start = Vec3::new(10.0, 0.0, 64.0);
    let unit_id = spawn_warthog(&mut world, start);
    let target = Vec3::new(110.0, 0.0, 64.0);
    assert!(world.get_unit_mut(unit_id).unwrap().move_to(target));

    let mut highest = f32::MIN;
    let mut tilted_up_hill = false;
    for _ in 0..200 {
        world.update_entities(DT);
        let unit = world.get_unit(unit_id).expect("the Warthog stays alive");
        let position = unit.base.position;
        let terrain = world.terrain_height(position, true).unwrap();
        assert!(
            (position.y - terrain).abs() < 0.3,
            "the body left the terrain at {position:?} (terrain {terrain})"
        );
        highest = highest.max(position.y);
        let climbing = position.x > HILL_START + 5.0 && position.x < HILL_START + 25.0;
        tilted_up_hill |= climbing && unit.base.up.x < -0.1;
    }

    let unit = world.get_unit(unit_id).unwrap();
    assert!(highest > HILL_HEIGHT - 0.5, "highest point {highest}");
    assert!(tilted_up_hill, "the body never tilted with the slope");
    assert!((unit.base.position.x - target.x).abs() < 1.0);
    assert!(unit.base.position.y.abs() < 0.05);
    assert!(unit.base.up.distance(Vec3::Y) < 0.001);
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
