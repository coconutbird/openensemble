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
            squad.base.player_id == player_id
                && squad.is_alive()
                && !self.is_squad_incapacitated(recipient_id)
                && !squad.garrison.is_garrisoned()
                && !squad.is_jumping()
                && squad.trained_air_birth.is_none()
        }) {
            let _cancelled = self.cancel_capture_order(recipient_id);
            let _repair_cancelled = self.cancel_repair_other_order(recipient_id);
            let Some(squad) = self.squads.get_mut(recipient_id) else {
                return false;
            };
            let accepted = squad.issue_scripted_move(target, false, false);
            if accepted {
                self.cancel_incoming_power_transport(recipient_id);
            }
            return accepted;
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

    /// Issue a squad work command to a fixed position.
    pub fn issue_squad_move_order_to_position(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        target: Vec3,
        attack_move: bool,
        queue: bool,
    ) -> bool {
        self.issue_squad_move_path(player_id, squad_id, &[target], attack_move, queue)
    }

    /// Snapshot a live unit or squad leader position into a squad work command.
    pub fn issue_squad_move_order_to_entity(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        requested_target_id: EntityId,
        attack_move: bool,
        queue: bool,
    ) -> bool {
        let Some((_target_id, position)) = self.squad_move_entity_target(requested_target_id)
        else {
            return false;
        };
        self.issue_squad_move_path(player_id, squad_id, &[position], attack_move, queue)
    }

    pub(crate) fn squad_move_entity_target(
        &self,
        requested_target_id: EntityId,
    ) -> Option<(EntityId, Vec3)> {
        if let Some(unit) = self
            .units
            .get(requested_target_id)
            .filter(|unit| unit.is_alive())
        {
            return Some((requested_target_id, unit.base.position));
        }
        let squad = self.squads.get(requested_target_id).filter(|squad| {
            squad.is_alive() && !self.is_squad_incapacitated(requested_target_id)
        })?;
        let leader_id = squad.unit_ids.first().copied()?;
        let leader = self.units.get(leader_id).filter(|unit| unit.is_alive())?;
        Some((leader_id, leader.base.position))
    }

    /// Issue one multi-waypoint work command to an owned squad.
    pub fn issue_squad_move_path(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        waypoints: &[Vec3],
        attack_move: bool,
        queue: bool,
    ) -> bool {
        if !self.squads.get(squad_id).is_some_and(|squad| {
            squad.base.player_id == player_id
                && squad.is_alive()
                && !self.is_squad_incapacitated(squad_id)
                && !squad.garrison.is_garrisoned()
                && !squad.is_jumping()
                && squad.trained_air_birth.is_none()
        }) {
            return false;
        }
        let _cancelled = self.cancel_capture_order(squad_id);
        let _repair_cancelled = self.cancel_repair_other_order(squad_id);
        let accepted = self
            .squads
            .get_mut(squad_id)
            .is_some_and(|squad| squad.issue_scripted_path(waypoints, attack_move, queue));
        if accepted {
            self.cancel_incoming_power_transport(squad_id);
        }
        accepted
    }

    /// Persistently enable or disable reverse movement for a squad and its members.
    pub fn set_squad_reverse_move(&mut self, squad_id: EntityId, reverse_move: bool) -> bool {
        let Some(unit_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return false;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.set_reverse_move(reverse_move);
        }
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.set_reverse_move(reverse_move);
            }
        }
        true
    }
}
