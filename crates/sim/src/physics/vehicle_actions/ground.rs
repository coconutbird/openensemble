//! `BPhysicsGroundVehicleAction` (`applyAction` `0x1402CF050`): the scorpion, grizzly,
//! elephant, rhino, reactor, cobra, gremlin, wolverine and ground (scarab) vehicles.
//!
//! The family has no drive forces. The body follows the turn-radius position and forward axis
//! of its squad through a teleport impulse and a yaw-match impulse. Its own suspension holds it
//! above the terrain, and the filter turns off its Havok contacts with the terrain; the action
//! queues a terrain fix when a hardpoint will go below the terrain.

use glam::{Mat3, Quat, Vec3};
use physics::{BodyId, World as PhysicsWorld};
use pipeline::database::hw1::physics::VehicleInfo;

use super::frame::{BodyFrame, hk_acos, world_inertia};
use super::input::SquadInput;
use super::reorient::reorient;
use super::suspension::{Suspension, Wheel, apply_gravity_and_springs, cast_wheels};
use crate::sync::SyncChecksum;

/// `cExtraGravityFactor`: the family feels three times the world gravity.
const EXTRA_GRAVITY_FACTOR: f32 = 2.0;
/// `cTeleportImpulseFactor`: the part of the position error that one substep removes.
const TELEPORT_IMPULSE_FACTOR: f32 = 0.166_666_66;
/// Below this yaw-match angle the desired angular velocity is 0.
const YAW_MATCH_MIN_ANGLE: f32 = 0.001;
/// `cReorientStrength` before the inertia scale.
const REORIENT_STRENGTH: f32 = 0.75;
/// `cReorientDamping` before the inertia scale.
const REORIENT_DAMPING: f32 = 0.1;
/// `cInertiaMultiplier`: `1 / (150 · 0.4)`, the warthog mass and roll inertia factor.
const INERTIA_MULTIPLIER: f32 = 1.0 / 60.0;
/// The reorient axis if the body up axis is parallel to world up.
const REORIENT_FALLBACK_AXIS: Vec3 = Vec3::Z;
/// The squared cross product length below which the reorient axis falls back.
const REORIENT_AXIS_EPSILON: f32 = 0.000_001;

/// The derived actions of the family. They differ only in visual updates, which the sim does
/// not model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroundVariant {
    /// `BPhysicsGroundVehicleAction` itself (the `ground` type).
    Base,
    /// `BPhysicsScorpionAction` (scorpion, grizzly, elephant, rhino and reactor).
    Scorpion,
    /// `BPhysicsCobraAction` (cobra and gremlin).
    Cobra,
    /// `BPhysicsWolverineAction`.
    Wolverine,
}

/// The state of one ground-family action.
#[derive(Clone, Debug)]
pub(crate) struct GroundAction {
    variant: GroundVariant,
    hardpoints: Vec<Vec3>,
    suspension: Suspension,
    chassis_inertia_roll: f32,
    /// `mPrevLinearVelocity`.
    previous_linear_velocity: Vec3,
    /// `mDistBelowTerrain`.
    distance_below_terrain: f32,
    /// `mTerrainCollision`: the action waits for the terrain fix.
    terrain_collision: bool,
    /// The wheel results of the last substep (scratch space).
    wheels: Vec<Wheel>,
}

impl GroundAction {
    /// `BPhysicsGroundVehicleAction` constructor and `setVehicleInfo` (`0x1402CEEF0`): the
    /// inertia becomes `diag(pitch · m, yaw · m, roll · m)` from the data.
    pub(crate) fn new(
        physics: &mut PhysicsWorld,
        body: BodyId,
        variant: GroundVariant,
        info: &VehicleInfo,
    ) -> Self {
        let mass = physics.mass(body).unwrap_or(1.0);
        physics.set_inertia_local(
            body,
            Vec3::new(
                info.chassis_inertia_pitch * mass,
                info.chassis_inertia_yaw * mass,
                info.chassis_inertia_roll * mass,
            ),
        );
        let hardpoints: Vec<Vec3> = info
            .hardpoints
            .iter()
            .map(|point| Vec3::new(point.x, point.y, point.z))
            .collect();
        Self {
            variant,
            wheels: vec![Wheel::default(); hardpoints.len()],
            hardpoints,
            suspension: Suspension {
                spring_length: info.spring_length,
                wheel_radius: info.wheel_radius,
                spring_strength: info.spring_strength,
                damping_compression: info.damping_compression,
                damping_relaxation: info.damping_relaxation,
                normal_clipping_angle: info.normal_clipping_angle,
            },
            chassis_inertia_roll: info.chassis_inertia_roll,
            previous_linear_velocity: Vec3::ZERO,
            distance_below_terrain: 0.0,
            terrain_collision: false,
        }
    }

    /// The derived action.
    pub(crate) fn variant(&self) -> GroundVariant {
        self.variant
    }

    /// `true` if no wheel ray hit the terrain in the last substep.
    pub(crate) fn in_air(&self) -> bool {
        !self.wheels.iter().any(|wheel| wheel.hit)
    }

    /// `true` while the action waits for its terrain fix.
    pub(crate) fn needs_terrain_resolution(&self) -> bool {
        self.terrain_collision
    }

    /// One substep of `applyAction`.
    pub(crate) fn apply(
        &mut self,
        physics: &mut PhysicsWorld,
        body: BodyId,
        squad: Option<&SquadInput>,
        dt: f32,
    ) {
        let Some(frame) = BodyFrame::read(physics, body) else {
            return;
        };
        self.previous_linear_velocity = frame.linear_velocity;
        cast_wheels(
            physics,
            body,
            &frame,
            &self.hardpoints,
            &self.suspension,
            &mut self.wheels,
        );
        apply_gravity_and_springs(physics, body, dt, EXTRA_GRAVITY_FACTOR, &self.wheels);
        physics.set_damping(body, 0.0, 0.0);
        apply_teleport_impulse(physics, body, squad, dt);
        if let Some(squad) = squad {
            apply_yaw_match(physics, body, squad.turn_radius_forward, dt);
        }
        self.reorient(physics, body, dt);
        self.check_terrain_penetration(physics, body, dt);
    }

    /// Turns the body toward world up about `normalize(up × Y)`, with the warthog constants
    /// scaled by `m · roll / 60`.
    fn reorient(&self, physics: &mut PhysicsWorld, body: BodyId, dt: f32) {
        let Some(frame) = BodyFrame::read(physics, body) else {
            return;
        };
        let cross = frame.up().cross(Vec3::Y);
        let axis = if cross.length_squared() > REORIENT_AXIS_EPSILON {
            cross.normalize()
        } else {
            REORIENT_FALLBACK_AXIS
        };
        let body_axis = frame.matrix.transpose() * axis;
        let inertia_factor = frame.mass * self.chassis_inertia_roll * INERTIA_MULTIPLIER;
        reorient(
            physics,
            body,
            dt,
            body_axis,
            REORIENT_STRENGTH * inertia_factor,
            REORIENT_DAMPING * inertia_factor,
        );
    }

    /// Finds the largest depth of a hardpoint below the terrain after one more substep at the
    /// current velocity, and queues the terrain fix once.
    fn check_terrain_penetration(&mut self, physics: &PhysicsWorld, body: BodyId, dt: f32) {
        let (Some(frame), Some(field)) = (BodyFrame::read(physics, body), physics.heightfield())
        else {
            return;
        };
        let mut below = false;
        let mut depth = 0.0_f32;
        for &local in &self.hardpoints {
            let future = frame.transform_point(local) + frame.linear_velocity * dt;
            if let Some((height, _)) = field.surface_at(future.x, future.z)
                && future.y < height
            {
                depth = depth.max(height - future.y);
                below = true;
            }
        }
        if below {
            self.distance_below_terrain = depth;
            self.terrain_collision = true;
        }
    }

    /// `mainThreadTerrainCollisionResolution` (`0x1402D0480`): zero the vertical velocity, lift
    /// the body by the depth plus the wheel radius, and bounce it up with
    /// `−v.y · restitution · m`.
    pub(crate) fn resolve_terrain_penetration(
        &mut self,
        physics: &mut PhysicsWorld,
        body: BodyId,
        restitution: f32,
    ) {
        if physics.motion_type(body) == Some(physics::MotionType::Keyframed) {
            return;
        }
        let (Some(velocity), Some(position), Some(mass)) = (
            physics.linear_velocity(body),
            physics.position(body),
            physics.mass(body),
        ) else {
            return;
        };
        let downward = velocity.y;
        physics.set_linear_velocity(body, Vec3::new(velocity.x, 0.0, velocity.z));
        let lift = self.distance_below_terrain + self.suspension.wheel_radius;
        physics.set_position(body, position + Vec3::Y * lift);
        let bounce = -(mass * (restitution * downward));
        physics.apply_linear_impulse(body, Vec3::Y * bounce);
        self.terrain_collision = false;
    }

    /// Adds the action state to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.variant as u32);
        let velocity = self.previous_linear_velocity;
        checksum.hash_vec3(velocity.x, velocity.y, velocity.z);
        checksum.hash_f32(self.distance_below_terrain);
        checksum.hash_u32(u32::from(self.terrain_collision));
    }
}

/// The teleport impulse: `m · (−v.xz) + m / dt / 6 · (squad − body).xz`. It always acts, also
/// in the air. Without a squad the desired position is the body position.
fn apply_teleport_impulse(
    physics: &mut PhysicsWorld,
    body: BodyId,
    squad: Option<&SquadInput>,
    dt: f32,
) {
    let (Some(velocity), Some(position), Some(mass)) = (
        physics.linear_velocity(body),
        physics.position(body),
        physics.mass(body),
    ) else {
        return;
    };
    let desired = squad.map_or(position, |squad| squad.turn_radius_position);
    let stop = Vec3::new(-velocity.x, 0.0, -velocity.z) * mass;
    let offset = desired - position;
    let follow = Vec3::new(offset.x, 0.0, offset.z) * (mass * (1.0 / dt) * TELEPORT_IMPULSE_FACTOR);
    physics.apply_linear_impulse(body, stop + follow);
}

/// The yaw-match impulse: the angular impulse that reaches the desired orientation in one
/// substep, cut to its part about the body up axis.
fn apply_yaw_match(physics: &mut PhysicsWorld, body: BodyId, desired_forward: Vec3, dt: f32) {
    let (Some(frame), Some(inertia)) =
        (BodyFrame::read(physics, body), physics.inertia_local(body))
    else {
        return;
    };
    // `makeOrient(fwd, Y, Y × fwd)`: right, up and forward become the rotation columns.
    let desired = Quat::from_mat3(&Mat3::from_cols(
        Vec3::Y.cross(desired_forward),
        Vec3::Y,
        desired_forward,
    ));
    let difference = (desired * frame.rotation.inverse()).normalize();
    let angle = hk_acos(difference.w.abs()) * 2.0;
    let desired_velocity = if angle < YAW_MATCH_MIN_ANGLE {
        Vec3::ZERO
    } else {
        // `hkQuaternion::getAxis` flips the axis for a negative real part.
        let axis = Vec3::new(difference.x, difference.y, difference.z).normalize();
        let axis = if difference.w < 0.0 { -axis } else { axis };
        axis * (angle * (1.0 / dt))
    };
    let impulse =
        world_inertia(frame.matrix, inertia) * (desired_velocity - frame.angular_velocity);
    let up = frame.up();
    physics.apply_angular_impulse(body, up * up.dot(impulse));
}
