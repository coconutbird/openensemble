//! The ray-cast suspension that the ground, warthog and ghost actions share (the first part of
//! each `applyAction`).

use glam::Vec3;
use physics::{BodyId, World as PhysicsWorld};

use super::frame::BodyFrame;

/// The suspension constants of one vehicle (`BPhysicsVehicleInfo` or the hardcoded values).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Suspension {
    /// `springLength`: the rest length of each spring.
    pub(super) spring_length: f32,
    /// `wheelRadius`.
    pub(super) wheel_radius: f32,
    /// `springStrength`, per unit of mass.
    pub(super) spring_strength: f32,
    /// `dampingCompression`.
    pub(super) damping_compression: f32,
    /// `dampingRelaxation`.
    pub(super) damping_relaxation: f32,
    /// `normalClippingAngle`.
    pub(super) normal_clipping_angle: f32,
}

/// The result of the ray of one wheel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Wheel {
    /// `true` if the ray hit the terrain.
    pub(super) hit: bool,
    /// The hardpoint in world space. The suspension impulse acts here.
    pub(super) hardpoint: Vec3,
    /// The contact normal, or minus the suspension direction for a miss.
    pub(super) normal: Vec3,
    /// The suspension force per unit of mass. It is 0 for a miss.
    pub(super) force: f32,
}

/// Casts the wheel rays against the terrain heightfield only and computes the spring forces.
///
/// Each ray goes from the hardpoint along the body down axis for `L + R`. A hit gives
/// `F = k · (L − (d − R)) · c − damping · v`, where `c` is `−1 / (n · down)` (or
/// `1 / clip` if the normal is too steep, with no damping). Returns `true` if any ray hit.
pub(super) fn cast_wheels(
    physics: &PhysicsWorld,
    body: BodyId,
    frame: &BodyFrame,
    hardpoints: &[Vec3],
    suspension: &Suspension,
    wheels: &mut [Wheel],
) -> bool {
    let down = -frame.up();
    let ray_length = suspension.spring_length + suspension.wheel_radius;
    let mut any_hit = false;
    for (wheel, &local) in wheels.iter_mut().zip(hardpoints) {
        let hardpoint = frame.transform_point(local);
        let end = hardpoint + down * ray_length;
        *wheel = match physics.ray_cast_heightfield(hardpoint, end) {
            Some(hit) => {
                any_hit = true;
                let hit_distance = hit.fraction * ray_length;
                let length = hit_distance - suspension.wheel_radius;
                let contact = hardpoint + down * hit_distance;
                let point_velocity = physics.point_velocity(body, contact).unwrap_or(Vec3::ZERO);
                let projected_velocity = hit.normal.dot(point_velocity);
                let denominator = hit.normal.dot(down);
                let (relative_velocity, inverse) =
                    if denominator >= -suspension.normal_clipping_angle {
                        (0.0, 1.0 / suspension.normal_clipping_angle)
                    } else {
                        let inverse = -1.0 / denominator;
                        (projected_velocity * inverse, inverse)
                    };
                let mut force =
                    suspension.spring_strength * (suspension.spring_length - length) * inverse;
                if relative_velocity < 0.0 {
                    force -= suspension.damping_compression * relative_velocity;
                } else {
                    force -= suspension.damping_relaxation * relative_velocity;
                }
                Wheel {
                    hit: true,
                    hardpoint,
                    normal: hit.normal,
                    force,
                }
            }
            None => Wheel {
                hit: false,
                hardpoint,
                normal: -down,
                force: 0.0,
            },
        };
    }
    any_hit
}

/// Adds the extra gravity (`applyForce(dt, m · factor · g)`) and then the suspension impulses
/// (`applyPointImpulse(dt · m · F · n, hardpoint)` for each wheel with `F > 0`).
pub(super) fn apply_gravity_and_springs(
    physics: &mut PhysicsWorld,
    body: BodyId,
    dt: f32,
    extra_gravity_factor: f32,
    wheels: &[Wheel],
) {
    let mass = physics.mass(body).unwrap_or(0.0);
    let gravity = physics.gravity();
    physics.apply_force(body, dt, gravity * (mass * extra_gravity_factor));
    for wheel in wheels {
        if wheel.force > 0.0 {
            let impulse = wheel.normal * (dt * mass * wheel.force);
            physics.apply_point_impulse(body, impulse, wheel.hardpoint);
        }
    }
}
