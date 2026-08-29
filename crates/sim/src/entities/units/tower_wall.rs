//! Persistent state owned by retail's `BUnitActionTowerWall`.

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Target and beam anchors configured by `SetTowerWallDestination`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TowerWallAction {
    target_squad_id: EntityId,
    beam_start_position: Vec3,
    beam_end_position: Vec3,
}

impl TowerWallAction {
    #[must_use]
    pub(crate) const fn new(
        target_squad_id: EntityId,
        beam_start_position: Vec3,
        beam_end_position: Vec3,
    ) -> Self {
        Self {
            target_squad_id,
            beam_start_position,
            beam_end_position,
        }
    }

    /// Destination tower squad selected by the trigger effect.
    #[must_use]
    pub const fn target_squad_id(self) -> EntityId {
        self.target_squad_id
    }

    /// Source visual anchor before renderer-specific bounds adjustment.
    #[must_use]
    pub const fn beam_start_position(self) -> Vec3 {
        self.beam_start_position
    }

    /// Destination visual anchor before renderer-specific bounds adjustment.
    #[must_use]
    pub const fn beam_end_position(self) -> Vec3 {
        self.beam_end_position
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.target_squad_id.as_u32());
        checksum.hash_vec3(
            self.beam_start_position.x,
            self.beam_start_position.y,
            self.beam_start_position.z,
        );
        checksum.hash_vec3(
            self.beam_end_position.x,
            self.beam_end_position.y,
            self.beam_end_position.z,
        );
    }
}
