//! `BPhysicsGhostAction` (`applyAction` `0x1402CCC00`, `calcMovement` `0x1402CD370`).
//!
//! The ghost hovers on the warthog suspension with softer springs. It drives forward and
//! strafes toward the squad position, also without ground contact, and turns toward its attack
//! target. The constants are hardcoded (2008 `physicsghostaction.cpp:29-102`).

use glam::Vec3;
use physics::{BodyId, World as PhysicsWorld};

use super::frame::{BodyFrame, flat, normalize, safe_normalize, signed_yaw, xz_distance};
use super::input::VehicleActionInput;
use super::reorient::reorient;
use super::suspension::{Suspension, Wheel, apply_gravity_and_springs, cast_wheels};
use super::warthog::{EXTRA_GRAVITY_FACTOR, HARDPOINTS, set_vehicle_inertia};
use crate::sync::SyncChecksum;

const SUSPENSION: Suspension = Suspension {
    spring_length: 1.3,
    wheel_radius: 1.0,
    spring_strength: 30.0,
    damping_compression: 1.0,
    damping_relaxation: 0.5,
    normal_clipping_angle: 0.1,
};
/// `cDecelRate` (per unit of maximum velocity).
const DECEL_RATE: f32 = -1.0;
/// `cFwdK`.
const FORWARD_K: f32 = 2.5;
/// `cFwdDamp`.
const FORWARD_DAMP: f32 = 0.0;
/// `cStopFwdK` (also the static of the stop branch).
const STOP_FORWARD_K: f32 = 5.0;
/// `cStopFwdDamp`.
const STOP_FORWARD_DAMP: f32 = 0.0;
/// `cStopRightK`.
const STOP_RIGHT_K: f32 = 5.0;
/// `cStopRightDamp`.
const STOP_RIGHT_DAMP: f32 = 0.0;
/// `cRightK`.
const RIGHT_K: f32 = 2.5;
/// `cRightDamp`.
const RIGHT_DAMP: f32 = 0.0;
/// `cTurnK` and `cAttackTurnK`.
const TURN_K: f32 = 10.0;
/// `cTurnDamp` and `cAttackTurnDamp`.
const TURN_DAMP: f32 = 0.0;
/// The yaw damping factor at the goal.
const STOP_TURN_FACTOR: f32 = -0.5;
/// `cAccelerationFactor`.
const ACCELERATION_FACTOR: f32 = 1.0;
/// `cReorientStrength`.
const REORIENT_STRENGTH: f32 = 0.5;
/// `cReorientDamping`.
const REORIENT_DAMPING: f32 = 0.1;
/// The squad is at the move target below this distance.
const GOAL_EPSILON: f32 = 0.05;
/// The smallest obstruction radius for the catch-up speed.
const MIN_OBSTRUCTION_RADIUS: f32 = 0.01;

/// The result of `calcMovement`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Movement {
    forward: f32,
    right: f32,
    turning: f32,
}

/// The state of one ghost action.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GhostAction {
    /// `mDesiredPos`.
    desired_position: Vec3,
    /// `true` if no wheel ray hit the terrain in the last substep.
    in_air: bool,
}

impl GhostAction {
    /// `BPhysicsGhostAction_ctor` (`0x1402CCA90`): the inertia becomes `diag(m, m, 0.4 · m)`.
    pub(crate) fn new(physics: &mut PhysicsWorld, body: BodyId, unit_position: Vec3) -> Self {
        set_vehicle_inertia(physics, body);
        Self {
            desired_position: unit_position,
            in_air: false,
        }
    }

    /// `true` if no wheel ray hit the terrain in the last substep.
    pub(crate) fn in_air(&self) -> bool {
        self.in_air
    }

    /// One substep of `applyAction`.
    pub(crate) fn apply(
        &mut self,
        physics: &mut PhysicsWorld,
        body: BodyId,
        input: &VehicleActionInput,
        dt: f32,
    ) {
        let Some(frame) = BodyFrame::read(physics, body) else {
            return;
        };
        let mut wheels = [Wheel::default(); 4];
        let any_hit = cast_wheels(physics, body, &frame, &HARDPOINTS, &SUSPENSION, &mut wheels);
        self.in_air = !any_hit;
        apply_gravity_and_springs(physics, body, dt, EXTRA_GRAVITY_FACTOR, &wheels);
        let movement = self.calc_movement(physics, body, input);
        physics.set_damping(body, 0.0, 0.0);
        let mass = frame.mass;
        let forward = frame.forward() * (dt * mass * ACCELERATION_FACTOR * movement.forward);
        let right = frame.right() * (dt * mass * ACCELERATION_FACTOR * movement.right);
        physics.apply_linear_impulse(body, forward + right);
        physics.apply_angular_impulse(body, Vec3::new(0.0, dt * mass * movement.turning, 0.0));
        reorient(
            physics,
            body,
            dt,
            Vec3::Z,
            REORIENT_STRENGTH,
            REORIENT_DAMPING,
        );
    }

    /// `calcMovement`.
    fn calc_movement(
        &mut self,
        physics: &PhysicsWorld,
        body: BodyId,
        input: &VehicleActionInput,
    ) -> Movement {
        let unit = &input.unit;
        let (Some(velocity), Some(angular_velocity)) = (
            physics.linear_velocity(body),
            physics.angular_velocity(body),
        ) else {
            return Movement::default();
        };
        let current_position = unit.position;
        let squad_move = input
            .squad
            .and_then(|squad| squad.move_action.map(|move_action| (squad, move_action)));
        let (max_velocity, squad_velocity, desired_is_goal) = if unit.doing_fatality {
            // A hijacked ghost holds its place: the desired position does not change.
            (unit.proto_desired_velocity, 0.0, true)
        } else if let Some((squad, move_action)) = squad_move {
            self.desired_position = squad.position;
            let direction = move_action.target - self.desired_position;
            let is_goal = direction.length() < GOAL_EPSILON || move_action.waiting;
            (squad.desired_velocity, move_action.velocity, is_goal)
        } else {
            if let Some(squad) = input.squad {
                self.desired_position = squad.position;
            }
            (unit.proto_desired_velocity, 0.0, true)
        };
        let decel_rate = max_velocity * DECEL_RATE;
        let current_forward = normalize(flat(unit.forward));
        let mut desired_forward = safe_normalize(flat(self.desired_position - current_position))
            .unwrap_or(current_forward);
        let current_right = normalize(flat(unit.right));
        let linear = flat(velocity);
        let forward_velocity = linear.dot(current_forward);
        let right_velocity = linear.dot(current_right);
        let mut distance = xz_distance(self.desired_position, current_position);
        let stopping_distance = 0.5 * max_velocity * max_velocity / -decel_rate;
        let mut movement = Movement::default();
        let mut at_goal = false;
        if desired_is_goal && distance < stopping_distance {
            at_goal = true;
            let desired_velocity = max_velocity * distance / stopping_distance;
            let difference =
                desired_forward.dot(current_forward) * desired_velocity - forward_velocity;
            let damp = STOP_FORWARD_DAMP * 2.0 * STOP_FORWARD_K.sqrt();
            movement.forward = STOP_FORWARD_K * difference - damp * forward_velocity;
            let difference = desired_forward.dot(current_right) * desired_velocity - right_velocity;
            let damp = STOP_RIGHT_DAMP * 2.0 * STOP_RIGHT_K.sqrt();
            movement.right = STOP_RIGHT_K * difference - damp * right_velocity;
        } else {
            let obstruction = f32::max(MIN_OBSTRUCTION_RADIUS, unit.obstruction_radius);
            if distance > obstruction * 2.0 {
                distance = obstruction * 2.0;
            }
            let mut desired_velocity = squad_velocity;
            if distance > obstruction {
                let factor = (distance - obstruction) / obstruction;
                desired_velocity = squad_velocity + (max_velocity - squad_velocity) * factor;
            }
            let difference =
                desired_forward.dot(current_forward) * desired_velocity - forward_velocity;
            let k = if difference < 0.0 {
                STOP_FORWARD_K
            } else {
                FORWARD_K
            };
            let damp = FORWARD_DAMP * 2.0 * k.sqrt();
            movement.forward = k * difference - damp * forward_velocity;
            let difference = desired_forward.dot(current_right) * max_velocity - right_velocity;
            let damp = RIGHT_DAMP * 2.0 * RIGHT_K.sqrt();
            movement.right = RIGHT_K * difference - damp * right_velocity;
        }
        let mut attack = false;
        if let Some(target) = unit.ranged_target {
            attack = target.entity_position.is_some();
            if unit.doing_fatality {
                let forward = flat(unit.forward);
                desired_forward = safe_normalize(forward).unwrap_or(forward);
            } else if let Some(position) = target.entity_position {
                let direction = flat(position - current_position);
                desired_forward = safe_normalize(direction).unwrap_or(direction);
            }
        }
        movement.turning = if !attack && at_goal {
            STOP_TURN_FACTOR * angular_velocity.y
        } else {
            let angle = signed_yaw(desired_forward, current_forward);
            let damp = TURN_DAMP * 2.0 * TURN_K.sqrt();
            TURN_K * angle - damp * angular_velocity.y
        };
        movement
    }

    /// Adds the action state to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        let position = self.desired_position;
        checksum.hash_vec3(position.x, position.y, position.z);
        checksum.hash_u32(u32::from(self.in_air));
    }
}
