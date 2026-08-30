//! Authoritative execution of retail squad and per-unit `Capture` actions.

use super::World;
use crate::entities::{CapturePhase, SquadState, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, TeamRelation};
use glam::Vec3;

mod costs;
mod order;
mod update;

#[derive(Debug, Clone)]
struct CaptureTarget {
    id: EntityId,
    player_id: PlayerId,
    proto_object_name: String,
    position: Vec3,
    forward: Vec3,
    radius: f32,
    invulnerable: bool,
    capturable: bool,
}

#[derive(Debug, Clone)]
struct CaptureOrder {
    player_id: PlayerId,
    target_id: EntityId,
    unit_ids: Vec<EntityId>,
}

impl World {
    /// Return the canonical unit targeted by a capturable unit or squad.
    #[must_use]
    pub fn capture_target_unit_id(&self, requested_target_id: EntityId) -> Option<EntityId> {
        self.squad_move_entity_target(requested_target_id)
            .map(|(target_id, _)| target_id)
    }

    /// Return whether a squad can begin capturing a target under current ownership rules.
    #[must_use]
    pub fn can_squad_capture_target(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
        requested_target_id: EntityId,
    ) -> bool {
        let Some(target_id) = self.capture_target_unit_id(requested_target_id) else {
            return false;
        };
        self.capture_order_source(player_id, squad_id).is_some()
            && self.capture_target(target_id).is_some_and(|target| {
                target.player_id != player_id
                    && self.capture_target_accepts(&target, player_id, squad_id)
            })
    }

    /// Cancel a connected capture order and apply retail's shared-cost refund rules.
    pub fn cancel_capture_order(&mut self, squad_id: EntityId) -> bool {
        let Some(order) = self.capture_order(squad_id) else {
            return false;
        };
        self.stop_capture_activity(squad_id, order.target_id, &order.unit_ids);
        self.disconnect_capture_link(order.target_id, order.player_id, squad_id, false);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.capture.cancel();
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            if squad.is_alive() && squad.state == SquadState::Working {
                squad.state = SquadState::Idle;
            }
        }
        for unit_id in order.unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id)
                && unit.capture.action.target_id() == Some(order.target_id)
            {
                unit.cancel_capture_action();
                stop_capture_unit(unit);
            }
        }
        true
    }

    pub(crate) fn prepare_remove_squad_capture(&mut self, squad_id: EntityId) {
        if self.cancel_capture_order(squad_id) {
            return;
        }
        let links = self
            .units
            .iter()
            .flat_map(|(target_id, target)| {
                target
                    .capture
                    .target
                    .linked_squads()
                    .into_iter()
                    .filter(move |(_, linked_squad_id)| *linked_squad_id == squad_id)
                    .map(move |(player_id, _)| (target_id, player_id))
            })
            .collect::<Vec<_>>();
        for (target_id, player_id) in links {
            self.disconnect_capture_link(target_id, player_id, squad_id, false);
        }
    }

    pub(crate) fn prepare_remove_unit_capture(&mut self, unit_id: EntityId) {
        self.detach_capture_source_unit(unit_id);
        let linked = self
            .units
            .get(unit_id)
            .map(|unit| unit.capture.target.linked_squads())
            .unwrap_or_default();
        let targeting_squads = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                (squad.capture.target_id() == Some(unit_id)).then_some(squad_id)
            })
            .collect::<Vec<_>>();
        for squad_id in targeting_squads {
            self.finish_capture_order(squad_id, CapturePhase::Failed, false);
        }
        for (player_id, squad_id) in linked {
            self.disconnect_capture_link(unit_id, player_id, squad_id, false);
        }
    }

    fn capture_order_source(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
    ) -> Option<Vec<EntityId>> {
        let squad = self.squads.get(squad_id)?;
        (squad.base.player_id == player_id
            && squad.is_alive()
            && !self.is_squad_incapacitated(squad_id)
            && !squad.garrison.is_garrisoned()
            && !squad.is_cryo_frozen()
            && !squad.is_raging()
            && squad.trained_air_birth.is_none())
        .then(|| squad.unit_ids.clone())
    }

    fn capture_order(&self, squad_id: EntityId) -> Option<CaptureOrder> {
        let squad = self.squads.get(squad_id)?;
        Some(CaptureOrder {
            player_id: squad.capture.player_id()?,
            target_id: squad.capture.target_id()?,
            unit_ids: squad.unit_ids.clone(),
        })
    }

    fn capture_target(&self, target_id: EntityId) -> Option<CaptureTarget> {
        let unit = self.units.get(target_id).filter(|unit| unit.is_alive())?;
        Some(CaptureTarget {
            id: target_id,
            player_id: unit.base.player_id,
            proto_object_name: unit.proto_object_name.clone(),
            position: unit.base.position,
            forward: unit.base.forward,
            radius: unit.obstruction_radius(),
            invulnerable: unit.is_invulnerable(),
            capturable: unit.is_capturable(),
        })
    }

    fn capture_target_accepts(
        &self,
        target: &CaptureTarget,
        player_id: PlayerId,
        squad_id: EntityId,
    ) -> bool {
        if !target.capturable
            || (target.player_id != crate::player::GAIA_PLAYER && !target.invulnerable)
        {
            return false;
        }
        self.units.get(target.id).is_some_and(|unit| {
            unit.capture.target.active_unit_ids().iter().all(|unit_id| {
                self.units.get(*unit_id).is_some_and(|source| {
                    source.base.player_id == player_id && source.squad_id == Some(squad_id)
                })
            })
        })
    }

    fn disconnect_capture_link(
        &mut self,
        target_id: EntityId,
        player_id: PlayerId,
        squad_id: EntityId,
        captured: bool,
    ) {
        let Some((unlink, target_owner, still_active)) =
            self.units.get_mut(target_id).and_then(|target| {
                let unlink = target
                    .capture
                    .target
                    .disconnect_squad(player_id, squad_id)?;
                Some((
                    unlink,
                    target.base.player_id,
                    target.capture.target.is_being_captured(),
                ))
            })
        else {
            return;
        };
        if unlink.removed_last_player_link {
            if !captured
                && target_owner != player_id
                && let Some(player) = self.get_player_mut(player_id)
            {
                player.resources.refund(&unlink.cost);
            }
            if !still_active && let Some(target) = self.units.get_mut(target_id) {
                target.capture.target.reset_progress();
            }
        }
    }

    fn stop_capture_activity(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        unit_ids: &[EntityId],
    ) {
        if let Some(target) = self.units.get_mut(target_id) {
            for unit_id in unit_ids {
                target.capture.target.stop_unit(*unit_id);
            }
        }
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(*unit_id)
                && unit.squad_id == Some(squad_id)
                && unit.capture.action.target_id() == Some(target_id)
            {
                unit.capture.action.set_phase(CapturePhase::Moving);
            }
        }
    }

    fn detach_capture_source_unit(&mut self, unit_id: EntityId) {
        let target_id = self
            .units
            .get(unit_id)
            .and_then(|unit| unit.capture.action.target_id());
        if let Some(target_id) = target_id
            && let Some(target) = self.units.get_mut(target_id)
        {
            target.capture.target.stop_unit(unit_id);
        }
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.cancel_capture_action();
        }
    }

    fn capture_relation(
        &self,
        source: PlayerId,
        target: PlayerId,
    ) -> crate::gameplay::TacticRelation {
        if source == target {
            return crate::gameplay::TacticRelation::SelfPlayer;
        }
        match self.player_relation(source, target) {
            Some(TeamRelation::Ally) => crate::gameplay::TacticRelation::Ally,
            Some(TeamRelation::Enemy) => crate::gameplay::TacticRelation::Enemy,
            Some(TeamRelation::Neutral) | None => crate::gameplay::TacticRelation::Neutral,
        }
    }
}

fn stop_capture_unit(unit: &mut crate::entities::Unit) {
    unit.move_target = None;
    unit.base.velocity = Vec3::ZERO;
    if unit.is_alive() {
        unit.state = UnitState::Idle;
    }
}

fn planar_direction(to: Vec3, from: Vec3) -> Vec3 {
    Vec3::new(to.x - from.x, 0.0, to.z - from.z).normalize_or_zero()
}

fn planar_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero()
}

#[cfg(test)]
mod tests;
