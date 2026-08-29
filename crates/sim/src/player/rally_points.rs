//! Player-global rally-point state.

use super::Player;
use crate::entities::RallyPoint;
use crate::entities::units::rally_points::hash_optional_rally_point;
use crate::sync::SyncChecksum;

impl Player {
    /// Return the player's authoritative global rally point.
    #[must_use]
    pub const fn rally_point(&self) -> Option<RallyPoint> {
        self.rally_point
    }

    pub(crate) fn set_rally_point(&mut self, rally_point: RallyPoint) {
        self.rally_point = Some(rally_point);
    }

    pub(crate) fn clear_rally_point(&mut self) {
        self.rally_point = None;
    }

    pub(crate) fn hash_rally_point_state(&self, checksum: &mut SyncChecksum) {
        hash_optional_rally_point(checksum, self.rally_point);
    }
}
