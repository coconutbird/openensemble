//! Checksummed squad state for retail's targeted Detonate work action.

use super::{Squad, SquadMode, SquadState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Observable phase of an authoritative squad Detonate action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SquadDetonatePhase {
    /// No Detonate action exists.
    #[default]
    Inactive = 0,
    /// The squad is moving toward the target.
    Moving = 1,
    /// The squad has entered tactic state zero and continues closing range.
    Glowing = 2,
    /// Members may create immediate unit Detonate opportunities on contact.
    Attacking = 3,
}

/// Immutable command context retained by a squad Detonate action.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DetonateOrder {
    pub(crate) target_id: EntityId,
    pub(crate) requested_ability_id: Option<u8>,
}

/// Persistent retail `BSquadActionDetonate` state.
#[derive(Debug, Clone, Default)]
pub(crate) struct SquadDetonate {
    order: Option<DetonateOrder>,
    phase: SquadDetonatePhase,
    action_name: Option<String>,
    recovery_started: bool,
}

impl SquadDetonate {
    fn begin(&mut self, order: DetonateOrder) {
        self.order = Some(order);
        self.phase = SquadDetonatePhase::Moving;
        self.action_name = None;
        self.recovery_started = false;
    }

    pub(crate) const fn order(&self) -> Option<DetonateOrder> {
        self.order
    }

    pub(crate) const fn phase(&self) -> SquadDetonatePhase {
        self.phase
    }

    pub(crate) fn action_name(&self) -> Option<&str> {
        self.action_name.as_deref()
    }

    pub(crate) fn select_action(&mut self, action_name: &str) {
        self.action_name = Some(action_name.to_owned());
    }

    pub(crate) fn enter_glowing(&mut self) {
        self.phase = SquadDetonatePhase::Glowing;
    }

    pub(crate) fn enter_attacking(&mut self) {
        self.phase = SquadDetonatePhase::Attacking;
    }

    pub(crate) const fn recovery_started(&self) -> bool {
        self.recovery_started
    }

    pub(crate) fn mark_recovery_started(&mut self) {
        self.recovery_started = true;
    }

    pub(super) fn cancel(&mut self) {
        self.order = None;
        self.phase = SquadDetonatePhase::Inactive;
        self.action_name = None;
        self.recovery_started = false;
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        if let Some(order) = self.order {
            checksum.hash_u32(1);
            checksum.hash_u32(order.target_id.as_u32());
            checksum.hash_u32(order.requested_ability_id.map_or(u32::MAX, u32::from));
        } else {
            checksum.hash_u32(0);
        }
        if let Some(action_name) = &self.action_name {
            checksum.hash_u32(1);
            checksum.hash_u32(u32::try_from(action_name.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(action_name.as_bytes());
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::from(self.recovery_started));
    }
}

impl Squad {
    /// Return whether this squad owns a targeted Detonate work action.
    #[must_use]
    pub fn is_detonating(&self) -> bool {
        self.detonate.order().is_some()
    }

    /// Return the current authoritative Detonate phase.
    #[must_use]
    pub const fn detonate_phase(&self) -> SquadDetonatePhase {
        self.detonate.phase()
    }

    /// Return the target retained by the active Detonate order.
    #[must_use]
    pub fn detonate_target(&self) -> Option<EntityId> {
        self.detonate.order().map(|order| order.target_id)
    }

    /// Return the Detonate tactic action selected for this squad.
    #[must_use]
    pub fn detonate_action_name(&self) -> Option<&str> {
        self.detonate.action_name()
    }

    pub(crate) fn begin_detonate_order(
        &mut self,
        order: DetonateOrder,
        target_position: Vec3,
    ) -> bool {
        if !self.is_alive()
            || !self.base.is_mobile()
            || self.garrison.is_garrisoned()
            || self.unit_ids.is_empty()
            || order.target_id.is_invalid()
            || !target_position.is_finite()
        {
            return false;
        }
        self.remove_all_orders();
        self.detonate.begin(order);
        self.mode = SquadMode::Normal;
        self.move_target = Some(target_position);
        self.state = SquadState::Moving;
        self.cancel_idle_action();
        true
    }

    pub(crate) fn follow_detonate_target(&mut self, target_position: Vec3) {
        if self.is_detonating() && self.detonate.phase() != SquadDetonatePhase::Attacking {
            self.move_target = Some(target_position);
            self.state = SquadState::Moving;
        }
    }

    pub(crate) fn enter_detonate_attacking(&mut self, target_position: Vec3) {
        self.detonate.enter_attacking();
        self.base.position = target_position;
        self.base.velocity = Vec3::ZERO;
        self.move_target = None;
        self.state = SquadState::Attacking;
        self.mode = SquadMode::HitAndRun;
        self.cancel_idle_action();
    }

    pub(crate) fn finish_detonate_order(&mut self) {
        self.detonate.cancel();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        self.mode = SquadMode::Normal;
        if self.is_alive() {
            self.state = SquadState::Idle;
        }
        self.cancel_idle_action();
    }

    pub(crate) fn hash_detonate_state(&self, checksum: &mut SyncChecksum) {
        self.detonate.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn lifecycle_is_explicit_and_clears_hit_and_run_mode() {
        let mut squad = Squad::new(EntityId::new(EntityClass::Squad, 1), 1);
        squad.unit_ids.push(EntityId::new(EntityClass::Unit, 1));
        let target = EntityId::new(EntityClass::Squad, 2);
        assert!(squad.begin_detonate_order(
            DetonateOrder {
                target_id: target,
                requested_ability_id: Some(3),
            },
            Vec3::X,
        ));
        assert_eq!(squad.detonate_phase(), SquadDetonatePhase::Moving);
        squad.detonate.select_action("SuicideBomb");
        squad.detonate.enter_glowing();
        assert_eq!(squad.detonate_phase(), SquadDetonatePhase::Glowing);
        squad.enter_detonate_attacking(Vec3::X);
        assert_eq!(squad.mode, SquadMode::HitAndRun);
        squad.finish_detonate_order();
        assert_eq!(squad.detonate_phase(), SquadDetonatePhase::Inactive);
        assert_eq!(squad.mode, SquadMode::Normal);
        assert_eq!(squad.state, SquadState::Idle);
    }
}
