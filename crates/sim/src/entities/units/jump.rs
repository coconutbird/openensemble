//! Unit-owned quadratic flight for voluntary squad Jump orders.

use super::Unit;
use crate::order::JumpOrderType;
use crate::sync::SyncChecksum;
use glam::Vec3;

const MIN_DISTANCE_SQUARED: f32 = 0.000_001;

/// Authoritative phase of one unit's voluntary Jump action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum UnitJumpPhase {
    /// No voluntary Jump action owns this unit.
    #[default]
    Inactive = 0,
    /// The critical member opportunity is connected but has not updated yet.
    Pending = 1,
    /// The unit is moving along its quadratic spline.
    Flying = 2,
}

#[derive(Debug, Clone)]
pub(crate) struct UnitJump {
    phase: UnitJumpPhase,
    kind: JumpOrderType,
    action_name: String,
    target: Vec3,
    target_height: f32,
    coefficient_0: Vec3,
    coefficient_1: Vec3,
    coefficient_2: Vec3,
    planar_distance: f32,
    parameter: f32,
    velocity_scalar: f32,
}

impl Default for UnitJump {
    fn default() -> Self {
        Self {
            phase: UnitJumpPhase::Inactive,
            kind: JumpOrderType::Jump,
            action_name: String::new(),
            target: Vec3::ZERO,
            target_height: 0.0,
            coefficient_0: Vec3::ZERO,
            coefficient_1: Vec3::ZERO,
            coefficient_2: Vec3::ZERO,
            planar_distance: 0.0,
            parameter: 0.0,
            velocity_scalar: 0.0,
        }
    }
}

pub(crate) struct JumpAdvance {
    pub(crate) position: Option<Vec3>,
    pub(crate) complete: bool,
}

impl UnitJump {
    pub(crate) fn begin(
        &mut self,
        kind: JumpOrderType,
        action_name: &str,
        start: Vec3,
        target: Vec3,
        velocity_scalar: f32,
    ) -> bool {
        if self.phase != UnitJumpPhase::Inactive
            || kind == JumpOrderType::Pull
            || !start.is_finite()
            || !target.is_finite()
            || !velocity_scalar.is_finite()
        {
            return false;
        }
        let delta = target - start;
        let planar_distance = Vec3::new(delta.x, 0.0, delta.z).length();
        let mut midpoint = (start + target) * 0.5;
        midpoint.y = start.y.max(target.y) + planar_distance * 0.5;
        let coefficient_2 = ((midpoint - start) - delta * 0.5) / -0.25;
        self.phase = UnitJumpPhase::Pending;
        self.kind = kind;
        action_name.clone_into(&mut self.action_name);
        self.target = target;
        self.target_height = target.y;
        self.coefficient_0 = start;
        self.coefficient_1 = delta - coefficient_2;
        self.coefficient_2 = coefficient_2;
        self.planar_distance = planar_distance;
        self.parameter = 0.0;
        self.velocity_scalar = velocity_scalar;
        true
    }

    pub(crate) fn advance(&mut self, dt: f32) -> JumpAdvance {
        if self.phase == UnitJumpPhase::Pending {
            self.phase = UnitJumpPhase::Flying;
            return JumpAdvance {
                position: None,
                complete: false,
            };
        }
        if self.phase != UnitJumpPhase::Flying || !dt.is_finite() || dt <= 0.0 {
            return JumpAdvance {
                position: None,
                complete: false,
            };
        }
        if self.planar_distance * self.planar_distance <= MIN_DISTANCE_SQUARED {
            self.parameter = 1.0;
        } else {
            self.parameter += dt * (self.velocity_scalar / self.planar_distance);
            if self.parameter >= 1.0 {
                self.parameter = 1.0;
            }
        }
        let position = self.coefficient_2 * self.parameter * self.parameter
            + self.coefficient_1 * self.parameter
            + self.coefficient_0;
        JumpAdvance {
            position: Some(position),
            complete: self.parameter >= 1.0,
        }
    }

    pub(crate) fn cancel(&mut self) {
        *self = Self::default();
    }

    const fn is_active(&self) -> bool {
        !matches!(self.phase, UnitJumpPhase::Inactive)
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.kind as u32);
        hash_string(checksum, &self.action_name);
        hash_vec3(checksum, self.target);
        checksum.hash_f32(self.target_height);
        hash_vec3(checksum, self.coefficient_0);
        hash_vec3(checksum, self.coefficient_1);
        hash_vec3(checksum, self.coefficient_2);
        checksum.hash_f32(self.planar_distance);
        checksum.hash_f32(self.parameter);
        checksum.hash_f32(self.velocity_scalar);
    }
}

impl Unit {
    /// Return whether a voluntary Jump action currently owns this unit.
    #[must_use]
    pub const fn is_jumping(&self) -> bool {
        self.jump.is_active()
    }

    /// Return the authoritative member Jump phase.
    #[must_use]
    pub const fn jump_phase(&self) -> UnitJumpPhase {
        self.jump.phase
    }

    /// Return the member's final formation-adjusted landing position.
    #[must_use]
    pub const fn jump_target(&self) -> Option<Vec3> {
        if self.jump.is_active() {
            Some(self.jump.target)
        } else {
            None
        }
    }

    /// Return normalized spline progress while a Jump is active.
    #[must_use]
    pub const fn jump_progress(&self) -> Option<f32> {
        if self.jump.is_active() {
            Some(self.jump.parameter)
        } else {
            None
        }
    }

    pub(crate) fn begin_jump_action(
        &mut self,
        kind: JumpOrderType,
        action_name: &str,
        target: Vec3,
        velocity_scalar: f32,
    ) -> bool {
        self.jump.begin(
            kind,
            action_name,
            self.base.position,
            target,
            velocity_scalar,
        )
    }

    pub(crate) fn advance_jump_action(&mut self, dt: f32) -> JumpAdvance {
        self.jump.advance(dt)
    }

    pub(crate) fn cancel_jump_action(&mut self) {
        self.jump.cancel();
    }

    pub(crate) fn hash_jump_state(&self, checksum: &mut SyncChecksum) {
        self.jump.hash_state(checksum);
    }
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}

#[cfg(test)]
mod tests;
