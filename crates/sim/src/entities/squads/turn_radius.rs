#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the squad update of the plumbing branch calls the turn-radius update"
    )
)]
//! The turn-radius position and forward axis of a physics vehicle squad.
//!
//! Retail `BSquad` (DE `+0x1D0` and `+0x1E0`, 2008 `squad.cpp:8490-8741`, `NEW_TURNRADIUS`)
//! keeps a point that follows the squad position at a limited turn rate. The ground vehicle
//! action drives its body toward this point and turns it to this forward axis.
//! `BSquad_updateTurning` (`0x1403A70E0`) moves the point once per sim update, and
//! `BSquad_calcTurning` (`0x1403A73E0`) limits its turn.

use glam::Vec3;

use super::Squad;
use crate::math;
use crate::sync::SyncChecksum;

/// `cActionCompleteEpsilon`: the point has caught up with the squad below this distance.
const COMPLETE_EPSILON: f32 = 0.1;
/// `cMaxSpeedupFactor`: the point moves and turns up to twice as fast to catch up.
const MAX_SPEEDUP_FACTOR: f32 = 2.0;
/// `cCatchUpFastestDistance`.
const CATCH_UP_FASTEST_DISTANCE: f32 = 10.0;
/// `cFloatCompareEpsilon`.
const EPSILON: f32 = 0.000_001;
/// `cRadiansPerDegree` as DE stores it.
const RADIANS_PER_DEGREE: f32 = 0.017_453_292;
/// The time step with no movement uses this speed for the turn-radius speed limit.
const NO_TIME_SPEED: f32 = 100.0;

/// The turn-radius state of a squad (`mFlagUpdateTurnRadius`, `mTurnRadiusPos`,
/// `mTurnRadiusFwd`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SquadTurnRadius {
    state: Option<(Vec3, Vec3)>,
}

impl SquadTurnRadius {
    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.state.is_some()));
        if let Some((position, forward)) = self.state {
            checksum.hash_vec3(position.x, position.y, position.z);
            checksum.hash_vec3(forward.x, forward.y, forward.z);
        }
    }
}

/// What the leader unit gives the turn-radius update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TurnRadiusLeader {
    /// The turn rate of the leader proto object in degrees per second.
    pub(crate) turn_rate_degrees: f32,
    /// `BSquad::getDesiredVelocity`: the desired velocity of the leader unit.
    pub(crate) desired_velocity: f32,
}

/// The state that `updateTurning` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TurnRadiusProgress {
    /// The point has caught up and the squad completed its movement (`cStateDone`).
    Done,
    /// The point has caught up, but the squad still moves (`cStatePathing`).
    Pathing,
    /// The point moves (`cStateWorking`).
    Working,
}

impl Squad {
    /// The turn-radius position and forward axis, if the squad updates them.
    pub(crate) fn turn_radius(&self) -> Option<(Vec3, Vec3)> {
        self.turn_radius_state.state
    }

    /// `BSquad::init` for a physics vehicle squad that is not an aircraft: the point starts at
    /// the squad position and forward axis.
    pub(crate) fn start_turn_radius(&mut self) {
        self.turn_radius_state.state = Some((self.base.position, self.base.forward));
    }

    /// `BSquad::updateTurning`. `completed` is `hasCompletedMovement_4`: the squad has no move
    /// action that works.
    pub(crate) fn update_turning(
        &mut self,
        elapsed: f32,
        leader: Option<TurnRadiusLeader>,
        completed: bool,
    ) -> TurnRadiusProgress {
        let Some((position, forward)) = self.turn_radius_state.state else {
            return TurnRadiusProgress::Done;
        };
        let mut direction = self.base.position - position;
        direction.y = 0.0;
        let remaining = direction.length();
        let direction = safe_normalize(direction);
        if remaining < COMPLETE_EPSILON {
            // `mFlagMatchFacing` is never set for these squads in the sim.
            return if completed {
                TurnRadiusProgress::Done
            } else {
                TurnRadiusProgress::Pathing
            };
        }
        let speedup = smooth_step(remaining / CATCH_UP_FASTEST_DISTANCE);
        let base_velocity = leader.map_or(0.0, |leader| leader.desired_velocity);
        let velocity =
            base_velocity + (MAX_SPEEDUP_FACTOR * base_velocity - base_velocity) * speedup;
        let step = (elapsed * velocity).min(remaining);
        let mut new_position = position + direction * step;
        let mut new_forward = direction;
        if let Some(leader) = leader {
            self.limit_turn(
                elapsed,
                step,
                remaining,
                leader,
                (position, forward),
                (&mut new_position, &mut new_forward),
            );
        }
        new_position.y = 0.0;
        self.turn_radius_state.state = Some((new_position, new_forward));
        TurnRadiusProgress::Working
    }

    /// `BSquad::calcTurning`: turns `forward` toward `new_forward` by at most the turn rate,
    /// and shortens the step for a sharp turn and for the turn radius.
    fn limit_turn(
        &self,
        elapsed: f32,
        step: f32,
        remaining: f32,
        leader: TurnRadiusLeader,
        (position, forward): (Vec3, Vec3),
        (new_position, new_forward): (&mut Vec3, &mut Vec3),
    ) {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        let mut difference = yaw_from_direction(*new_forward) - yaw_from_direction(forward);
        if (-EPSILON..=EPSILON).contains(&difference) {
            return;
        }
        let turn_speed = leader.turn_rate_degrees * RADIANS_PER_DEGREE;
        if turn_speed < EPSILON {
            return;
        }
        if difference > PI {
            difference = -(TAU - difference);
        } else if difference < -PI {
            difference += TAU;
        }
        let magnitude = difference.abs();
        let turn_speed = turn_speed
            + (MAX_SPEEDUP_FACTOR * turn_speed - turn_speed) * smooth_step(magnitude / FRAC_PI_2);
        let max_turn = turn_speed * elapsed;
        if magnitude <= max_turn {
            return;
        }
        let yaw = if difference < 0.0 {
            -max_turn
        } else {
            max_turn
        };
        let rotated = math::quat_from_axis_angle(self.base.up, yaw) * forward;
        *new_forward = rotated.normalize_or_zero();
        let mut distance = step;
        if self.min_turn_radius > 0.0 && self.max_turn_radius > 0.0 {
            let min_velocity = self.min_turn_radius * turn_speed;
            let max_velocity = if elapsed > EPSILON {
                step / elapsed
            } else {
                NO_TIME_SPEED
            };
            let factor = (magnitude / FRAC_PI_2).min(1.0);
            distance = (max_velocity + (min_velocity - max_velocity) * factor) * elapsed;
        }
        if remaining > EPSILON {
            let clamp = max_turn * remaining / magnitude * 0.5;
            if distance > clamp {
                distance = clamp;
            }
        }
        *new_position = position + *new_forward * distance;
    }
}

/// `Math::fSmoothStep(0, 1, t)`.
fn smooth_step(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (3.0 - t * 2.0) * (t * t)
}

/// `BVector::safeNormalize`.
fn safe_normalize(vector: Vec3) -> Vec3 {
    let length_squared = vector.length_squared();
    if length_squared > EPSILON * EPSILON {
        vector * (1.0 / length_squared.sqrt())
    } else {
        vector
    }
}

/// `BVector::getAngleAroundY` (DE `BVector_yawFromDirection`, `0x1406A5630`): the yaw in
/// `[0, 2π)`, measured from `+z` toward `+x`.
pub(crate) fn yaw_from_direction(vector: Vec3) -> f32 {
    use std::f32::consts::PI;
    let flat = Vec3::new(vector.x, 0.0, vector.z);
    let length = flat.length();
    let unit = if length > 0.0 { flat / length } else { Vec3::Z };
    let arc = || {
        if unit.z > 1.0 {
            0.0
        } else {
            math::acos(unit.z)
        }
    };
    if unit.x > EPSILON {
        arc()
    } else if unit.x < -EPSILON {
        PI - arc() + PI
    } else if unit.z < 0.0 {
        PI
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
