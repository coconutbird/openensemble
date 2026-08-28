//! Shared authoritative order entry points.

use super::World;
use crate::entity::Entity;
use crate::{EntityId, PlayerId};
use glam::Vec3;

impl World {
    /// Issue an immediate movement order through the shared simulation path.
    ///
    /// A recipient must be a live player-owned squad or a standalone mobile
    /// unit. Buildings, contained squads, and units controlled by a squad
    /// reject the order.
    pub fn issue_move_order(
        &mut self,
        player_id: PlayerId,
        recipient_id: EntityId,
        target: Vec3,
    ) -> bool {
        if !target.is_finite() {
            return false;
        }
        if self.squads.get(recipient_id).is_some_and(|squad| {
            squad.base.player_id == player_id && squad.is_alive() && !squad.garrison.is_garrisoned()
        }) {
            let Some(squad) = self.squads.get_mut(recipient_id) else {
                return false;
            };
            squad.move_to(target);
            return true;
        }
        if self.units.get(recipient_id).is_some_and(|unit| {
            unit.base.player_id == player_id
                && unit.is_alive()
                && !unit.is_building()
                && unit.squad_id.is_none()
                && !unit.is_garrisoned()
        }) {
            return self
                .units
                .get_mut(recipient_id)
                .is_some_and(|unit| unit.move_to(target));
        }
        false
    }
}
