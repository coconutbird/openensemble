//! Tests of the gravity ball pull action and of the action order.

use glam::Vec3;

use super::{BallMotion, GravityBallSpring, PhysicsAction};
use crate::entity_id::EntityId;
use crate::physics::test_support::{
    box_template, clamshell_template, hover_template, spawn_body_unit, world_with_flat_terrain,
};
use crate::world::World;

/// One Havok substep.
const SUBSTEP: f32 = 0.0125;

const SPRING: GravityBallSpring = GravityBallSpring {
    rest_length: 1.0,
    strength: 100.0,
    damping: 0.5,
    minimum_lateral_speed: 3.0,
};

/// A world with a box body of mass 10 high above the terrain and a ball unit at `ball`. The
/// body has no `CenterOffset`, so its centre of mass is the unit position.
fn pulled_box(body_at: Vec3, ball_at: Vec3) -> (World, EntityId, EntityId) {
    let mut world = world_with_flat_terrain();
    let template = box_template("pulled", Vec3::splat(0.5), 10.0, Vec3::ZERO);
    let unit_id = spawn_body_unit(&mut world, body_at, &template);
    let ball_id = world.create_unit_at(1, ball_at);
    (world, unit_id, ball_id)
}

fn apply_once(world: &mut World, unit_id: EntityId, ball: BallMotion, dt: f32) {
    let body = world.unit_body(unit_id).unwrap();
    let pull = world.unit_gravity_ball_pull(unit_id).unwrap().clone();
    pull.apply(world.physics.world_mut(), body, ball, dt);
}

#[test]
fn the_pull_cancels_gravity_pulls_on_extension_and_adds_the_slow_lateral_velocity() {
    let ball = Vec3::new(20.0, 40.0, 20.0);
    let (mut world, unit_id, ball_id) = pulled_box(ball + Vec3::X * 3.0, ball);
    assert!(world.add_gravity_ball_pull(unit_id, ball_id, SPRING));
    assert!(world.set_unit_body_linear_velocity(unit_id, Vec3::new(2.0, 0.0, 1.0)));

    apply_once(
        &mut world,
        unit_id,
        BallMotion {
            position: ball,
            velocity: Vec3::ZERO,
        },
        0.1,
    );

    // Anti-gravity adds 21.7 · 0.1 up before the action reads the body velocity (2, 2.17, 1).
    // Spring: F = (3 − 1) · 100 + 2 · 0.5 = 201, so −201 · 0.1 / 10 along +X.
    // Lateral: (0, 2.17, 1), slower than 3, is added again.
    let velocity = world.unit_body_linear_velocity(unit_id).unwrap();
    assert!(
        velocity.abs_diff_eq(Vec3::new(-0.01, 4.34, 2.0), 1.0e-4),
        "{velocity:?}"
    );
}

#[test]
fn a_body_inside_the_rest_length_only_gets_the_anti_gravity() {
    let ball = Vec3::new(20.0, 40.0, 20.0);
    let (mut world, unit_id, ball_id) = pulled_box(ball + Vec3::X * 0.5, ball);
    assert!(world.add_gravity_ball_pull(unit_id, ball_id, SPRING));
    assert!(world.set_unit_body_linear_velocity(unit_id, Vec3::Z));

    apply_once(
        &mut world,
        unit_id,
        BallMotion {
            position: ball,
            velocity: Vec3::new(50.0, 0.0, 0.0),
        },
        0.1,
    );

    let velocity = world.unit_body_linear_velocity(unit_id).unwrap();
    assert!(
        velocity.abs_diff_eq(Vec3::new(0.0, 2.17, 1.0), 1.0e-5),
        "{velocity:?}"
    );
}

#[test]
fn the_spring_damps_the_radial_speed_relative_to_the_ball_velocity() {
    let ball = Vec3::new(20.0, 40.0, 20.0);
    let (mut world, unit_id, ball_id) = pulled_box(ball + Vec3::X * 3.0, ball);
    let spring = GravityBallSpring {
        minimum_lateral_speed: 0.0,
        ..SPRING
    };
    assert!(world.add_gravity_ball_pull(unit_id, ball_id, spring));
    assert!(world.set_unit_body_linear_velocity(unit_id, Vec3::X * 2.0));

    apply_once(
        &mut world,
        unit_id,
        BallMotion {
            position: ball,
            velocity: Vec3::X * 2.0,
        },
        0.1,
    );

    // The relative radial speed is 0: F = 2 · 100 = 200, so −200 · 0.1 / 10 along +X.
    let velocity = world.unit_body_linear_velocity(unit_id).unwrap();
    assert!(
        velocity.abs_diff_eq(Vec3::new(0.0, 2.17, 0.0), 1.0e-4),
        "{velocity:?}"
    );
}

#[test]
fn a_nan_ball_position_or_velocity_is_read_as_zero() {
    let body_at = Vec3::new(3.0, 40.0, 0.0);
    let (mut nan_world, nan_unit, nan_ball) = pulled_box(body_at, Vec3::ZERO);
    let (mut zero_world, zero_unit, zero_ball) = pulled_box(body_at, Vec3::ZERO);
    for (world, unit, ball) in [
        (&mut nan_world, nan_unit, nan_ball),
        (&mut zero_world, zero_unit, zero_ball),
    ] {
        assert!(world.add_gravity_ball_pull(unit, ball, SPRING));
        assert!(world.set_unit_body_linear_velocity(unit, Vec3::new(1.0, 0.0, 2.0)));
    }

    apply_once(
        &mut nan_world,
        nan_unit,
        BallMotion {
            position: Vec3::NAN,
            velocity: Vec3::new(f32::INFINITY, 0.0, 0.0),
        },
        SUBSTEP,
    );
    apply_once(
        &mut zero_world,
        zero_unit,
        BallMotion {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
        },
        SUBSTEP,
    );

    let nan_velocity = nan_world.unit_body_linear_velocity(nan_unit).unwrap();
    assert!(nan_velocity.is_finite());
    assert_eq!(
        nan_velocity,
        zero_world.unit_body_linear_velocity(zero_unit).unwrap()
    );
}

#[test]
fn the_clamshell_bodies_get_the_anti_gravity_of_the_pelvis_mass_and_the_same_spring_force() {
    let mut world = world_with_flat_terrain();
    let template = clamshell_template("pulled", Vec3::splat(0.4), Vec3::ZERO);
    let unit_id = spawn_body_unit(&mut world, Vec3::new(20.0, 40.0, 20.0), &template);
    let ball_id = world.create_unit_at(1, Vec3::new(20.0, 40.0, 10.0));
    assert!(world.add_gravity_ball_pull(unit_id, ball_id, SPRING));
    let pull = world.unit_gravity_ball_pull(unit_id).unwrap().clone();
    let [upper, lower] = world
        .get_unit(unit_id)
        .and_then(|unit| unit.physics_object.as_ref())
        .and_then(crate::physics::UnitPhysics::clamshell_bodies)
        .unwrap();
    assert_eq!(pull.extra_bodies, vec![upper, lower]);
    let pelvis = world.unit_body(unit_id).unwrap();
    let pelvis_mass = world.physics.world().mass(pelvis).unwrap();
    let pelvis_center = world.physics.world().center_of_mass_world(pelvis).unwrap();
    let ball = BallMotion {
        position: Vec3::new(20.0, 40.0, 10.0),
        velocity: Vec3::ZERO,
    };
    let pull = super::GravityBallPull {
        spring: GravityBallSpring {
            minimum_lateral_speed: 0.0,
            ..SPRING
        },
        ..pull
    };
    pull.apply(world.physics.world_mut(), pelvis, ball, 0.1);

    let physics = world.physics.world();
    let anti_gravity = Vec3::Y * (21.7 * pelvis_mass * 0.1);
    let length = pelvis_center.distance(ball.position);
    let direction = (pelvis_center - ball.position) / length;
    let radial_speed = (anti_gravity / pelvis_mass).dot(direction);
    let force = (length - 1.0) * 100.0 + radial_speed * 0.5;
    let spring_impulse = direction * (-force * 0.1);
    for body in [upper, lower] {
        let mass = physics.mass(body).unwrap();
        let expected = (anti_gravity + spring_impulse) / mass;
        let velocity = physics.linear_velocity(body).unwrap();
        assert!(
            velocity.abs_diff_eq(expected, 1.0e-3),
            "{velocity:?} {expected:?}"
        );
        assert!(
            physics
                .angular_velocity(body)
                .unwrap()
                .abs_diff_eq(Vec3::ZERO, 1.0e-4)
        );
    }
}

#[test]
fn the_actions_run_in_creation_order_and_only_on_active_bodies() {
    let mut world = world_with_flat_terrain();
    let hornet = spawn_body_unit(
        &mut world,
        Vec3::new(10.0, 30.0, 10.0),
        &hover_template("hornet", Vec3::new(2.0, 1.0, 3.0)),
    );
    let crate_template = box_template("crate", Vec3::splat(0.5), 10.0, Vec3::ZERO);
    let first = spawn_body_unit(&mut world, Vec3::new(20.0, 30.0, 20.0), &crate_template);
    let second = spawn_body_unit(&mut world, Vec3::new(25.0, 30.0, 20.0), &crate_template);
    let ball = world.create_unit_at(1, Vec3::new(22.0, 30.0, 20.0));
    assert!(world.add_gravity_ball_pull(second, ball, SPRING));
    assert!(world.add_gravity_ball_pull(first, ball, SPRING));
    assert!(world.add_gravity_ball_pull(hornet, ball, SPRING));

    assert_eq!(
        world.active_physics_actions(),
        vec![
            PhysicsAction::Vehicle(hornet),
            PhysicsAction::GravityBall(second),
            PhysicsAction::GravityBall(first),
            PhysicsAction::GravityBall(hornet),
        ]
    );

    let body = world.unit_body(second).unwrap();
    world.physics.world_mut().deactivate(body);
    assert!(world.remove_gravity_ball_pull(first));
    assert!(!world.remove_gravity_ball_pull(first));
    assert_eq!(
        world.active_physics_actions(),
        vec![
            PhysicsAction::Vehicle(hornet),
            PhysicsAction::GravityBall(hornet),
        ]
    );
}

#[test]
fn each_substep_cancels_gravity_so_a_pulled_body_at_the_ball_hovers() {
    let ball_at = Vec3::new(20.0, 40.0, 20.0);
    let (mut world, unit_id, ball_id) = pulled_box(ball_at, ball_at);
    assert!(world.add_gravity_ball_pull(unit_id, ball_id, SPRING));
    let (mut free_world, free_unit, _) = pulled_box(ball_at, ball_at);

    for _ in 0..10 {
        world.step_physics(0.05);
        free_world.step_physics(0.05);
    }

    let velocity = world.unit_body_linear_velocity(unit_id).unwrap();
    let position = world.unit_body_position(unit_id).unwrap();
    assert!(velocity.abs_diff_eq(Vec3::ZERO, 1.0e-3), "{velocity:?}");
    assert!(position.abs_diff_eq(ball_at, 1.0e-3), "{position:?}");
    assert!(free_world.unit_body_position(free_unit).unwrap().y < ball_at.y - 1.0);
}

#[test]
fn a_fixed_body_gets_no_pull_action() {
    let mut world = world_with_flat_terrain();
    let template = crate::physics::test_support::fixed_box_template("wall", Vec3::ONE);
    let wall = spawn_body_unit(&mut world, Vec3::new(20.0, 2.0, 20.0), &template);
    let ball = world.create_unit_at(1, Vec3::new(20.0, 30.0, 20.0));
    assert!(!world.add_gravity_ball_pull(wall, ball, SPRING));
    assert!(world.unit_gravity_ball_pull(wall).is_none());
}
