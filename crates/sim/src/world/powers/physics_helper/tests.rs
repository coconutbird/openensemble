//! Tests of the `BPowerHelper` aircraft impulses and of the nudge direction.

use glam::Vec3;

use super::{
    AirImpulse, impulse_physics_air_unit, impulse_physics_air_units, is_physics_aircraft,
    nudge_direction,
};
use crate::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::physics::test_support::{box_template, hover_template, world_with_flat_terrain};
use crate::world::World;

const POSITION: Vec3 = Vec3::new(10.0, 30.0, 10.0);

/// A world with one flying hornet of mass 100 at [`POSITION`] with 50 of 100 hit points, and
/// the gameplay that gives its proto the hornet `PhysicsInfo`.
fn hornet_world() -> (World, GameplayCatalog, EntityId) {
    let mut world = world_with_flat_terrain();
    let template = hover_template("hornet", Vec3::new(2.0, 1.0, 3.0));
    let unit_id = world.create_unit_at(1, POSITION);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_name = "unsc_air_hornet".to_owned();
    unit.flying = true;
    unit.max_hitpoints = 100.0;
    unit.hitpoints = 50.0;
    unit.obstruction_half_extents = Vec3::new(2.0, 1.0, 3.0);
    assert!(world.create_physics_object(unit_id, &template, false));
    let mut gameplay = GameplayCatalog::default();
    gameplay.insert_test_physics_info("unsc_air_hornet", template);
    (world, gameplay, unit_id)
}

#[test]
fn a_physics_aircraft_needs_an_aircraft_physics_info_air_movement_and_a_body() {
    let (mut world, gameplay, unit_id) = hornet_world();
    assert!(is_physics_aircraft(&world, Some(&gameplay), unit_id));
    assert!(!is_physics_aircraft(&world, None, unit_id));

    let mut ground = GameplayCatalog::default();
    ground.insert_test_physics_info(
        "unsc_air_hornet",
        box_template("crate", Vec3::ONE, 10.0, Vec3::ZERO),
    );
    assert!(!is_physics_aircraft(&world, Some(&ground), unit_id));

    world.get_unit_mut(unit_id).unwrap().flying = false;
    assert!(!is_physics_aircraft(&world, Some(&gameplay), unit_id));
}

#[test]
fn the_air_impulse_pushes_toward_the_target_and_down_with_health_and_falloff() {
    let (mut world, gameplay, unit_id) = hornet_world();
    let applied = impulse_physics_air_unit(
        &mut world,
        Some(&gameplay),
        unit_id,
        AirImpulse {
            scale: 1.0,
            target: Some(POSITION + Vec3::new(4.0, 7.0, 0.0)),
            radius: 10.0,
        },
    );
    assert!(applied);

    // (2 − 0.5) · 150 = 225; falloff 0.25 · (1 − 4 · 0.5 / 10) + 0.75 = 0.95.
    let linear = Vec3::new(1.0, -1.0, 0.0) * 213.75;
    // `applyImpulse(J)` plus `applyPointImpulse(2.5·J)` on a body of mass 100.
    let expected = linear * 3.5 / 100.0;
    let velocity = world.unit_body_linear_velocity(unit_id).unwrap();
    assert!(velocity.abs_diff_eq(expected, 1.0e-4), "{velocity:?}");
    let spin = world.unit_body_angular_velocity(unit_id).unwrap();
    assert!(
        spin.z < 0.0,
        "the point impulse at position + dir turns the body: {spin:?}"
    );
}

#[test]
fn without_a_target_the_air_impulse_aims_at_a_random_point_below_the_aircraft() {
    let (mut world, gameplay, unit_id) = hornet_world();
    let (mut reference, _, _) = hornet_world();
    let x = reference.trigger_random_float(-0.5, 0.5) * 2.0;
    let z = reference.trigger_random_float(-0.5, 0.5) * 3.0;

    assert!(impulse_physics_air_unit(
        &mut world,
        Some(&gameplay),
        unit_id,
        AirImpulse {
            scale: 0.5,
            target: None,
            radius: 0.0,
        },
    ));

    let direction = Vec3::new(x, 0.0, z).normalize();
    let expected = (direction - Vec3::Y) * (1.5 * 150.0 * 0.5) * 3.5 / 100.0;
    let velocity = world.unit_body_linear_velocity(unit_id).unwrap();
    assert!(velocity.abs_diff_eq(expected, 1.0e-4), "{velocity:?}");
    assert_eq!(
        world.trigger_random_index(32_767),
        reference.trigger_random_index(32_767)
    );
}

#[test]
fn a_target_straight_above_gives_no_impulse() {
    let (mut world, gameplay, unit_id) = hornet_world();
    assert!(impulse_physics_air_unit(
        &mut world,
        Some(&gameplay),
        unit_id,
        AirImpulse {
            scale: 1.0,
            target: Some(POSITION + Vec3::Y * 5.0),
            radius: 10.0,
        },
    ));
    assert_eq!(world.unit_body_linear_velocity(unit_id), Some(Vec3::ZERO));
    assert_eq!(world.unit_body_angular_velocity(unit_id), Some(Vec3::ZERO));
}

/// [`hornet_world`] with the hornet in a squad at [`POSITION`] and a fixed random seed.
fn hornet_squad_world() -> (World, GameplayCatalog, EntityId) {
    let (mut world, gameplay, unit_id) = hornet_world();
    let squad_id = world.create_squad(1);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    world.get_squad_mut(squad_id).unwrap().base.position = POSITION;
    world.sim_random_mut().set_seed(12_345);
    (world, gameplay, unit_id)
}

#[test]
fn the_area_impulse_rolls_once_per_unit_against_the_percent_chance() {
    let (mut reference, _, _) = hornet_squad_world();
    let roll = reference.trigger_random_index(100);
    assert!(roll > 0);
    let target = POSITION + Vec3::X * 4.0;

    let (mut world, gameplay, unit_id) = hornet_squad_world();
    impulse_physics_air_units(&mut world, Some(&gameplay), target, 10.0, roll - 1);
    assert_eq!(world.unit_body_linear_velocity(unit_id), Some(Vec3::ZERO));
    assert_eq!(
        world.trigger_random_index(32_767),
        reference.trigger_random_index(32_767)
    );

    let (mut world, gameplay, unit_id) = hornet_squad_world();
    impulse_physics_air_units(&mut world, Some(&gameplay), target, 10.0, roll);
    assert!(world.unit_body_linear_velocity(unit_id).unwrap().x > 0.0);
}

#[test]
fn the_nudge_direction_is_the_flat_unit_direction_with_one_up() {
    let direction = nudge_direction(Vec3::new(3.0, 5.0, 4.0).normalize());
    assert!(direction.abs_diff_eq(Vec3::new(0.6, 1.0, 0.8), 1.0e-6));
    assert!(!nudge_direction(Vec3::Y).is_finite());
}
