//! Retail rally-point state stored by units and players.

use super::Unit;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// A rally destination with the last known position of an optional entity.
///
/// Retail keeps both values. When the target still exists its current position
/// wins; if it disappears, the stored position remains a deterministic
/// fallback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RallyPoint {
    position: Vec3,
    target_entity_id: Option<EntityId>,
}

impl RallyPoint {
    pub(crate) fn new(position: Vec3, target_entity_id: Option<EntityId>) -> Self {
        Self {
            position,
            target_entity_id,
        }
    }

    /// Return the stored position, which is also the target's last known
    /// position when this rally point was assigned.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Return the entity this rally point follows, if one resolved when set.
    #[must_use]
    pub const fn target_entity_id(self) -> Option<EntityId> {
        self.target_entity_id
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_vec3(self.position.x, self.position.y, self.position.z);
        checksum.hash_u32(
            self.target_entity_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct UnitRallyPoints {
    primary: Option<RallyPoint>,
    secondary: Option<RallyPoint>,
}

impl UnitRallyPoints {
    fn get(self, owner: PlayerId, player_id: PlayerId) -> Option<RallyPoint> {
        if player_id == owner {
            self.primary
        } else {
            self.secondary
        }
    }

    fn set(&mut self, owner: PlayerId, player_id: PlayerId, rally_point: RallyPoint) {
        if player_id == owner {
            self.primary = Some(rally_point);
        } else {
            self.secondary = Some(rally_point);
        }
    }

    fn clear(&mut self, owner: PlayerId, player_id: PlayerId) {
        if player_id == owner {
            self.primary = None;
        } else {
            self.secondary = None;
        }
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        hash_optional_rally_point(checksum, self.primary);
        hash_optional_rally_point(checksum, self.secondary);
    }
}

impl Unit {
    /// Return the primary or co-op rally point used by `player_id`.
    #[must_use]
    pub fn rally_point(&self, player_id: PlayerId) -> Option<RallyPoint> {
        self.rally_points.get(self.base.player_id, player_id)
    }

    pub(crate) fn set_rally_point(&mut self, player_id: PlayerId, rally_point: RallyPoint) {
        self.rally_points
            .set(self.base.player_id, player_id, rally_point);
    }

    pub(crate) fn clear_rally_point(&mut self, player_id: PlayerId) {
        self.rally_points.clear(self.base.player_id, player_id);
    }

    pub(crate) fn hash_rally_point_state(&self, checksum: &mut SyncChecksum) {
        self.rally_points.hash_state(checksum);
    }
}

pub(crate) fn hash_optional_rally_point(
    checksum: &mut SyncChecksum,
    rally_point: Option<RallyPoint>,
) {
    if let Some(rally_point) = rally_point {
        checksum.hash_u32(1);
        rally_point.hash_state(checksum);
    } else {
        checksum.hash_u32(0);
    }
}
