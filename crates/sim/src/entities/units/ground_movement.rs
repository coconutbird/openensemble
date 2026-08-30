//! Retail `BUnitActionMove` state for non-physics squad members.

use super::Unit;
use crate::entities::BaseEntity;
use crate::sync::SyncChecksum;
use glam::Vec3;

const ACTION_COMPLETE_EPSILON: f32 = 0.1;
const ANGLE_EPSILON: f32 = 0.000_001;
const MAX_RETRY_ATTEMPTS: u8 = 3;

/// Source action phase retained by a ground unit following its squad formation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroundMovePhase {
    /// No squad-owned ground movement action.
    #[default]
    Inactive,
    /// Moving toward the child formation target.
    Working,
    /// Caught the current target before the squad action completed and retrying.
    Pathing,
    /// The source three-retry guard expired.
    Failed,
}

/// Checksummed subset of retail's `BUnitActionMove` used by Move4 squad moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UnitGroundMove {
    phase: GroundMovePhase,
    target: Option<Vec3>,
    action_velocity: f32,
    retry_attempts: u8,
    squad_action_working: bool,
}

impl Default for UnitGroundMove {
    fn default() -> Self {
        Self {
            phase: GroundMovePhase::Inactive,
            target: None,
            action_velocity: 0.0,
            retry_attempts: 0,
            squad_action_working: false,
        }
    }
}

impl UnitGroundMove {
    fn prepare(&mut self, target: Vec3, action_velocity: f32, squad_action_working: bool) {
        if !target.is_finite() {
            self.cancel();
            return;
        }
        let changed_target = self.target != Some(target);
        if changed_target
            || matches!(
                self.phase,
                GroundMovePhase::Inactive | GroundMovePhase::Failed
            )
            || (!squad_action_working && self.phase == GroundMovePhase::Pathing)
        {
            self.phase = GroundMovePhase::Working;
            self.retry_attempts = 0;
        }
        self.target = Some(target);
        self.action_velocity = finite_nonnegative(action_velocity);
        self.squad_action_working = squad_action_working;
    }

    fn cancel(&mut self) {
        *self = Self::default();
    }

    fn advance(
        &mut self,
        base: &mut BaseEntity,
        desired_velocity: f32,
        turn_rate_degrees: f32,
        reverse: bool,
        dt: f32,
    ) {
        if !valid_step(dt) || self.phase == GroundMovePhase::Inactive {
            return;
        }
        if self.phase == GroundMovePhase::Failed {
            base.velocity = Vec3::ZERO;
            return;
        }
        if self.phase == GroundMovePhase::Pathing {
            self.retry_attempts = self.retry_attempts.saturating_add(1);
            if self.retry_attempts > MAX_RETRY_ATTEMPTS {
                self.phase = GroundMovePhase::Failed;
                base.velocity = Vec3::ZERO;
                return;
            }
            self.phase = GroundMovePhase::Working;
        }
        let Some(target) = self.target else {
            self.cancel();
            base.velocity = Vec3::ZERO;
            return;
        };
        let to_target = planar(target - base.position);
        let distance = to_target.length();
        if distance < ACTION_COMPLETE_EPSILON {
            finish_at_target(self, base, target);
            return;
        }

        let direction = to_target / distance;
        let desired_facing = if reverse { -direction } else { direction };
        let velocity = if self.squad_action_working {
            self.action_velocity
        } else {
            finite_nonnegative(desired_velocity)
        };
        let distance_change = (velocity * dt).min(distance);
        let (facing, distance_change) = limit_turn(
            base.forward,
            desired_facing,
            turn_rate_degrees,
            dt,
            distance_change,
            distance,
        );
        let travel_direction = if reverse { -facing } else { facing };
        if distance_change >= distance && travel_direction.dot(direction) > 0.999 {
            finish_at_target(self, base, target);
            return;
        }

        base.set_forward(facing);
        base.position += travel_direction * distance_change;
        base.velocity = travel_direction * (distance_change / dt);
    }

    pub(crate) const fn phase(self) -> GroundMovePhase {
        self.phase
    }

    pub(crate) const fn target(self) -> Option<Vec3> {
        self.target
    }

    pub(crate) const fn owns_squad_transform(self) -> bool {
        !matches!(self.phase, GroundMovePhase::Inactive)
    }

    pub(crate) const fn is_active(self) -> bool {
        matches!(
            self.phase,
            GroundMovePhase::Working | GroundMovePhase::Pathing
        )
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        if let Some(target) = self.target {
            checksum.hash_u32(1);
            checksum.hash_vec3(target.x, target.y, target.z);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_f32(self.action_velocity);
        checksum.hash_u32(u32::from(self.retry_attempts));
        checksum.hash_u32(u32::from(self.squad_action_working));
    }
}

impl Unit {
    pub(crate) fn prepare_squad_ground_move(
        &mut self,
        target: Vec3,
        action_velocity: f32,
        squad_action_working: bool,
    ) {
        self.ground_move
            .prepare(target, action_velocity, squad_action_working);
    }

    pub(crate) fn cancel_squad_ground_move(&mut self) {
        if self.ground_move.owns_squad_transform() {
            self.base.velocity = Vec3::ZERO;
        }
        self.ground_move.cancel();
    }

    pub(crate) fn advance_squad_ground_move(&mut self, dt: f32) {
        let reverse = self.is_reverse_moving();
        let desired_velocity = if reverse {
            self.reverse_speed()
        } else {
            self.speed
        } * self.effective_velocity_scalar();
        self.ground_move.advance(
            &mut self.base,
            desired_velocity,
            self.turn_rate_degrees,
            reverse,
            dt,
        );
    }

    /// Current per-unit ground movement phase owned by the squad action.
    #[must_use]
    pub const fn ground_move_phase(&self) -> GroundMovePhase {
        self.ground_move.phase()
    }

    /// Current transformed child-formation target.
    #[must_use]
    pub const fn ground_move_target(&self) -> Option<Vec3> {
        self.ground_move.target()
    }

    pub(crate) const fn ground_move_owns_squad_transform(&self) -> bool {
        self.ground_move.owns_squad_transform()
    }

    pub(crate) const fn has_active_ground_move_action(&self) -> bool {
        self.ground_move.is_active()
    }

    pub(crate) fn hash_ground_move_state(&self, checksum: &mut SyncChecksum) {
        self.ground_move.hash_state(checksum);
    }
}

fn finish_at_target(action: &mut UnitGroundMove, base: &mut BaseEntity, target: Vec3) {
    base.position = target;
    base.velocity = Vec3::ZERO;
    if action.squad_action_working {
        action.phase = GroundMovePhase::Pathing;
    } else {
        action.cancel();
    }
}

fn limit_turn(
    current: Vec3,
    desired: Vec3,
    turn_rate_degrees: f32,
    dt: f32,
    distance_change: f32,
    distance_remaining: f32,
) -> (Vec3, f32) {
    let current = normalized_planar_or(current, desired);
    let desired = normalized_planar_or(desired, current);
    let angle = signed_angle(current, desired);
    let absolute_angle = angle.abs();
    if absolute_angle <= ANGLE_EPSILON {
        return (desired, distance_change);
    }
    let mut turn_speed = finite_nonnegative(turn_rate_degrees).to_radians();
    if turn_speed < ANGLE_EPSILON {
        return (desired, distance_change);
    }
    turn_speed *= turn_speed_factor(absolute_angle);
    let maximum_turn = turn_speed * dt;
    if absolute_angle <= maximum_turn {
        return (desired, distance_change);
    }
    let facing = rotate_planar(current, maximum_turn.copysign(angle));
    let clamp_distance = distance_remaining * maximum_turn / absolute_angle * 0.5;
    (facing, distance_change.min(clamp_distance))
}

fn turn_speed_factor(absolute_angle: f32) -> f32 {
    if absolute_angle < std::f32::consts::PI * 0.03125 {
        0.125
    } else if absolute_angle < std::f32::consts::PI * 0.0625 {
        0.25
    } else if absolute_angle < std::f32::consts::FRAC_PI_8 {
        0.5
    } else if absolute_angle < std::f32::consts::FRAC_PI_4 {
        0.75
    } else {
        1.0
    }
}

fn signed_angle(current: Vec3, desired: Vec3) -> f32 {
    let dot = current.dot(desired).clamp(-1.0, 1.0);
    let cross_y = current.z.mul_add(desired.x, -current.x * desired.z);
    cross_y.atan2(dot)
}

fn rotate_planar(vector: Vec3, angle: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    Vec3::new(
        vector.x.mul_add(cos, vector.z * sin),
        0.0,
        (-vector.x).mul_add(sin, vector.z * cos),
    )
    .normalize_or_zero()
}

fn normalized_planar_or(vector: Vec3, fallback: Vec3) -> Vec3 {
    let direction = planar(vector).normalize_or_zero();
    if direction == Vec3::ZERO {
        planar(fallback).normalize_or_zero()
    } else {
        direction
    }
}

fn planar(vector: Vec3) -> Vec3 {
    Vec3::new(vector.x, 0.0, vector.z)
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn valid_step(dt: f32) -> bool {
    dt.is_finite() && dt > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_turn_limit_rotates_and_clamps_travel() {
        let (forward, distance) = limit_turn(Vec3::Z, Vec3::X, 90.0, 0.1, 1.0, 10.0);
        let expected_angle = 9.0_f32.to_radians();
        assert!((signed_angle(Vec3::Z, forward) - expected_angle).abs() < 0.000_001);
        assert!((distance - 0.5).abs() < 0.000_001);
    }

    #[test]
    fn source_small_angle_bands_reduce_turn_speed() {
        let assert_factor = |angle: f32, expected: f32| {
            assert!((turn_speed_factor(angle.to_radians()) - expected).abs() < f32::EPSILON);
        };
        assert_factor(5.0, 0.125);
        assert_factor(10.0, 0.25);
        assert_factor(20.0, 0.5);
        assert_factor(40.0, 0.75);
        assert_factor(60.0, 1.0);
    }

    #[test]
    fn caught_up_member_enters_pathing_until_squad_finishes() {
        let mut action = UnitGroundMove::default();
        let mut base = BaseEntity::default();
        action.prepare(Vec3::ZERO, 10.0, true);
        action.advance(&mut base, 10.0, 90.0, false, 0.05);
        assert_eq!(action.phase(), GroundMovePhase::Pathing);

        action.prepare(Vec3::ZERO, 0.0, false);
        action.advance(&mut base, 10.0, 90.0, false, 0.05);
        assert_eq!(action.phase(), GroundMovePhase::Inactive);
    }

    #[test]
    fn source_pathing_guard_fails_after_three_retries() {
        let mut action = UnitGroundMove::default();
        let mut base = BaseEntity::default();
        action.prepare(Vec3::ZERO, 10.0, true);
        action.advance(&mut base, 10.0, 90.0, false, 0.05);

        for retry in 1..=MAX_RETRY_ATTEMPTS {
            action.prepare(Vec3::ZERO, 10.0, true);
            action.advance(&mut base, 10.0, 90.0, false, 0.05);
            assert_eq!(action.retry_attempts, retry);
            assert_eq!(action.phase(), GroundMovePhase::Pathing);
        }
        action.prepare(Vec3::ZERO, 10.0, true);
        action.advance(&mut base, 10.0, 90.0, false, 0.05);
        assert_eq!(action.phase(), GroundMovePhase::Failed);
    }

    #[test]
    fn ground_action_state_is_checksummed() {
        let mut inactive = Unit::default();
        let mut moving = inactive.clone();
        moving.prepare_squad_ground_move(Vec3::X, 4.0, true);
        let hash = |unit: &Unit| {
            let mut checksum = SyncChecksum::new();
            unit.hash_ground_move_state(&mut checksum);
            checksum.value()
        };
        assert_ne!(hash(&inactive), hash(&moving));
        inactive.prepare_squad_ground_move(Vec3::X, 4.0, true);
        assert_eq!(hash(&inactive), hash(&moving));
    }
}
