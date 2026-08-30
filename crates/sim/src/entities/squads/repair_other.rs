//! Squad ownership of one retail `RepairOther` action.

use super::{Squad, SquadState};
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// Authoritative phase of a targeted squad repair action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepairOtherPhase {
    /// No repair order is connected.
    #[default]
    None,
    /// The source squad is approaching the target.
    Moving,
    /// The source squad is actively restoring target combat value.
    Working,
    /// The repair action completed normally.
    Done,
    /// The order could not remain connected to its target or action.
    Failed,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SquadRepairOther {
    target_id: Option<EntityId>,
    source_player_id: Option<PlayerId>,
    action_name: String,
    ability_id: Option<u8>,
    phase: RepairOtherPhase,
    effect_id: Option<EntityId>,
    beam_head_id: Option<EntityId>,
    beam_tail_id: Option<EntityId>,
}

impl SquadRepairOther {
    fn start(
        &mut self,
        source_player_id: PlayerId,
        target_id: EntityId,
        action_name: &str,
        ability_id: Option<u8>,
    ) {
        self.target_id = Some(target_id);
        self.source_player_id = Some(source_player_id);
        self.action_name.clear();
        self.action_name.push_str(action_name);
        self.ability_id = ability_id;
        self.phase = RepairOtherPhase::Moving;
        self.effect_id = None;
        self.beam_head_id = None;
        self.beam_tail_id = None;
    }

    pub(crate) fn cancel(&mut self) {
        self.target_id = None;
        self.source_player_id = None;
        self.action_name.clear();
        self.ability_id = None;
        self.phase = RepairOtherPhase::None;
        self.effect_id = None;
        self.beam_head_id = None;
        self.beam_tail_id = None;
    }

    pub(crate) fn finish(&mut self, phase: RepairOtherPhase) {
        self.phase = phase;
    }

    pub(crate) fn set_phase(&mut self, phase: RepairOtherPhase) {
        self.phase = phase;
    }

    pub(crate) const fn target_id(&self) -> Option<EntityId> {
        self.target_id
    }

    pub(crate) const fn source_player_id(&self) -> Option<PlayerId> {
        self.source_player_id
    }

    pub(crate) fn action_name(&self) -> Option<&str> {
        (!self.action_name.is_empty()).then_some(self.action_name.as_str())
    }

    pub(crate) const fn ability_id(&self) -> Option<u8> {
        self.ability_id
    }

    pub(crate) const fn phase(&self) -> RepairOtherPhase {
        self.phase
    }

    pub(crate) const fn effect_id(&self) -> Option<EntityId> {
        self.effect_id
    }

    pub(crate) const fn beam_head_id(&self) -> Option<EntityId> {
        self.beam_head_id
    }

    pub(crate) const fn beam_tail_id(&self) -> Option<EntityId> {
        self.beam_tail_id
    }

    pub(crate) fn set_effect_ids(
        &mut self,
        effect_id: EntityId,
        beam_head_id: Option<EntityId>,
        beam_tail_id: Option<EntityId>,
    ) {
        self.effect_id = Some(effect_id);
        self.beam_head_id = beam_head_id;
        self.beam_tail_id = beam_tail_id;
    }

    pub(crate) fn take_effect_ids(&mut self) -> Vec<EntityId> {
        [
            self.effect_id.take(),
            self.beam_head_id.take(),
            self.beam_tail_id.take(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            self.target_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_u32(self.source_player_id.map_or(u32::MAX, u32::from));
        checksum.hash_u32(u32::try_from(self.action_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.action_name.as_bytes());
        checksum.hash_u32(self.ability_id.map_or(u32::MAX, u32::from));
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.effect_id.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.beam_head_id.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.beam_tail_id.map_or(u32::MAX, EntityId::as_u32));
    }
}

impl Squad {
    pub(crate) fn begin_repair_other(
        &mut self,
        source_player_id: PlayerId,
        target_id: EntityId,
        action_name: &str,
        ability_id: Option<u8>,
    ) {
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
        self.capture.cancel();
        self.cancel_idle_action();
        self.move_target = None;
        self.base.velocity = glam::Vec3::ZERO;
        self.repair_other
            .start(source_player_id, target_id, action_name, ability_id);
        self.state = SquadState::Working;
    }

    /// Return the canonical target squad of the current repair order.
    #[must_use]
    pub const fn repair_other_target(&self) -> Option<EntityId> {
        self.repair_other.target_id()
    }

    /// Return the selected tactic action for the current repair order.
    #[must_use]
    pub fn repair_other_action(&self) -> Option<&str> {
        self.repair_other.action_name()
    }

    /// Ability database index carried by the active repair order, if any.
    #[must_use]
    pub const fn repair_other_ability_id(&self) -> Option<u8> {
        self.repair_other.ability_id()
    }

    /// Return the current targeted-repair phase.
    #[must_use]
    pub const fn repair_other_phase(&self) -> RepairOtherPhase {
        self.repair_other.phase()
    }

    /// Return whether this squad is currently applying repair work.
    #[must_use]
    pub const fn is_repairing_other(&self) -> bool {
        matches!(self.repair_other.phase(), RepairOtherPhase::Working)
    }

    /// Main sim-owned visual entity belonging to the active repair action.
    #[must_use]
    pub const fn repair_other_effect_id(&self) -> Option<EntityId> {
        self.repair_other.effect_id()
    }

    /// Optional sim-owned source endpoint of an explicit repair beam.
    #[must_use]
    pub const fn repair_other_beam_head_id(&self) -> Option<EntityId> {
        self.repair_other.beam_head_id()
    }

    /// Optional sim-owned target endpoint of an explicit repair beam.
    #[must_use]
    pub const fn repair_other_beam_tail_id(&self) -> Option<EntityId> {
        self.repair_other.beam_tail_id()
    }

    pub(crate) fn hash_repair_other_state(&self, checksum: &mut SyncChecksum) {
        self.repair_other.hash_state(checksum);
    }
}
