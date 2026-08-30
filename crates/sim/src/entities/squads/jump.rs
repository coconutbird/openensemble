//! Uninterruptible squad ownership for voluntary Jump orders.

use super::Squad;
use crate::entity_id::EntityId;
use crate::order::JumpOrderType;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Authoritative phase of one voluntary squad Jump action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SquadJumpPhase {
    /// No voluntary Jump action owns this squad.
    #[default]
    Inactive = 0,
    /// The squad action is connected and member opportunities are pending.
    Pending = 1,
    /// At least one member Jump action is in flight.
    Flying = 2,
}

#[derive(Debug, Clone)]
pub(crate) struct SquadJump {
    phase: SquadJumpPhase,
    kind: JumpOrderType,
    action_name: String,
    target_id: EntityId,
    target_anchor: Vec3,
    ability_id: Option<u8>,
    members: Vec<EntityId>,
}

impl Default for SquadJump {
    fn default() -> Self {
        Self {
            phase: SquadJumpPhase::Inactive,
            kind: JumpOrderType::Jump,
            action_name: String::new(),
            target_id: EntityId::INVALID,
            target_anchor: Vec3::ZERO,
            ability_id: None,
            members: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SquadJumpCompletion {
    pub(crate) kind: JumpOrderType,
    pub(crate) target_id: Option<EntityId>,
    pub(crate) ability_id: Option<u8>,
}

impl SquadJump {
    pub(crate) fn begin(
        &mut self,
        kind: JumpOrderType,
        action_name: &str,
        target_id: Option<EntityId>,
        target_anchor: Vec3,
        ability_id: Option<u8>,
        mut members: Vec<EntityId>,
    ) -> bool {
        if self.phase != SquadJumpPhase::Inactive
            || kind == JumpOrderType::Pull
            || !target_anchor.is_finite()
            || members.is_empty()
        {
            return false;
        }
        members.sort_unstable();
        members.dedup();
        self.phase = SquadJumpPhase::Pending;
        self.kind = kind;
        action_name.clone_into(&mut self.action_name);
        self.target_id = target_id.unwrap_or(EntityId::INVALID);
        self.target_anchor = target_anchor;
        self.ability_id = ability_id;
        self.members = members;
        true
    }

    pub(crate) fn activate(&mut self) -> bool {
        if self.phase != SquadJumpPhase::Pending {
            return false;
        }
        self.phase = SquadJumpPhase::Flying;
        true
    }

    pub(crate) fn retain_members(&mut self, mut keep: impl FnMut(EntityId) -> bool) {
        self.members.retain(|member| keep(*member));
    }

    pub(crate) fn completion(&mut self) -> Option<SquadJumpCompletion> {
        if self.phase == SquadJumpPhase::Inactive || !self.members.is_empty() {
            return None;
        }
        let completion = SquadJumpCompletion {
            kind: self.kind,
            target_id: self.target_id(),
            ability_id: self.ability_id,
        };
        self.cancel();
        Some(completion)
    }

    pub(crate) fn cancel(&mut self) {
        *self = Self::default();
    }

    pub(crate) const fn members(&self) -> &[EntityId] {
        self.members.as_slice()
    }

    pub(crate) fn target_id(&self) -> Option<EntityId> {
        if self.target_id.is_invalid() {
            None
        } else {
            Some(self.target_id)
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.kind as u32);
        hash_string(checksum, &self.action_name);
        checksum.hash_u32(self.target_id.as_u32());
        checksum.hash_vec3(
            self.target_anchor.x,
            self.target_anchor.y,
            self.target_anchor.z,
        );
        checksum.hash_u32(self.ability_id.map_or(u32::MAX, u32::from));
        checksum.hash_u32(u32::try_from(self.members.len()).unwrap_or(u32::MAX));
        for member in &self.members {
            checksum.hash_u32(member.as_u32());
        }
    }
}

impl Squad {
    /// Return whether a voluntary Jump action currently owns this squad.
    #[must_use]
    pub const fn is_jumping(&self) -> bool {
        !matches!(self.jump.phase, SquadJumpPhase::Inactive)
    }

    /// Return the authoritative voluntary Jump phase.
    #[must_use]
    pub const fn jump_phase(&self) -> SquadJumpPhase {
        self.jump.phase
    }

    /// Return the shared squad landing anchor.
    #[must_use]
    pub const fn jump_target(&self) -> Option<Vec3> {
        if self.is_jumping() {
            Some(self.jump.target_anchor)
        } else {
            None
        }
    }

    /// Return the entity retained for a post-landing order.
    #[must_use]
    pub fn jump_target_entity(&self) -> Option<EntityId> {
        self.jump.target_id()
    }

    pub(crate) fn hash_jump_state(&self, checksum: &mut SyncChecksum) {
        self.jump.hash_state(checksum);
    }
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}
