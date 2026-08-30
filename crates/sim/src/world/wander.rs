//! Source-backed execution of persistent squad `Wander` actions.

use super::World;
use crate::entities::squads::WanderPhase;
use crate::entities::{Squad, SquadState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, WanderProfile};
use crate::player::PlayerId;
use glam::Vec3;

const NEARBY_SQUAD_RADIUS: f32 = 5.0;
const MOVE_TARGET_RANGE: f32 = 5.0;

#[derive(Debug, Clone)]
struct WanderContext {
    player_id: PlayerId,
    leader_id: EntityId,
    proto_squad_id: i32,
    average_position: Vec3,
    profile: WanderProfile,
}

impl World {
    pub(super) fn update_wanders(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let squad_ids = self.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_wander(squad_id, dt, gameplay);
        }
    }

    fn update_wander(&mut self, squad_id: EntityId, dt: f32, gameplay: &GameplayCatalog) {
        let Some(context) = self.wander_context(squad_id, gameplay) else {
            if self
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.wander.is_initialized())
            {
                self.disconnect_wander(squad_id);
            }
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.wander.initialize(squad.base.position);
        }
        if !self.wander_action_enabled(&context) {
            return;
        }
        let movement_finished = self.wander_movement_finished(squad_id);
        let choose_target = self
            .squads
            .get_mut(squad_id)
            .is_some_and(|squad| squad.wander.advance(dt, movement_finished));
        if choose_target {
            let target = self.choose_wander_target(squad_id, &context);
            self.begin_wander_move(squad_id, target);
        }
    }

    fn wander_context(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<WanderContext> {
        let squad = self.squads.get(squad_id).filter(|squad| squad.is_alive())?;
        let leader_id = *squad.unit_ids.first()?;
        let leader = self.units.get(leader_id).filter(|unit| unit.is_alive())?;
        Some(WanderContext {
            player_id: squad.base.player_id,
            leader_id,
            proto_squad_id: squad.proto_squad_id,
            average_position: self.average_live_member_position(squad),
            profile: gameplay.wander(&leader.proto_object_name)?.clone(),
        })
    }

    fn average_live_member_position(&self, squad: &Squad) -> Vec3 {
        let (sum, count) = squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id).filter(|unit| unit.is_alive()))
            .fold((Vec3::ZERO, 0_u16), |(sum, count), unit| {
                (sum + unit.base.position, count.saturating_add(1))
            });
        if count == 0 {
            squad.base.position
        } else {
            sum / f32::from(count)
        }
    }

    fn wander_action_enabled(&self, context: &WanderContext) -> bool {
        let Some(leader) = self.units.get(context.leader_id) else {
            return false;
        };
        let authored_enabled = !context.profile.starts_disabled();
        let player_enabled =
            self.get_player(context.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &leader.proto_object_name,
                        context.profile.action_name(),
                        authored_enabled,
                    )
                });
        leader
            .actions
            .is_enabled(context.profile.action_name(), !player_enabled)
    }

    fn wander_movement_finished(&self, squad_id: EntityId) -> bool {
        self.squads.get(squad_id).is_some_and(|squad| {
            squad.wander.phase() == WanderPhase::Waiting
                && squad.state == SquadState::Idle
                && squad.move_target.is_none()
        })
    }

    fn choose_wander_target(&mut self, squad_id: EntityId, context: &WanderContext) -> Vec3 {
        let origin = self
            .squads
            .get(squad_id)
            .map_or(context.average_position, |squad| squad.wander.origin());
        let mut offset = Vec3::ZERO;
        let mut nearby_count = 0_u16;
        for (other_id, other) in self.squads.iter() {
            if other_id == squad_id
                || !other.is_alive()
                || other.proto_squad_id != context.proto_squad_id
                || planar_distance_squared(other.base.position, context.average_position)
                    > NEARBY_SQUAD_RADIUS * NEARBY_SQUAD_RADIUS
            {
                continue;
            }
            offset += context.average_position - other.base.position;
            nearby_count = nearby_count.saturating_add(1);
        }
        if nearby_count == 0 {
            return self.random_circular_position(origin, context.profile.work_range(), 0.0);
        }
        offset /= f32::from(nearby_count);
        let direction = (-offset).normalize_or_zero();
        let distance_from_origin = context.average_position.distance(origin);
        context.average_position + direction * (context.profile.work_range() - distance_from_origin)
    }

    fn begin_wander_move(&mut self, squad_id: EntityId, target: Vec3) {
        let Some(position) = self.squads.get(squad_id).map(|squad| squad.base.position) else {
            return;
        };
        let destination = ranged_destination(position, target);
        let movement_target = self.squads.get_mut(squad_id).and_then(|squad| {
            squad.remove_all_orders();
            destination.filter(|destination| squad.issue_scripted_move(*destination, false, false))
        });
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.wander.begin_move(target, movement_target);
        }
    }

    fn disconnect_wander(&mut self, squad_id: EntityId) {
        let Some((owned_target, active_target)) = self.squads.get_mut(squad_id).map(|squad| {
            let active_target = squad.move_target;
            (squad.wander.disconnect(), active_target)
        }) else {
            return;
        };
        if owned_target.is_some()
            && owned_target == active_target
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.stop();
        }
    }

    pub(super) fn prepare_remove_squad_wander(&mut self, squad_id: EntityId) {
        self.disconnect_wander(squad_id);
    }

    pub(super) fn prepare_remove_unit_wander(&mut self, unit_id: EntityId) {
        let Some(squad_id) = self.units.get(unit_id).and_then(|unit| unit.squad_id) else {
            return;
        };
        self.disconnect_wander(squad_id);
    }

    pub(super) fn prepare_squad_membership_change_wander(&mut self, squad_id: EntityId) {
        self.disconnect_wander(squad_id);
    }
}

fn ranged_destination(position: Vec3, target: Vec3) -> Option<Vec3> {
    let delta = target - position;
    let distance = delta.length();
    if !distance.is_finite() || distance <= MOVE_TARGET_RANGE {
        return None;
    }
    Some(position + delta * ((distance - MOVE_TARGET_RANGE) / distance))
}

fn planar_distance_squared(first: Vec3, second: Vec3) -> f32 {
    let delta = first - second;
    delta.x.mul_add(delta.x, delta.z * delta.z)
}

#[cfg(test)]
mod tests;
