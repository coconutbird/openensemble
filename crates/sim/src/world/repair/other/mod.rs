//! Targeted squad `RepairOther` orders, healing, effects, and idle opportunities.

use super::super::World;
use crate::entities::{RepairOtherPhase, SquadState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, TeamRelation};
use glam::Vec3;

mod auto;
mod effects;
mod order;
mod update;

#[derive(Debug, Clone)]
struct RepairTarget {
    squad_id: EntityId,
    player_id: PlayerId,
    leader_id: EntityId,
    proto_object_name: String,
    position: Vec3,
}

impl World {
    /// Resolve an entity order target to its live parent squad.
    #[must_use]
    pub fn repair_other_target_squad_id(&self, requested_id: EntityId) -> Option<EntityId> {
        if self.squads.get(requested_id).is_some_and(Entity::is_alive) {
            return Some(requested_id);
        }
        let squad_id = self.units.get(requested_id)?.squad_id?;
        self.squads
            .get(squad_id)
            .is_some_and(Entity::is_alive)
            .then_some(squad_id)
    }

    /// Cancel a targeted repair order and remove all sim-owned effect objects.
    pub fn cancel_repair_other_order(&mut self, squad_id: EntityId) -> bool {
        let Some((active, effect_ids)) = self.squads.get_mut(squad_id).map(|squad| {
            let active = squad.repair_other.target_id().is_some();
            (active, squad.repair_other.take_effect_ids())
        }) else {
            return false;
        };
        if !active {
            return false;
        }
        self.remove_repair_other_effects(effect_ids);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.repair_other.cancel();
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            if squad.is_alive() && matches!(squad.state, SquadState::Moving | SquadState::Working) {
                squad.state = SquadState::Idle;
            }
        }
        true
    }

    pub(crate) fn prepare_remove_squad_repair_other(&mut self, squad_id: EntityId) {
        let _cancelled = self.cancel_repair_other_order(squad_id);
        let targeting = self
            .squads
            .iter()
            .filter_map(|(source_id, squad)| {
                (squad.repair_other.target_id() == Some(squad_id)).then_some(source_id)
            })
            .collect::<Vec<_>>();
        for source_id in targeting {
            self.finish_repair_other_order(source_id, RepairOtherPhase::Done);
        }
    }

    fn repair_target(&self, squad_id: EntityId) -> Option<RepairTarget> {
        let squad = self.squads.get(squad_id).filter(|squad| squad.is_alive())?;
        let leader_id = squad.unit_ids.first().copied()?;
        let leader = self
            .units
            .get(leader_id)
            .filter(|unit| unit.is_alive() && !unit.is_crashing())?;
        Some(RepairTarget {
            squad_id,
            player_id: squad.base.player_id,
            leader_id,
            proto_object_name: leader.proto_object_name.clone(),
            position: squad.base.position,
        })
    }

    fn repair_relation(
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

    fn connect_repair_other_order(
        &mut self,
        source_player_id: PlayerId,
        source_squad_id: EntityId,
        target: &RepairTarget,
        action_name: &str,
        ability_id: Option<u8>,
    ) -> bool {
        let _cancelled = self.cancel_repair_other_order(source_squad_id);
        let _capture_cancelled = self.cancel_capture_order(source_squad_id);
        let Some(squad) = self.squads.get_mut(source_squad_id) else {
            return false;
        };
        squad.begin_repair_other(source_player_id, target.squad_id, action_name, ability_id);
        self.cancel_incoming_power_transport(source_squad_id);
        true
    }

    fn finish_repair_other_order(&mut self, squad_id: EntityId, phase: RepairOtherPhase) {
        let effect_ids = self
            .squads
            .get_mut(squad_id)
            .map(|squad| squad.repair_other.take_effect_ids())
            .unwrap_or_default();
        self.remove_repair_other_effects(effect_ids);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            squad.repair_other.finish(phase);
            if squad.is_alive() {
                squad.state = SquadState::Idle;
            }
        }
    }
}

fn planar_direction(to: Vec3, from: Vec3) -> Vec3 {
    Vec3::new(to.x - from.x, 0.0, to.z - from.z).normalize_or_zero()
}

#[cfg(test)]
mod tests;
