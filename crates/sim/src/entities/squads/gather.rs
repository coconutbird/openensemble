//! Squad ownership of one retail gather work order.

use super::{Squad, SquadState};
use crate::entities::GatherPhase;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Default)]
pub(crate) struct SquadGather {
    target_id: Option<EntityId>,
    phase: GatherPhase,
}

impl SquadGather {
    fn start(&mut self, target_id: EntityId) {
        self.target_id = Some(target_id);
        self.phase = GatherPhase::Moving;
    }

    pub(crate) fn cancel(&mut self) {
        self.target_id = None;
        self.phase = GatherPhase::None;
    }

    pub(crate) const fn target_id(&self) -> Option<EntityId> {
        self.target_id
    }

    pub(crate) const fn phase(&self) -> GatherPhase {
        self.phase
    }

    pub(crate) fn set_phase(&mut self, phase: GatherPhase) {
        self.phase = phase;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            self.target_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_u32(self.phase as u32);
    }
}

impl Squad {
    pub(crate) fn begin_gather(&mut self, target_id: EntityId) {
        self.garrison.cancel_pending();
        self.cancel_scripted_move_orders();
        self.attack_target = None;
        self.clear_experience_bank();
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.ability_used_unit_ids.clear();
        self.join.cancel();
        self.mines.cancel();
        self.detonate.cancel();
        self.capture.cancel();
        self.cancel_idle_action();
        self.move_target = None;
        self.base.velocity = glam::Vec3::ZERO;
        self.gather.start(target_id);
        self.state = SquadState::Working;
    }

    /// Return the target of the current gather work order.
    #[must_use]
    pub const fn gather_target(&self) -> Option<EntityId> {
        self.gather.target_id()
    }

    /// Return the current squad gather phase.
    #[must_use]
    pub const fn gather_phase(&self) -> GatherPhase {
        self.gather.phase()
    }

    /// Return whether at least one member can currently gather the target.
    #[must_use]
    pub const fn is_gathering(&self) -> bool {
        matches!(self.gather.phase(), GatherPhase::Working)
    }

    pub(crate) fn hash_gather_state(&self, checksum: &mut SyncChecksum) {
        self.gather.hash_state(checksum);
    }
}
