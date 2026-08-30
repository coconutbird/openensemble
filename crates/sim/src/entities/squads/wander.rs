//! Persistent squad wander state owned by the deterministic simulation.

use super::Squad;
use crate::sync::SyncChecksum;
use glam::Vec3;

const RETAIL_WAIT_SECONDS: f32 = 5.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum WanderPhase {
    #[default]
    Disconnected = 0,
    Ready = 1,
    Moving = 2,
    Waiting = 3,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SquadWander {
    phase: WanderPhase,
    origin: Vec3,
    target: Option<Vec3>,
    movement_target: Option<Vec3>,
    wait_time: f32,
}

impl SquadWander {
    pub(crate) const fn phase(self) -> WanderPhase {
        self.phase
    }

    pub(crate) const fn is_initialized(self) -> bool {
        !matches!(self.phase, WanderPhase::Disconnected)
    }

    pub(crate) const fn origin(self) -> Vec3 {
        self.origin
    }

    pub(crate) const fn target(self) -> Option<Vec3> {
        self.target
    }

    pub(crate) const fn wait_time(self) -> f32 {
        self.wait_time
    }

    pub(crate) fn initialize(&mut self, origin: Vec3) {
        if self.is_initialized() || !origin.is_finite() {
            return;
        }
        self.phase = WanderPhase::Ready;
        self.origin = origin;
    }

    pub(crate) fn advance(&mut self, dt: f32, movement_finished: bool) -> bool {
        match self.phase {
            WanderPhase::Ready => true,
            WanderPhase::Moving => {
                self.phase = WanderPhase::Waiting;
                self.wait_time = 0.0;
                false
            }
            WanderPhase::Waiting if movement_finished => true,
            WanderPhase::Waiting if dt.is_finite() && dt > 0.0 => {
                self.wait_time += dt;
                self.wait_time > RETAIL_WAIT_SECONDS
            }
            WanderPhase::Disconnected | WanderPhase::Waiting => false,
        }
    }

    pub(crate) fn begin_move(&mut self, target: Vec3, movement_target: Option<Vec3>) {
        self.phase = WanderPhase::Moving;
        self.target = Some(target);
        self.movement_target = movement_target;
        self.wait_time = 0.0;
    }

    pub(crate) fn disconnect(&mut self) -> Option<Vec3> {
        let movement_target = self.movement_target;
        *self = Self::default();
        movement_target
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_vec3(self.origin.x, self.origin.y, self.origin.z);
        hash_optional_vec3(checksum, self.target);
        hash_optional_vec3(checksum, self.movement_target);
        checksum.hash_f32(self.wait_time);
    }
}

impl Squad {
    /// Whether retail's persistent wander action is currently cycling.
    #[must_use]
    pub const fn is_wandering(&self) -> bool {
        matches!(
            self.wander.phase(),
            WanderPhase::Moving | WanderPhase::Waiting
        )
    }

    /// Connection-time center around which this squad chooses wander targets.
    #[must_use]
    pub const fn wander_origin(&self) -> Option<Vec3> {
        if self.wander.is_initialized() {
            Some(self.wander.origin())
        } else {
            None
        }
    }

    /// Latest source-selected wander target, before movement-range adjustment.
    #[must_use]
    pub const fn wander_target(&self) -> Option<Vec3> {
        self.wander.target()
    }

    /// Time spent in retail's five-second wait state.
    #[must_use]
    pub const fn wander_wait_time(&self) -> f32 {
        self.wander.wait_time()
    }
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_vec3(value.x, value.y, value.z);
    } else {
        checksum.hash_u32(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_transitions_to_wait_and_uses_strict_five_second_timeout() {
        let mut wander = SquadWander::default();
        wander.initialize(Vec3::new(1.0, 2.0, 3.0));
        assert!(wander.advance(0.05, false));
        wander.begin_move(Vec3::X, Some(Vec3::X * 2.0));
        assert!(!wander.advance(0.05, false));
        assert!(!wander.advance(5.0, false));
        assert!(wander.advance(0.01, false));
        assert_eq!(wander.disconnect(), Some(Vec3::X * 2.0));
        assert!(!wander.is_initialized());
    }
}
