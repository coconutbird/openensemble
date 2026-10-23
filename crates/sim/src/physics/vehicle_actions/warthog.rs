//! `BPhysicsWarthogAction` (`applyAction` `0x1402D77B0`, `calcMovement` `0x1402D8190`): the
//! warthog and the brute chopper.
//!
//! The warthog drives: while a wheel touches the terrain, spring accelerations push it along
//! its forward axis toward the squad position, side friction holds it on its line, and a yaw
//! spring turns it. The constants are hardcoded (2008 `physicswarthogaction.cpp:39-101`).

use glam::Vec3;
use physics::{BodyId, World as PhysicsWorld};

use super::frame::{BodyFrame, flat, normalize, safe_normalize, signed_yaw, xz_distance};
use super::input::{EarlyTerminateFlags, VehicleActionInput};
use super::reorient::reorient;
use super::suspension::{Suspension, Wheel, apply_gravity_and_springs, cast_wheels};
use crate::sync::SyncChecksum;

/// `(4 − 2.2142857) / 2.2142857`: four times the gravity of 9.8 m/s² in total.
pub(super) const EXTRA_GRAVITY_FACTOR: f32 = 0.806_451_7;
/// The warthog and ghost hardpoints: front left, front right, back left, back right.
pub(super) const HARDPOINTS: [Vec3; 4] = [
    Vec3::new(-1.1, -0.05, 1.3),
    Vec3::new(1.1, -0.05, 1.3),
    Vec3::new(-1.1, -0.05, -1.1),
    Vec3::new(1.1, -0.05, -1.1),
];
const SUSPENSION: Suspension = Suspension {
    spring_length: 1.3,
    wheel_radius: 1.0,
    spring_strength: 50.0,
    damping_compression: 2.5,
    damping_relaxation: 2.5,
    normal_clipping_angle: 0.1,
};
/// `cDecelRate` (per unit of maximum velocity).
const DECEL_RATE: f32 = -1.0;
/// `cFwdK`.
const FORWARD_K: f32 = 2.5;
/// `cFwdDamp`.
const FORWARD_DAMP: f32 = 0.0;
/// `cStopFwdK`.
const STOP_FORWARD_K: f32 = 5.0;
/// `cStopFwdDamp`.
const STOP_FORWARD_DAMP: f32 = 0.0;
/// `cMaxTurnK`.
const MAX_TURN_K: f32 = 50.0;
/// `cMinTurnK`.
const MIN_TURN_K: f32 = 30.0;
/// `cTurnDamp · 2` (the compiler folds the product). `mStayOnPath` is always `false` in
/// retail, so the path turn spring and the path friction (`cPathTurnK`, `cPathSideFriction`)
/// never act.
const TURN_DAMP_TWICE: f32 = 0.6;
/// `cMaxSideFriction`: at the goal.
const MAX_SIDE_FRICTION: f32 = -40.0;
/// `cMinSideFriction`: on the way.
const MIN_SIDE_FRICTION: f32 = -4.0;
/// `cReorientStrength`.
const REORIENT_STRENGTH: f32 = 0.75;
/// `cReorientDamping`.
const REORIENT_DAMPING: f32 = 0.1;
/// The squad is at the move target below this distance.
const GOAL_EPSILON: f32 = 0.05;
/// The smallest maximum velocity.
const MIN_MAX_VELOCITY: f32 = 0.001;
/// The smallest obstruction radius for the catch-up speed.
const MIN_OBSTRUCTION_RADIUS: f32 = 0.01;
/// The action does not turn closer to the desired position than this.
const NO_TURN_DISTANCE: f32 = 1.0;
/// `cIdleMagnitude`: below this speed the vehicle is idle.
const IDLE_SPEED: f32 = 1.0;
/// `cSkidMagnitude`: above this side speed the vehicle skids.
const SKID_SPEED: f32 = 5.0;

/// The result of `calcMovement`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Movement {
    /// The forward acceleration.
    pub(super) acceleration: f32,
    /// The yaw acceleration.
    pub(super) turning: f32,
    /// `true` if the vehicle brakes at its goal.
    pub(super) at_goal: bool,
}

/// The state of one warthog or chopper action.
#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one retail action flag (mFlagChopper, mCanPivotToTarget, ...)"
)]
pub(crate) struct WarthogAction {
    /// `mFlagChopper`.
    chopper: bool,
    /// `mDesiredPos`.
    desired_position: Vec3,
    /// `mCanPivotToTarget`.
    can_pivot_to_target: bool,
    /// `mFlagInAir`.
    in_air: bool,
    /// `mFlagSkidding`.
    skidding: bool,
    /// `mPrevTurning`.
    previous_turning: f32,
}

impl WarthogAction {
    /// `BPhysicsWarthogAction_ctor` (`0x1402D6AA0`): the inertia becomes
    /// `diag(m, m, 0.4 · m)`; the desired position starts at the unit position.
    pub(crate) fn new(
        physics: &mut PhysicsWorld,
        body: BodyId,
        chopper: bool,
        unit_position: Vec3,
    ) -> Self {
        set_vehicle_inertia(physics, body);
        Self {
            chopper,
            desired_position: unit_position,
            can_pivot_to_target: false,
            in_air: false,
            skidding: false,
            previous_turning: 0.0,
        }
    }

    #[cfg(test)]
    /// `true` for the brute chopper.
    pub(crate) fn is_chopper(&self) -> bool {
        self.chopper
    }

    /// `mFlagInAir`.
    pub(crate) fn in_air(&self) -> bool {
        self.in_air
    }

    /// One substep of `applyAction`.
    pub(crate) fn apply(
        &mut self,
        physics: &mut PhysicsWorld,
        body: BodyId,
        input: &VehicleActionInput,
        flags: &mut EarlyTerminateFlags,
        dt: f32,
    ) {
        let Some(frame) = BodyFrame::read(physics, body) else {
            return;
        };
        let mut wheels = [Wheel::default(); 4];
        let any_hit = cast_wheels(physics, body, &frame, &HARDPOINTS, &SUSPENSION, &mut wheels);
        apply_gravity_and_springs(physics, body, dt, EXTRA_GRAVITY_FACTOR, &wheels);
        self.in_air = !any_hit;
        let movement = if any_hit {
            self.calc_movement(physics, body, input, flags)
        } else {
            Movement::default()
        };
        physics.set_damping(body, 0.0, 0.0);
        self.previous_turning = physics
            .angular_velocity(body)
            .map_or(0.0, |velocity| velocity.y);
        if any_hit {
            let side_friction = if movement.at_goal {
                MAX_SIDE_FRICTION
            } else {
                MIN_SIDE_FRICTION
            };
            apply_drive(physics, body, &frame, movement, side_friction, dt);
        }
        reorient(
            physics,
            body,
            dt,
            Vec3::Z,
            REORIENT_STRENGTH,
            REORIENT_DAMPING,
        );
    }

    /// `calcMovement` with the squad move action.
    fn calc_movement(
        &mut self,
        physics: &PhysicsWorld,
        body: BodyId,
        input: &VehicleActionInput,
        flags: &mut EarlyTerminateFlags,
    ) -> Movement {
        let unit = &input.unit;
        if !unit.has_physics_action {
            return Movement {
                acceleration: 0.0,
                turning: 0.0,
                at_goal: true,
            };
        }
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
        let (max_velocity, squad_velocity, desired_is_goal) = match (squad_move, input.squad) {
            (Some((squad, move_action)), _) => {
                flags.allow = true;
                self.desired_position = squad.position;
                let direction = move_action.target - self.desired_position;
                let is_goal = direction.length() < GOAL_EPSILON || move_action.waiting;
                (squad.desired_velocity, move_action.velocity, is_goal)
            }
            (None, Some(squad)) => {
                self.desired_position = squad.position;
                (unit.proto_desired_velocity, 0.0, true)
            }
            (None, None) => {
                self.desired_position = current_position;
                (unit.proto_desired_velocity, 0.0, true)
            }
        };
        let max_velocity = f32::max(max_velocity, MIN_MAX_VELOCITY);
        let decel_rate = max_velocity * DECEL_RATE;
        let current_forward = normalize(flat(unit.forward));
        let mut desired_forward = safe_normalize(flat(self.desired_position - current_position))
            .unwrap_or(current_forward);
        let linear = flat(velocity);
        let forward_velocity = linear.dot(current_forward);
        let mut distance = xz_distance(self.desired_position, current_position);
        let stopping_distance = 0.5 * max_velocity * max_velocity / -decel_rate;
        let mut movement = Movement::default();
        if desired_is_goal && distance < stopping_distance {
            let desired_velocity = max_velocity * distance / stopping_distance;
            let difference =
                desired_forward.dot(current_forward) * desired_velocity - forward_velocity;
            let damp = STOP_FORWARD_DAMP * 2.0 * STOP_FORWARD_K.sqrt();
            movement.acceleration = STOP_FORWARD_K * difference - damp * forward_velocity;
            movement.at_goal = true;
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
            movement.acceleration = k * difference - damp * forward_velocity;
        }
        if self.can_pivot_to_target {
            if !movement.at_goal {
                self.can_pivot_to_target = false;
            }
        } else if movement.at_goal && distance <= NO_TURN_DISTANCE {
            self.can_pivot_to_target = true;
        }
        let mut attack = false;
        if self.chopper
            && self.can_pivot_to_target
            && let Some(target) = unit.ranged_target
        {
            attack = target.entity_position.is_some();
            if unit.doing_fatality {
                let forward = flat(unit.forward);
                desired_forward = safe_normalize(forward).unwrap_or(forward);
            } else if let Some(position) = target.entity_position {
                let direction = flat(position - current_position);
                desired_forward = safe_normalize(direction).unwrap_or(direction);
            }
        }
        let mut angle = 0.0;
        if distance > NO_TURN_DISTANCE || attack {
            angle = signed_yaw(desired_forward, current_forward);
        }
        if movement.at_goal && !attack {
            angle = fold_to_front_or_back(angle);
        }
        let turn_k = MIN_TURN_K
            + (MAX_TURN_K - MIN_TURN_K) * (forward_velocity / max_velocity).clamp(0.0, 1.0);
        let turn_damp = TURN_DAMP_TWICE * turn_k.sqrt();
        movement.turning = turn_k * angle - turn_damp * angular_velocity.y;
        let speed = linear.length();
        self.skidding = speed >= IDLE_SPEED && linear.dot(unit.right).abs() > SKID_SPEED;
        if movement.at_goal
            && speed < IDLE_SPEED
            && physics.is_deactivation_enabled(body) == Some(true)
            && flags.allow
        {
            flags.terminate = true;
        }
        movement
    }

    /// Adds the action state to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.chopper));
        let position = self.desired_position;
        checksum.hash_vec3(position.x, position.y, position.z);
        checksum.hash_u32(u32::from(self.can_pivot_to_target));
        checksum.hash_u32(u32::from(self.in_air));
        checksum.hash_u32(u32::from(self.skidding));
        checksum.hash_f32(self.previous_turning);
    }
}

/// The warthog, ghost and hover constructors: `diag(1 · m, 1 · m, 0.4 · m)`.
pub(super) fn set_vehicle_inertia(physics: &mut PhysicsWorld, body: BodyId) {
    let mass = physics.mass(body).unwrap_or(1.0);
    physics.set_inertia_local(body, Vec3::new(mass, mass, 0.4 * mass));
}

/// At the goal the warthog may back up: an angle beyond ±90° turns the back toward the goal.
fn fold_to_front_or_back(angle: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    if angle > FRAC_PI_2 {
        angle - PI
    } else if angle < -FRAC_PI_2 {
        angle + PI
    } else {
        angle
    }
}

/// The drive impulses of a vehicle with ground contact: the forward acceleration and side
/// friction, each with the part of the gravity along its axis removed, and the yaw.
fn apply_drive(
    physics: &mut PhysicsWorld,
    body: BodyId,
    frame: &BodyFrame,
    movement: Movement,
    side_friction: f32,
    dt: f32,
) {
    let mass = frame.mass;
    let forward = frame.forward();
    let right = frame.right();
    let gravity = physics.gravity() * (dt * (1.0 + mass * EXTRA_GRAVITY_FACTOR));
    let forward_impulse =
        forward * (dt * mass * movement.acceleration) + forward * -gravity.dot(forward);
    let side_velocity = frame.linear_velocity.dot(right);
    let right_impulse =
        right * (dt * mass * side_velocity * side_friction) + right * -gravity.dot(right);
    physics.apply_linear_impulse(body, forward_impulse + right_impulse);
    physics.apply_angular_impulse(body, Vec3::new(0.0, dt * mass * movement.turning, 0.0));
}
