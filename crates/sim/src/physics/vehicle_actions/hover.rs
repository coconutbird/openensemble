//! `BPhysicsHoverFlightAction` (`applyAction` `0x1402D0E20`, `calcMovement` `0x1402D1BD0`): the
//! aircraft (hawk, hornet, vulture, banshee, vampire and sentinel).
//!
//! The action cancels the gravity and climbs toward the highest flight height under and ahead
//! of the aircraft. A crashing aircraft (`BUnitActionAvoidCollisionAir::Crashing`) dives through
//! its crash position instead. In the plane, springs drive the aircraft toward the squad
//! position and turn, pitch and roll it.

use glam::Vec3;
use physics::{BodyId, World as PhysicsWorld};

use super::frame::{
    BodyFrame, FLOAT_COMPARE_EPSILON, flat, normalize, safe_normalize_or_keep, signed_yaw,
    xz_distance,
};
use super::input::{AirInput, CrashInput, HeightSample, VehicleActionInput};
use super::reorient::reorient;
use super::warthog::set_vehicle_inertia;
use crate::math;
use crate::sync::SyncChecksum;

/// `cClimbRateMult`.
const CLIMB_RATE_MULTIPLIER: f32 = 1.0;
/// `cDescentRateMult`.
const DESCENT_RATE_MULTIPLIER: f32 = 0.5;
/// The flight height floor above the terrain: `+ 8` for the height, then `cMinAGLAlt` (5).
const TERRAIN_HEIGHT_OFFSET: f32 = 8.0;
/// `cMinAGLAlt`.
const MIN_AGL_ALTITUDE: f32 = 5.0;
/// The start value of the look-ahead heights and of the highest height.
const NO_HEIGHT: f32 = -10_000.0;
/// The number of look-ahead slots.
const LOOK_AHEAD_SLOTS: usize = 5;
/// The crash aim goes this far past the crash position.
const CRASH_AIM_DISTANCE: f32 = 50.0;
/// The crash aim goes this far below the crash position.
const CRASH_AIM_DROP: f32 = 100.0;
/// The crash throttle gain.
const CRASH_THROTTLE: f32 = 1.2;
/// A live kamikaze target triples the maximum velocity.
const KAMIKAZE_SPEED_FACTOR: f32 = 3.0;
/// The maximum velocity during the birth speed limit.
const SPEED_LIMIT: f32 = 8.0;
/// `cDecelRate` (per unit of maximum velocity).
const DECEL_RATE: f32 = -1.0;
/// `cFwdK`, `cStopFwdK`, `cRightK` and `cStopRightK`.
const PLANE_K: f32 = 1.0;
/// `cStopFwdDamp`.
const STOP_FORWARD_DAMP: f32 = 1.0;
/// `cTurnK` and `cAttackTurnK`.
const TURN_K: f32 = 5.0;
/// `cNoTurnDist`.
const NO_TURN_DISTANCE: f32 = 5.0;
/// The yaw damping factor at the goal.
const STOP_TURN_FACTOR: f32 = -0.5;
/// The attack target replaces the desired forward axis above this cosine.
const ATTACK_FACING_COSINE: f32 = 0.7;
/// The pitch and roll spring.
const ATTITUDE_K: f32 = -10.0;
/// The vertical avoidance band above the nearest obstacle.
const AVOID_BAND: f32 = 30.0;
/// The vertical avoidance height above the nearest obstacle.
const AVOID_HEIGHT: f32 = 20.0;
/// The vertical avoidance offset relaxes by this much each substep.
const AVOID_RELAX_STEP: f32 = 0.05;
/// The vertical avoidance offset snaps to 0 inside this band.
const AVOID_RELAX_BAND: f32 = 0.1;
/// Below this speed the pitch and roll ramp starts again.
const STOPPED_SPEED: f32 = 0.3;
/// `cReorientStrength`.
const REORIENT_STRENGTH: f32 = 0.5;
/// `cReorientDamping`.
const REORIENT_DAMPING: f32 = 0.1;
/// The body points of the height samples.
pub(crate) const SAMPLE_POINTS: [Vec3; 3] = [
    Vec3::new(0.0, 0.0, 4.0),
    Vec3::new(-4.0, 0.0, -4.0),
    Vec3::new(4.0, 0.0, -4.0),
];

/// The result of `calcMovement`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Movement {
    forward: f32,
    right: f32,
    yaw: f32,
    pitch: f32,
    roll: f32,
}

/// The state of one hover flight action.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HoverAction {
    /// `mDesiredPos`.
    desired_position: Vec3,
    /// `mVertAvoidOffset`.
    vertical_avoid_offset: f32,
    /// `mTimeSinceStopped`.
    time_since_stopped: f32,
    /// `mLookAheadHeight`.
    look_ahead_heights: [f32; LOOK_AHEAD_SLOTS],
    /// `mLookAheadIndex`.
    look_ahead_index: usize,
}

impl HoverAction {
    /// `BPhysicsHoverFlightAction_ctor` (`0x1402D0C90`): the inertia becomes
    /// `diag(m, m, 0.4 · m)`.
    pub(crate) fn new(physics: &mut PhysicsWorld, body: BodyId, unit_position: Vec3) -> Self {
        set_vehicle_inertia(physics, body);
        Self {
            desired_position: unit_position,
            vertical_avoid_offset: 0.0,
            time_since_stopped: 0.0,
            look_ahead_heights: [NO_HEIGHT; LOOK_AHEAD_SLOTS],
            look_ahead_index: 0,
        }
    }

    /// One substep of `applyAction`. Returns the new squad height: retail sets the squad height
    /// to the unit height first.
    ///
    /// The sim gives each hover unit its air input. Without it the action does nothing.
    pub(crate) fn apply(
        &mut self,
        physics: &mut PhysicsWorld,
        body: BodyId,
        input: &VehicleActionInput,
        dt: f32,
    ) -> Option<f32> {
        let squad_height = input.squad.map(|_| input.unit.position.y);
        let air = input.air?;
        match air.crash {
            Some(crash) => self.climb_to_crash(physics, body, input, crash, dt),
            None => self.climb_to_flight_height(physics, body, input, &air, dt),
        }
        let frame = BodyFrame::read(physics, body)?;
        let movement = self.calc_movement(physics, body, input, &air, dt);
        physics.set_damping(body, 0.0, 0.0);
        let mass = frame.mass;
        let forward = normalize(flat(frame.forward())) * (dt * mass * movement.forward);
        let right = normalize(flat(frame.right())) * (dt * mass * movement.right);
        physics.apply_linear_impulse(body, forward + right);
        physics.apply_angular_impulse(body, Vec3::new(0.0, dt * mass * movement.yaw, 0.0));
        physics.apply_angular_impulse(body, frame.right() * (dt * mass * movement.pitch));
        physics.apply_angular_impulse(body, frame.forward() * (dt * mass * movement.roll));
        reorient(
            physics,
            body,
            dt,
            Vec3::Z,
            REORIENT_STRENGTH,
            REORIENT_DAMPING,
        );
        squad_height
    }

    /// The crash branch: aim through the crash position and climb or dive to reach it at the
    /// time of impact.
    fn climb_to_crash(
        &mut self,
        physics: &mut PhysicsWorld,
        body: BodyId,
        input: &VehicleActionInput,
        crash: CrashInput,
        dt: f32,
    ) {
        let unit_position = input.unit.position;
        let crash_position = crash.kamikaze_target.unwrap_or(crash.position);
        let altitude_change = crash_position.y - unit_position.y;
        let mut through = normalize(crash_position - unit_position) * CRASH_AIM_DISTANCE;
        through.y -= CRASH_AIM_DROP;
        self.desired_position = crash_position + through;
        let Some(velocity) = physics.linear_velocity(body) else {
            return;
        };
        let horizontal_speed = flat(velocity).length().max(FLOAT_COMPARE_EPSILON);
        let time_to_impact = (xz_distance(unit_position, crash_position) / horizontal_speed)
            .max(FLOAT_COMPARE_EPSILON);
        let mut goal_climb = altitude_change / time_to_impact;
        if crash.kamikaze_target.is_none() {
            let max_speed = input.unit.proto_desired_velocity;
            goal_climb = goal_climb.clamp(-max_speed, max_speed);
        }
        let throttle = CRASH_THROTTLE * (goal_climb - velocity.y);
        apply_vertical_impulse(physics, body, throttle, dt);
    }

    /// The flight branch: climb toward the highest flight height of the body points and of the
    /// look-ahead slots.
    fn climb_to_flight_height(
        &mut self,
        physics: &mut PhysicsWorld,
        body: BodyId,
        input: &VehicleActionInput,
        air: &AirInput,
        dt: f32,
    ) {
        let terrain_offset = air.transport_hover_offset;
        let hover_offset = air.hover_altitude_offset;
        let floor = |sample: HeightSample| {
            let height = sample.terrain + TERRAIN_HEIGHT_OFFSET;
            if terrain_offset.is_some() {
                sample.terrain
            } else if height + MIN_AGL_ALTITUDE > sample.flight + hover_offset {
                height + MIN_AGL_ALTITUDE - hover_offset
            } else {
                sample.flight
            }
        };
        let mut highest = NO_HEIGHT;
        let has_move_action = input.squad.and_then(|squad| squad.move_action).is_some();
        if has_move_action {
            let index = self.look_ahead_index;
            self.look_ahead_heights[index] = floor(air.future_samples[index]);
            for &height in &self.look_ahead_heights {
                if height > highest {
                    highest = height;
                }
            }
        }
        for &sample in &air.body_samples {
            let height = floor(sample);
            if height > highest {
                highest = height;
            }
        }
        let goal = match terrain_offset {
            Some(offset) => highest + offset,
            None => highest + hover_offset + self.vertical_avoid_offset,
        };
        let mut goal_climb = CLIMB_RATE_MULTIPLIER * (goal - input.unit.position.y);
        if goal_climb < 0.0 {
            goal_climb *= DESCENT_RATE_MULTIPLIER;
        }
        let climb = physics
            .linear_velocity(body)
            .map_or(0.0, |velocity| velocity.y);
        let throttle = CLIMB_RATE_MULTIPLIER * (goal_climb - climb);
        apply_vertical_impulse(physics, body, throttle, dt);
        self.look_ahead_index = (self.look_ahead_index + 1) % LOOK_AHEAD_SLOTS;
    }

    /// `calcMovement`.
    fn calc_movement(
        &mut self,
        physics: &PhysicsWorld,
        body: BodyId,
        input: &VehicleActionInput,
        air: &AirInput,
        dt: f32,
    ) -> Movement {
        let unit = &input.unit;
        let (Some(velocity), Some(angular_velocity), Some(body_position)) = (
            physics.linear_velocity(body),
            physics.angular_velocity(body),
            physics.position(body),
        ) else {
            return Movement::default();
        };
        let current_position = unit.position;
        let mut desired_is_goal = true;
        let move_action = input.squad.and_then(|squad| squad.move_action);
        let mut max_velocity = if let Some(crash) = air.crash {
            let speed = unit.proto_desired_velocity;
            if crash.kamikaze_target.is_some() {
                speed * KAMIKAZE_SPEED_FACTOR
            } else {
                speed
            }
        } else if let (Some(squad), Some(move_action)) = (input.squad, move_action) {
            self.desired_position = squad.position;
            desired_is_goal = !move_action.working;
            move_action.velocity
        } else {
            if let Some(squad) = input.squad {
                self.desired_position = squad.position;
            }
            unit.proto_desired_velocity
        };
        let decel_rate = max_velocity * DECEL_RATE;
        if air.speed_limited {
            max_velocity = SPEED_LIMIT;
        }
        let current_forward = normalize(flat(unit.forward));
        let mut desired_forward =
            super::frame::safe_normalize(flat(self.desired_position - current_position))
                .unwrap_or(current_forward);
        let current_right = normalize(flat(unit.right));
        let forward_velocity = velocity.dot(current_forward);
        let right_velocity = velocity.dot(current_right);
        let distance = xz_distance(self.desired_position, current_position);
        let stopping_distance = if max_velocity != 0.0 && decel_rate != 0.0 {
            0.5 * max_velocity * max_velocity / -decel_rate
        } else {
            0.0
        };
        self.update_vertical_avoidance(air, body_position.y);
        let stasis = input
            .squad
            .map_or(1.0, |squad| squad.stasis_speed_multiplier);
        let mut reverse_speed = unit.reverse_speed;
        let mut movement = Movement::default();
        let at_goal = desired_is_goal && distance < stopping_distance;
        let (max_forward_accel, max_right_accel) = if at_goal {
            let desired_velocity = stasis * max_velocity * distance / stopping_distance;
            let mut desired_forward_velocity =
                desired_forward.dot(current_forward) * desired_velocity;
            limit_reverse(
                &mut reverse_speed,
                unit.max_velocity,
                &mut desired_forward_velocity,
            );
            let damp = STOP_FORWARD_DAMP * 2.0 * PLANE_K.sqrt();
            movement.forward =
                PLANE_K * (desired_forward_velocity - forward_velocity) - damp * forward_velocity;
            let desired_right_velocity = desired_forward.dot(current_right) * desired_velocity;
            movement.right = PLANE_K * (desired_right_velocity - right_velocity);
            (PLANE_K * max_velocity, PLANE_K * max_velocity)
        } else {
            let mut desired_forward_velocity =
                stasis * desired_forward.dot(current_forward) * max_velocity;
            limit_reverse(
                &mut reverse_speed,
                unit.max_velocity,
                &mut desired_forward_velocity,
            );
            movement.forward = PLANE_K * (desired_forward_velocity - forward_velocity);
            let desired_right_velocity = stasis * desired_forward.dot(current_right) * max_velocity;
            movement.right = PLANE_K * (desired_right_velocity - right_velocity);
            (PLANE_K * max_velocity, PLANE_K * max_velocity)
        };
        if stasis == 0.0 {
            return movement;
        }
        let yaw_factor = unit.turn_rate_degrees / 120.0;
        let (pitch_factor, roll_factor) = air
            .max_pitch_roll
            .map_or((30.0 / 40.0, 60.0 / 40.0), |(pitch, roll)| {
                (pitch / 40.0, roll / 40.0)
            });
        let crashing = air.crash.is_some();
        let mut attack = false;
        if let Some(target) = unit.ranged_target
            && !crashing
        {
            attack = true;
            let direction =
                safe_normalize_or_keep(flat(target.entity_or_target_position() - current_position));
            if reverse_speed > 0.0
                || at_goal
                || desired_forward.dot(direction) > ATTACK_FACING_COSINE
            {
                desired_forward = direction;
            }
        }
        movement.yaw = if (!attack || crashing)
            && (at_goal || (desired_is_goal && distance < NO_TURN_DISTANCE))
        {
            STOP_TURN_FACTOR * angular_velocity.y
        } else {
            yaw_factor * TURN_K * signed_yaw(desired_forward, current_forward)
        };
        let ramp = self.time_since_stopped.min(1.0);
        let max_pitch_angle = (45.0 * pitch_factor) * DEGREES_TO_RADIANS;
        let current_pitch = math::asin(unit.forward.y);
        let horizontal_speed = flat(velocity).length().max(0.01);
        let climb_angle = math::atan(velocity.y / horizontal_speed);
        let goal_pitch = if max_velocity == 0.0 {
            0.0
        } else {
            if max_velocity < horizontal_speed {
                max_velocity = horizontal_speed;
            }
            let mut goal = 0.25 * climb_angle * horizontal_speed / max_velocity;
            if max_forward_accel > FLOAT_COMPARE_EPSILON {
                goal -= (movement.forward / max_forward_accel) * max_pitch_angle * 2.0;
            }
            goal
        };
        movement.pitch = ATTITUDE_K * (goal_pitch - current_pitch) * ramp;
        let max_roll_angle = (60.0 * roll_factor) * DEGREES_TO_RADIANS;
        let current_roll = math::asin(unit.right.y);
        let mut goal_roll = if max_right_accel > FLOAT_COMPARE_EPSILON {
            (movement.right / max_right_accel) * max_roll_angle
        } else {
            0.0
        };
        if max_velocity > FLOAT_COMPARE_EPSILON {
            goal_roll += (60.0 * roll_factor) * DEGREES_TO_RADIANS * right_velocity / max_velocity;
        } else {
            goal_roll = 0.0;
        }
        movement.roll = ATTITUDE_K * (goal_roll - current_roll) * ramp;
        self.time_since_stopped += dt;
        if velocity.length() < STOPPED_SPEED {
            self.time_since_stopped = 0.0;
        }
        movement
    }

    /// `mVertAvoidOffset`: rises to stay 20 above the nearest obstacle while the aircraft avoids
    /// it, then relaxes toward 0 by 0.05 each substep.
    fn update_vertical_avoidance(&mut self, air: &AirInput, altitude: f32) {
        if let Some(obstacle) = air.avoiding_obstacle_altitude {
            if altitude >= obstacle && altitude <= obstacle + AVOID_BAND {
                self.vertical_avoid_offset = AVOID_HEIGHT - (altitude - obstacle);
            }
        } else if self.vertical_avoid_offset > AVOID_RELAX_BAND {
            self.vertical_avoid_offset -= AVOID_RELAX_STEP;
        } else if self.vertical_avoid_offset < -AVOID_RELAX_BAND {
            self.vertical_avoid_offset += AVOID_RELAX_STEP;
        } else {
            self.vertical_avoid_offset = 0.0;
        }
    }

    /// Adds the action state to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        let position = self.desired_position;
        checksum.hash_vec3(position.x, position.y, position.z);
        checksum.hash_f32(self.vertical_avoid_offset);
        checksum.hash_f32(self.time_since_stopped);
        for &height in &self.look_ahead_heights {
            checksum.hash_f32(height);
        }
        checksum.hash_u32(u32::try_from(self.look_ahead_index).unwrap_or(u32::MAX));
    }
}

/// `Math::fDegToRad`.
const DEGREES_TO_RADIANS: f32 = std::f32::consts::PI / 180.0;

/// The counter-gravity impulse `−m · dt · g` plus the throttle impulse `dt · m · throttle · Y`.
fn apply_vertical_impulse(physics: &mut PhysicsWorld, body: BodyId, throttle: f32, dt: f32) {
    let mass = physics.mass(body).unwrap_or(0.0);
    let counter_gravity = physics.gravity() * (-mass * dt);
    let hover = Vec3::Y * (dt * mass * throttle);
    physics.apply_linear_impulse(body, counter_gravity + hover);
}

/// Limits the reverse speed: an unset reverse speed (exactly −1) becomes the maximum velocity.
fn limit_reverse(reverse_speed: &mut f32, max_velocity: f32, desired_forward_velocity: &mut f32) {
    if reverse_speed.to_bits() == UNSET_REVERSE_SPEED.to_bits() {
        *reverse_speed = max_velocity;
    }
    if *desired_forward_velocity < -*reverse_speed {
        *desired_forward_velocity = -*reverse_speed;
    }
}

/// The reverse speed of a proto object that sets none.
const UNSET_REVERSE_SPEED: f32 = -1.0;
