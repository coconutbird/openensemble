//! The ground family: teleport, yaw match, suspension and the terrain fix.

use glam::Vec3;
use pipeline::database::hw1::Vector3;
use pipeline::database::hw1::physics::{PhysicsVehicleType, VehicleInfo};

use super::{Scene, TICK, squad_at, up_of, yaw_of};
use crate::physics::vehicle_actions::input::VehicleActionInput;

/// The `<info>` of `scorpion.physics` (root.era).
pub(super) fn scorpion_info() -> VehicleInfo {
    let point = |x, z| Vector3 { x, y: -0.05, z };
    VehicleInfo {
        hardpoints: vec![
            point(-3.0, 3.0),
            point(3.0, 3.0),
            point(-3.0, -6.0),
            point(3.0, -6.0),
        ],
        spring_length: 1.3,
        wheel_radius: 1.0,
        spring_strength: 50.0,
        damping_compression: 2.0,
        damping_relaxation: 2.0,
        normal_clipping_angle: 0.1,
        chassis_inertia_yaw: 3.0,
        chassis_inertia_pitch: 3.0,
        chassis_inertia_roll: 2.4,
        ..VehicleInfo::default()
    }
}

/// The body height above flat terrain where the four springs carry three times the gravity:
/// `k · (L − len) = 3 · 21.7 / 4`, and the hardpoints sit 0.05 below the body origin.
fn scorpion_rest_height() -> f32 {
    let length = 1.3 - 3.0 * 21.7 / 4.0 / 50.0;
    length + 1.0 + 0.05
}

fn scorpion(position: Vec3, yaw: f32) -> Scene {
    Scene::new(
        PhysicsVehicleType::Scorpion,
        Some(&scorpion_info()),
        position,
        yaw,
        |_, _| 10.0,
    )
}

#[test]
fn the_teleport_impulse_removes_one_sixth_of_the_error_each_substep() {
    let mut scene = scorpion(Vec3::new(60.0, 10.0, 60.0), 0.0);
    let body = scene.position();
    let target = Vec3::new(body.x + 3.0, 0.0, body.z - 1.5);
    let input = VehicleActionInput {
        unit: scene.unit(),
        squad: Some(squad_at(target, Vec3::Z, None)),
        air: None,
    };
    let dt = 0.0125;
    scene
        .action
        .apply(scene.physics.world_mut(), &input, &mut scene.flags, dt);
    // The body is at rest, so the impulse is `m / dt / 6 · error` in the plane.
    let velocity = scene.velocity();
    let expected = (target - body) / (6.0 * dt);
    assert!((velocity.x - expected.x).abs() < 1e-3, "{velocity}");
    assert!((velocity.z - expected.z).abs() < 1e-3, "{velocity}");
}

#[test]
fn the_ground_family_follows_a_moving_squad_and_stays_level() {
    let start = Vec3::new(40.0, 10.0, 20.0);
    let mut scene = scorpion(start, 0.0);
    let speed = 8.0;
    let mut squad_z = start.z;
    for _ in 0..120 {
        squad_z += speed * TICK;
        let target = Vec3::new(start.x, 10.0, squad_z);
        scene.tick(|unit| VehicleActionInput {
            unit,
            squad: Some(squad_at(target, Vec3::Z, None)),
            air: None,
        });
    }
    let position = scene.position();
    // Each substep removes one sixth of the error and the point jumps `v · tick` each tick.
    // In the steady state the error after a tick is `jump · f / (1 − f)`, `f = (5/6)⁴`.
    let sixth = 5.0_f32 / 6.0;
    let remaining = sixth * sixth * sixth * sixth;
    let expected_lag = speed * TICK * remaining / (1.0 - remaining);
    let lag = squad_z - position.z;
    assert!(
        (lag - expected_lag).abs() < 0.02,
        "lag {lag}, expected {expected_lag}"
    );
    assert!((position.x - start.x).abs() < 0.05, "{position}");
    let rotation = scene.world().rotation(scene.body).expect("body");
    assert!(up_of(rotation).y > 0.999, "{rotation}");
    let height = position.y - 10.0;
    assert!(
        (height - scorpion_rest_height()).abs() < 0.05,
        "height {height}, rest {}",
        scorpion_rest_height()
    );
    assert!(!scene.action.in_air());
}

#[test]
fn the_yaw_match_turns_the_body_to_the_turn_radius_forward_axis() {
    let start = Vec3::new(60.0, 10.0, 60.0);
    let mut scene = scorpion(start, 0.0);
    let forward = Vec3::X;
    let target = Vec3::new(start.x, 10.0, start.z);
    for _ in 0..20 {
        scene.tick(|unit| VehicleActionInput {
            unit,
            squad: Some(squad_at(target, forward, None)),
            air: None,
        });
    }
    let yaw = yaw_of(scene.rotation().z_axis);
    assert!(
        (yaw - std::f32::consts::FRAC_PI_2).abs() < 0.01,
        "yaw {yaw}"
    );
    assert!(scene.rotation().y_axis.y > 0.999);
}

#[test]
fn a_hardpoint_below_the_terrain_queues_the_terrain_fix_that_lifts_the_body() {
    let start = Vec3::new(60.0, 10.0, 60.0);
    let mut scene = scorpion(start, 0.0);
    // Put the body so low that the hardpoints are under the terrain.
    let low = Vec3::new(scene.position().x, 9.5, scene.position().z);
    scene.physics.world_mut().set_position(scene.body, low);
    scene
        .physics
        .world_mut()
        .set_linear_velocity(scene.body, Vec3::new(0.0, -2.0, 0.0));
    let input = VehicleActionInput {
        unit: scene.unit(),
        squad: Some(squad_at(low, Vec3::Z, None)),
        air: None,
    };
    scene
        .action
        .apply(scene.physics.world_mut(), &input, &mut scene.flags, 0.0125);
    assert!(scene.action.needs_terrain_resolution());
    let before = scene.position().y;
    scene
        .action
        .resolve_terrain_penetration(scene.physics.world_mut(), 0.5);
    assert!(!scene.action.needs_terrain_resolution());
    // The hardpoints were about 0.55 below the terrain after the substep; the fix lifts by
    // that depth plus the wheel radius.
    let lift = scene.position().y - before;
    assert!(lift > 1.5 && lift < 1.7, "lift {lift}");
    assert!(scene.velocity().y >= 0.0);
}
