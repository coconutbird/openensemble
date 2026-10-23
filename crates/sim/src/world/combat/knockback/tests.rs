use super::*;
use crate::physics::test_support::box_template;

const MASS: f32 = 10.0;

#[test]
fn knockback_draws_angle_then_force_and_scales_by_mass() {
    let (mut world, attacker, target) = two_units(Vec3::new(3.0, -4.0, 0.0));
    let profile = KnockbackProfile {
        launch_angle_min: 10.0,
        launch_angle_max: 50.0,
        force_min: 1.0,
        force_max: 3.0,
    };
    let mut stream = world.sim_rng;
    let degrees = stream.range_float(10.0, 50.0);
    let force = stream.range_float(1.0, 3.0);

    world.do_knockback_attack(attacker, target, profile);

    // The target is below the attacker, but the impulse goes up (`y = |y|`).
    let scale = crate::math::cos(degrees * RADIANS_PER_DEGREE) * force;
    let expected = Vec3::new(0.6, 0.8, 0.0) * scale;
    assert_vec_close(world.unit_body_linear_velocity(target).unwrap(), expected);
    assert_eq!(world.sim_rng, stream);
}

#[test]
fn knockback_upward_impulse_drops_with_the_body_upward_speed() {
    let offset = Vec3::new(0.0, 1.0, 0.0);
    assert_vec_close(knockback_impulse(offset, 0.0, 2.0, 3.0), Vec3::Y * 2.0);
    assert_vec_close(knockback_impulse(offset, 0.0, 2.0, 4.5), Vec3::Y);
    assert_vec_close(knockback_impulse(offset, 0.0, 2.0, 6.5), Vec3::ZERO);
}

#[test]
fn attached_target_gets_no_knockback() {
    let (mut world, attacker, target) = two_units(Vec3::X);
    world
        .get_unit_mut(target)
        .unwrap()
        .object_state
        .set_attached_to(Some(attacker));
    let stream = world.sim_rng;
    let profile = KnockbackProfile {
        force_min: 5.0,
        force_max: 5.0,
        ..KnockbackProfile::default()
    };

    world.do_knockback_attack(attacker, target, profile);

    assert_eq!(world.unit_body_linear_velocity(target), Some(Vec3::ZERO));
    assert_eq!(world.sim_rng, stream);
}

fn two_units(target_offset: Vec3) -> (World, EntityId, EntityId) {
    let mut world = World::with_seed(41);
    world.init_players(2);
    let attacker = world.create_unit_at(1, Vec3::new(0.0, 10.0, 0.0));
    let target = world.create_unit_at(2, Vec3::new(0.0, 10.0, 0.0) + target_offset);
    let template = box_template("body", Vec3::ONE, MASS, Vec3::ZERO);
    assert!(world.create_physics_object(target, &template, false));
    (world, attacker, target)
}

fn assert_vec_close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, 0.000_1),
        "{actual:?} != {expected:?}"
    );
}
