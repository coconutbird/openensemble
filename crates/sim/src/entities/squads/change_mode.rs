//! Uninterruptible retail squad-mode transition state.

use super::{Squad, SquadMode, SquadState};
use crate::entity::Entity;
use crate::sync::SyncChecksum;

/// Mode context retained by a movement or attack order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SquadModeOrder {
    target_mode: SquadMode,
    ability_id: Option<u8>,
}

impl SquadModeOrder {
    pub(crate) const fn new(target_mode: SquadMode, ability_id: Option<u8>) -> Self {
        Self {
            target_mode,
            ability_id,
        }
    }

    pub(crate) const fn target_mode(self) -> SquadMode {
        self.target_mode
    }

    pub(crate) const fn ability_id(self) -> Option<u8> {
        self.ability_id
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.target_mode as u32);
        checksum.hash_u32(self.ability_id.map_or(u32::MAX, u32::from));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SquadChangeModeResume {
    #[default]
    None,
    ActiveMove,
    NextMove,
    Attack,
}

/// Authoritative phase of a squad `ChangeMode` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SquadChangeModePhase {
    /// No transition owns the squad.
    #[default]
    Inactive,
    /// The squad action is connected but has not issued member opportunities.
    Pending,
    /// Member unit actions are running.
    Working,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SquadChangeMode {
    phase: SquadChangeModePhase,
    target_mode: SquadMode,
    ability_id: Option<u8>,
    resume: SquadChangeModeResume,
    pending_move_relock: Option<SquadModeOrder>,
    attack_order: Option<SquadModeOrder>,
}

impl SquadChangeMode {
    fn begin(
        &mut self,
        target_mode: SquadMode,
        ability_id: Option<u8>,
        resume: SquadChangeModeResume,
    ) -> bool {
        if self.phase != SquadChangeModePhase::Inactive {
            return false;
        }
        self.phase = SquadChangeModePhase::Pending;
        self.target_mode = target_mode;
        self.ability_id = ability_id;
        self.resume = resume;
        true
    }

    pub(crate) fn mark_working(&mut self) {
        if self.phase == SquadChangeModePhase::Pending {
            self.phase = SquadChangeModePhase::Working;
        }
    }

    fn clear_transition(&mut self) {
        self.phase = SquadChangeModePhase::Inactive;
        self.target_mode = SquadMode::Normal;
        self.ability_id = None;
        self.resume = SquadChangeModeResume::None;
    }

    fn cancel_all(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn set_attack_order(&mut self, order: Option<SquadModeOrder>) {
        self.attack_order = order;
    }

    pub(crate) const fn attack_order(&self) -> Option<SquadModeOrder> {
        self.attack_order
    }

    pub(crate) fn schedule_move_relock(&mut self, order: SquadModeOrder) {
        self.pending_move_relock = Some(order);
    }

    pub(crate) const fn pending_move_relock(&self) -> Option<SquadModeOrder> {
        self.pending_move_relock
    }

    pub(crate) fn clear_pending_move_relock(&mut self) {
        self.pending_move_relock = None;
    }

    pub(crate) fn clear_order_automation(&mut self) {
        self.pending_move_relock = None;
        self.attack_order = None;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.target_mode as u32);
        checksum.hash_u32(self.ability_id.map_or(u32::MAX, u32::from));
        checksum.hash_u32(self.resume as u32);
        hash_optional_mode_order(checksum, self.pending_move_relock);
        hash_optional_mode_order(checksum, self.attack_order);
    }
}

impl Squad {
    /// Return the active squad-mode transition phase.
    #[must_use]
    pub const fn change_mode_phase(&self) -> SquadChangeModePhase {
        self.change_mode.phase
    }

    /// Return whether a mode-change action currently owns the squad.
    #[must_use]
    pub const fn is_changing_mode(&self) -> bool {
        !matches!(self.change_mode.phase, SquadChangeModePhase::Inactive)
    }

    /// Return retail's current-or-changing mode used by command validation.
    #[must_use]
    pub const fn current_or_changing_mode(&self) -> SquadMode {
        if self.is_changing_mode() {
            self.change_mode.target_mode
        } else {
            self.mode
        }
    }

    /// Return the target of the active transition.
    #[must_use]
    pub const fn changing_to_mode(&self) -> Option<SquadMode> {
        if self.is_changing_mode() {
            Some(self.change_mode.target_mode)
        } else {
            None
        }
    }

    /// Return whether an ordinary user movement order can run now.
    #[must_use]
    pub const fn can_accept_movement_order(&self) -> bool {
        !self.is_changing_mode()
            && !matches!(self.mode, SquadMode::StandGround | SquadMode::Lockdown)
    }

    pub(crate) const fn can_chase_attack_target(&self) -> bool {
        !self.is_changing_mode()
            && !matches!(self.mode, SquadMode::StandGround | SquadMode::Lockdown)
    }

    pub(crate) fn begin_change_mode(
        &mut self,
        target_mode: SquadMode,
        ability_id: Option<u8>,
    ) -> bool {
        self.begin_change_mode_with_resume(target_mode, ability_id, SquadChangeModeResume::None)
    }

    pub(crate) fn begin_change_mode_with_resume(
        &mut self,
        target_mode: SquadMode,
        ability_id: Option<u8>,
        resume: SquadChangeModeResume,
    ) -> bool {
        if !self.is_alive()
            || target_mode == self.mode
            || !self.change_mode.begin(target_mode, ability_id, resume)
        {
            return false;
        }
        self.move_target = None;
        self.base.velocity = glam::Vec3::ZERO;
        self.state = SquadState::Working;
        self.cancel_idle_action();
        true
    }

    pub(crate) fn finish_change_mode(&mut self) -> Option<(SquadMode, Option<u8>)> {
        if !self.is_changing_mode() {
            return None;
        }
        let result = (self.change_mode.target_mode, self.change_mode.ability_id);
        let resume = self.change_mode.resume;
        self.mode = self.change_mode.target_mode;
        self.change_mode.clear_transition();
        self.move_target = None;
        self.base.velocity = glam::Vec3::ZERO;
        if self.is_alive() {
            self.state = SquadState::Idle;
            match resume {
                SquadChangeModeResume::None => {}
                SquadChangeModeResume::ActiveMove => {
                    let _resumed = self.resume_active_move();
                }
                SquadChangeModeResume::NextMove => {
                    let _resumed = self.begin_next_queued_move();
                }
                SquadChangeModeResume::Attack => {
                    if self.attack_target.is_some() {
                        self.state = SquadState::Attacking;
                    }
                }
            }
        }
        self.cancel_idle_action();
        Some(result)
    }

    pub(crate) fn cancel_change_mode(&mut self) {
        self.change_mode.cancel_all();
    }

    pub(crate) fn set_attack_mode_order(&mut self, order: Option<SquadModeOrder>) {
        self.change_mode.set_attack_order(order);
    }

    pub(crate) const fn attack_mode_order(&self) -> Option<SquadModeOrder> {
        self.change_mode.attack_order()
    }

    pub(crate) const fn pending_move_relock(&self) -> Option<SquadModeOrder> {
        self.change_mode.pending_move_relock()
    }

    pub(crate) fn clear_pending_move_relock(&mut self) {
        self.change_mode.clear_pending_move_relock();
    }

    pub(crate) fn clear_mode_order_automation(&mut self) {
        self.change_mode.clear_order_automation();
    }

    /// Persistent movement scalar contributed by the active squad mode.
    #[must_use]
    pub const fn mode_movement_modifier(&self) -> f32 {
        self.mode_movement_modifier
    }

    pub(crate) fn apply_mode_movement_modifier(&mut self, modifier: Option<f32>) {
        if self.mode == SquadMode::Normal {
            self.mode_movement_modifier = 1.0;
        } else if let Some(modifier) = modifier.filter(|value| value.is_finite() && *value > 0.0) {
            self.mode_movement_modifier = modifier;
        }
    }

    pub(crate) fn hash_change_mode_state(&self, checksum: &mut SyncChecksum) {
        self.change_mode.hash_state(checksum);
        checksum.hash_f32(self.mode_movement_modifier);
    }
}

fn hash_optional_mode_order(checksum: &mut SyncChecksum, order: Option<SquadModeOrder>) {
    checksum.hash_u32(u32::from(order.is_some()));
    if let Some(order) = order {
        order.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;

    #[test]
    fn current_or_changing_mode_switches_at_connect_time() {
        let mut squad = Squad::new(EntityId::default(), 1);
        assert!(squad.begin_change_mode(SquadMode::Lockdown, Some(3)));
        assert_eq!(squad.mode, SquadMode::Normal);
        assert_eq!(squad.current_or_changing_mode(), SquadMode::Lockdown);
        assert!(!squad.can_accept_movement_order());

        assert_eq!(
            squad.finish_change_mode(),
            Some((SquadMode::Lockdown, Some(3)))
        );
        assert_eq!(squad.mode, SquadMode::Lockdown);
        assert_eq!(squad.change_mode_phase(), SquadChangeModePhase::Inactive);
    }
}
