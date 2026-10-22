//! Per-unit animation action for a squad-mode transition.

use super::{Unit, UnitState};
use crate::entities::SquadMode;
use crate::entity::Entity;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Authoritative phase of one unit's `ChangeMode` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitChangeModePhase {
    /// The unit owns no mode-change controllers.
    #[default]
    Inactive,
    /// The authored squad-mode animation is playing.
    Working,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UnitChangeMode {
    phase: UnitChangeModePhase,
    target_mode: SquadMode,
    animation_type: String,
    remaining_seconds: f32,
}

impl UnitChangeMode {
    fn begin(
        &mut self,
        target_mode: SquadMode,
        animation_type: Option<&str>,
        duration_seconds: f32,
    ) -> bool {
        self.cancel();
        let Some(animation_type) = animation_type
            .map(str::trim)
            .filter(|animation| !animation.is_empty())
        else {
            return false;
        };
        if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
            return false;
        }
        self.phase = UnitChangeModePhase::Working;
        self.target_mode = target_mode;
        animation_type.clone_into(&mut self.animation_type);
        self.remaining_seconds = duration_seconds;
        true
    }

    fn advance(&mut self, elapsed: f32) -> bool {
        if self.phase == UnitChangeModePhase::Inactive || !elapsed.is_finite() || elapsed <= 0.0 {
            return false;
        }
        self.remaining_seconds = (self.remaining_seconds - elapsed).max(0.0);
        if self.remaining_seconds > 0.0 {
            return false;
        }
        self.cancel();
        true
    }

    fn cancel(&mut self) {
        *self = Self::default();
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.target_mode as u32);
        checksum.hash_u32(u32::try_from(self.animation_type.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.animation_type.as_bytes());
        checksum.hash_f32(self.remaining_seconds);
    }
}

impl Unit {
    /// Return the active per-unit mode-change phase.
    #[must_use]
    pub const fn change_mode_phase(&self) -> UnitChangeModePhase {
        self.change_mode.phase
    }

    /// Return seconds remaining on the active mode animation.
    #[must_use]
    pub const fn change_mode_remaining(&self) -> f32 {
        self.change_mode.remaining_seconds
    }

    pub(crate) fn begin_change_mode(
        &mut self,
        target_mode: SquadMode,
        animation_type: Option<&str>,
        duration_seconds: f32,
    ) -> bool {
        self.clear_attack_order();
        self.cancel_squad_ground_move();
        self.cancel_gather_action();
        self.cancel_capture_action();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        self.cancel_idle_action();
        self.change_mode
            .begin(target_mode, animation_type, duration_seconds)
    }

    pub(crate) fn advance_change_mode(&mut self, elapsed: f32) -> bool {
        let completed = self.change_mode.advance(elapsed);
        if completed && self.is_alive() {
            self.state = UnitState::Idle;
            self.base.velocity = Vec3::ZERO;
            self.cancel_idle_action();
        }
        completed
    }

    pub(crate) fn cancel_change_mode(&mut self) {
        self.change_mode.cancel();
    }

    pub(crate) fn hash_change_mode_state(&self, checksum: &mut SyncChecksum) {
        self.change_mode.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_duration_controls_the_unit_action() {
        let mut unit = Unit::default();
        assert!(unit.begin_change_mode(SquadMode::Lockdown, Some("AnchorDown"), 0.1));
        assert_eq!(unit.change_mode_phase(), UnitChangeModePhase::Working);
        assert!(!unit.advance_change_mode(0.05));
        assert!(unit.advance_change_mode(0.05));
        assert_eq!(unit.change_mode_phase(), UnitChangeModePhase::Inactive);
    }

    #[test]
    fn missing_or_zero_length_animation_completes_immediately() {
        let mut unit = Unit::default();
        assert!(!unit.begin_change_mode(SquadMode::Lockdown, None, 1.0));
        assert!(!unit.begin_change_mode(SquadMode::Lockdown, Some("AnchorDown"), 0.0));
        assert_eq!(unit.change_mode_phase(), UnitChangeModePhase::Inactive);
    }
}
