//! Squad ownership of one retail capture work order.

use super::{Squad, SquadState};
use crate::entities::CapturePhase;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Default)]
pub(crate) struct SquadCapture {
    target_id: Option<EntityId>,
    player_id: Option<PlayerId>,
    phase: CapturePhase,
    used_second_approach: bool,
}

impl SquadCapture {
    fn start(&mut self, player_id: PlayerId, target_id: EntityId) {
        self.target_id = Some(target_id);
        self.player_id = Some(player_id);
        self.phase = CapturePhase::Moving;
        self.used_second_approach = false;
    }

    pub(crate) fn cancel(&mut self) {
        self.target_id = None;
        self.player_id = None;
        self.phase = CapturePhase::None;
        self.used_second_approach = false;
    }

    pub(crate) fn finish(&mut self, phase: CapturePhase) {
        self.phase = phase;
    }

    pub(crate) fn set_phase(&mut self, phase: CapturePhase) {
        self.phase = phase;
    }

    pub(crate) fn begin_second_approach(&mut self) -> bool {
        if self.used_second_approach {
            return false;
        }
        self.used_second_approach = true;
        self.phase = CapturePhase::Moving;
        true
    }

    pub(crate) const fn target_id(&self) -> Option<EntityId> {
        self.target_id
    }

    pub(crate) const fn player_id(&self) -> Option<PlayerId> {
        self.player_id
    }

    pub(crate) const fn phase(&self) -> CapturePhase {
        self.phase
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            self.target_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_u32(self.player_id.map_or(u32::MAX, u32::from));
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(u32::from(self.used_second_approach));
    }
}

impl Squad {
    pub(crate) fn begin_capture(&mut self, player_id: PlayerId, target_id: EntityId) {
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
        self.gather.cancel();
        self.cancel_idle_action();
        self.move_target = None;
        self.base.velocity = glam::Vec3::ZERO;
        self.capture.start(player_id, target_id);
        self.state = SquadState::Working;
    }

    /// Return the target unit of the current capture work order.
    #[must_use]
    pub const fn capture_target(&self) -> Option<EntityId> {
        self.capture.target_id()
    }

    /// Return the current squad capture phase.
    #[must_use]
    pub const fn capture_phase(&self) -> CapturePhase {
        self.capture.phase()
    }

    /// Return whether this squad currently contributes capture points.
    #[must_use]
    pub const fn is_capturing(&self) -> bool {
        matches!(self.capture.phase(), CapturePhase::Working)
    }

    pub(crate) fn hash_capture_state(&self, checksum: &mut SyncChecksum) {
        self.capture.hash_state(checksum);
    }
}
