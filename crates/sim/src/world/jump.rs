//! Source-backed voluntary Brute Jump lifecycle.

mod placement;

use super::World;
use crate::entities::squads::{SquadJumpCompletion, SquadJumpPhase, formation_offset_to_world};
use crate::entities::{RecoveryType, SquadState, Unit, UnitState};
use crate::entity::Entity;
use crate::entity_id::{EntityClass, EntityId};
use crate::gameplay::{GameplayCatalog, JumpActionProfile};
use crate::order::{JumpOrderRequest, JumpOrderType};
use crate::player::PlayerId;
use glam::Vec3;
use std::collections::BTreeSet;

const MINIMUM_JUMP_DISTANCE: f32 = 10.0;
const MINIMUM_DIRECTION_SQUARED: f32 = 0.000_001;

#[derive(Debug, Clone, Copy)]
struct JumpTarget {
    id: Option<EntityId>,
    position: Vec3,
    obstruction_radius: f32,
}

#[derive(Debug, Clone)]
struct JumpMemberPlan {
    unit_id: EntityId,
    action_name: String,
    target: Vec3,
    velocity_scalar: f32,
}

#[derive(Debug, Clone)]
struct JumpSquadContext {
    leader_id: EntityId,
    average: Vec3,
    forward: Vec3,
    radius: f32,
    position: Vec3,
    member_ids: Vec<EntityId>,
}

#[derive(Debug, Clone)]
struct JumpStartPlan {
    kind: JumpOrderType,
    target_id: Option<EntityId>,
    landing: Vec3,
    ability_id: u8,
    action_name: String,
    members: Vec<JumpMemberPlan>,
}

impl World {
    /// Issue one voluntary retail Jump-family order to a player-owned squad.
    pub fn issue_jump_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        request: JumpOrderRequest,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let kind = request.kind();
        let target_position = request.target_position();
        if kind == JumpOrderType::Pull || !target_position.is_finite() {
            return false;
        }
        let Some(target) = self.resolve_jump_target(kind, request.target_id(), target_position)
        else {
            return false;
        };
        let Some(context) = self.jump_squad_context(player_id, squad_id) else {
            return false;
        };
        let Some(leader) = self.units.get(context.leader_id) else {
            return false;
        };
        let Some(profile) = self
            .enabled_jump_profile(player_id, leader, kind, gameplay)
            .cloned()
        else {
            return false;
        };
        let Some(ability_id) =
            Self::resolve_jump_ability(leader, request.requested_ability_id(), gameplay)
        else {
            return false;
        };
        let maximum_distance_squared = profile.max_distance() * profile.max_distance();
        if context.average.distance_squared(target.position) > maximum_distance_squared {
            return false;
        }

        let mut desired = target.position;
        if kind == JumpOrderType::Attack {
            let weapon_range = self.effective_jump_weapon_range(player_id, leader, &profile);
            let direction = (target.position - context.average).normalize_or_zero();
            desired -= direction * (weapon_range * 0.5);
        }
        let Some(landing) = self.plan_jump_landing(desired, target.obstruction_radius, &context)
        else {
            return false;
        };
        if context.position.distance(landing) < MINIMUM_JUMP_DISTANCE {
            return false;
        }

        let members = self.plan_jump_members(player_id, kind, landing, &context, gameplay);
        if members.is_empty() {
            return false;
        }
        self.begin_jump(
            squad_id,
            &JumpStartPlan {
                kind,
                target_id: target.id,
                landing,
                ability_id,
                action_name: profile.action_name().to_owned(),
                members,
            },
        )
    }

    fn plan_jump_landing(
        &self,
        desired: Vec3,
        target_radius: f32,
        context: &JumpSquadContext,
    ) -> Option<Vec3> {
        let excluded = context.member_ids.iter().copied().collect::<BTreeSet<_>>();
        let mut landing = placement::find_landing_position(
            self,
            desired,
            target_radius,
            context.radius,
            &excluded,
        )?;
        let clamped = placement::clamp_inside_playable_bounds(self, landing);
        if clamped != landing {
            landing =
                placement::find_landing_position(self, clamped, 0.0, context.radius, &excluded)?;
        }
        self.jump_ground_position(landing)
    }

    fn plan_jump_members(
        &self,
        player_id: PlayerId,
        kind: JumpOrderType,
        landing: Vec3,
        context: &JumpSquadContext,
        gameplay: &GameplayCatalog,
    ) -> Vec<JumpMemberPlan> {
        context
            .member_ids
            .iter()
            .filter_map(|&unit_id| {
                let unit = self.units.get(unit_id)?;
                let profile = self.enabled_jump_profile(player_id, unit, kind, gameplay)?;
                let target = placement::clamp_inside_playable_bounds(
                    self,
                    landing + formation_offset_to_world(context.forward, unit.formation_offset),
                );
                Some(JumpMemberPlan {
                    unit_id,
                    action_name: profile.action_name().to_owned(),
                    target: self.jump_ground_position(target)?,
                    velocity_scalar: profile.velocity_scalar(),
                })
            })
            .collect()
    }

    pub(in crate::world) fn update_jumps(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| squad.is_jumping().then_some(squad_id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.advance_jump(squad_id, dt, gameplay);
        }
    }

    fn begin_jump(&mut self, squad_id: EntityId, plan: &JumpStartPlan) -> bool {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.remove_all_orders();
        }
        let mut started = Vec::new();
        for member in &plan.members {
            let Some(unit) = self.units.get_mut(member.unit_id) else {
                continue;
            };
            unit.clear_attack_order();
            unit.stop();
            unit.cancel_squad_ground_move();
            unit.cancel_gather_action();
            unit.cancel_capture_action();
            unit.state = UnitState::Idle;
            if unit.begin_jump_action(
                plan.kind,
                &member.action_name,
                member.target,
                member.velocity_scalar,
            ) {
                started.push(member.unit_id);
            }
        }
        let accepted = self.squads.get_mut(squad_id).is_some_and(|squad| {
            if !squad.jump.begin(
                plan.kind,
                &plan.action_name,
                plan.target_id,
                plan.landing,
                Some(plan.ability_id),
                started.clone(),
            ) {
                return false;
            }
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Working;
            squad.cancel_idle_action();
            true
        });
        if !accepted {
            for unit_id in started {
                if let Some(unit) = self.units.get_mut(unit_id) {
                    unit.cancel_jump_action();
                }
            }
            return false;
        }
        self.cancel_incoming_power_transport(squad_id);
        true
    }

    fn advance_jump(&mut self, squad_id: EntityId, dt: f32, gameplay: &GameplayCatalog) {
        let Some((pending, target_anchor, member_ids)) = self.squads.get(squad_id).map(|squad| {
            (
                squad.jump_phase() == SquadJumpPhase::Pending,
                squad.jump_target().unwrap_or(squad.base.position),
                squad.jump.members().to_vec(),
            )
        }) else {
            return;
        };
        if !self.squads.get(squad_id).is_some_and(Entity::is_alive) {
            self.cancel_jump(squad_id);
            return;
        }
        if pending {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                let _activated = squad.jump.activate();
                squad.base.position = target_anchor;
                squad.base.velocity = Vec3::ZERO;
            }
            for unit_id in member_ids {
                if let Some(unit) = self.units.get_mut(unit_id) {
                    let _pending = unit.advance_jump_action(dt);
                }
            }
            return;
        }

        let mut active_members = BTreeSet::new();
        let mut completed_members = Vec::new();
        for unit_id in member_ids {
            let valid = self.units.get(unit_id).is_some_and(|unit| {
                unit.is_alive() && unit.squad_id == Some(squad_id) && unit.is_jumping()
            });
            if !valid {
                if let Some(unit) = self.units.get_mut(unit_id) {
                    unit.cancel_jump_action();
                }
                continue;
            }
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            let target = unit.jump_target().unwrap_or(unit.base.position);
            let previous = unit.base.position;
            let advance = unit.advance_jump_action(dt);
            if let Some(position) = advance.position {
                unit.base.position = position;
                unit.base.velocity = if dt.is_finite() && dt > 0.0 {
                    (position - previous) / dt
                } else {
                    Vec3::ZERO
                };
                let direction = Vec3::new(target.x - position.x, 0.0, target.z - position.z)
                    .normalize_or_zero();
                if direction.length_squared() > MINIMUM_DIRECTION_SQUARED {
                    unit.base.set_forward(direction);
                }
                unit.move_target = None;
                unit.state = UnitState::Idle;
            }
            if advance.complete {
                unit.base.velocity = Vec3::ZERO;
                unit.cancel_jump_action();
                completed_members.push(unit_id);
            } else {
                active_members.insert(unit_id);
            }
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad
                .jump
                .retain_members(|unit_id| active_members.contains(&unit_id));
        }
        for unit_id in completed_members {
            self.resolve_jump_landing_obstruction(unit_id, gameplay);
        }
        self.sync_jump_squad_origin(squad_id, dt);
        let completion = self
            .squads
            .get_mut(squad_id)
            .and_then(|squad| squad.jump.completion());
        if let Some(completion) = completion {
            self.finish_jump(squad_id, completion, gameplay);
        }
    }

    fn finish_jump(
        &mut self,
        squad_id: EntityId,
        completion: SquadJumpCompletion,
        gameplay: &GameplayCatalog,
    ) {
        let Some(player_id) = self.squads.get(squad_id).map(|squad| squad.base.player_id) else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.base.velocity = Vec3::ZERO;
            squad.move_target = None;
            if squad.is_alive() {
                squad.state = SquadState::Idle;
            }
        }
        if let Some(ability) = completion.ability_id.and_then(|id| gameplay.ability(id)) {
            let recovery = self
                .get_player(player_id)
                .map_or(ability.recovery_time(), |player| {
                    player
                        .technologies
                        .ability_recovery_time(ability.name(), ability.recovery_time())
                });
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad
                    .recovery
                    .start(RecoveryType::Ability, recovery, Some(ability.database_id()));
            }
        }
        match (completion.kind, completion.target_id) {
            (JumpOrderType::Gather, Some(target_id)) => {
                let _issued = self.issue_gather_order(player_id, squad_id, target_id, gameplay);
            }
            (JumpOrderType::Garrison, Some(target_id)) => {
                let _issued = self.issue_garrison_order(player_id, squad_id, target_id, 0.0);
            }
            (JumpOrderType::Jump | JumpOrderType::Attack | JumpOrderType::Pull, _)
            | (JumpOrderType::Gather | JumpOrderType::Garrison, None) => {}
        }
    }

    fn cancel_jump(&mut self, squad_id: EntityId) {
        let member_ids = self
            .squads
            .get(squad_id)
            .map_or_else(Vec::new, |squad| squad.jump.members().to_vec());
        for unit_id in member_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.cancel_jump_action();
                unit.base.velocity = Vec3::ZERO;
            }
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.jump.cancel();
            squad.base.velocity = Vec3::ZERO;
            if squad.is_alive() {
                squad.state = SquadState::Idle;
            }
        }
    }

    fn sync_jump_squad_origin(&mut self, squad_id: EntityId, dt: f32) {
        let Some(average) = self.jump_member_average(squad_id) else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            let previous = squad.base.position;
            squad.base.position = average;
            squad.base.velocity = if dt.is_finite() && dt > 0.0 {
                (average - previous) / dt
            } else {
                Vec3::ZERO
            };
            squad.set_leash_position(average, true);
        }
    }

    fn jump_squad_context(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
    ) -> Option<JumpSquadContext> {
        let squad = self.squads.get(squad_id)?;
        if squad.base.player_id != player_id
            || !squad.is_alive()
            || self.is_squad_incapacitated(squad_id)
            || squad.garrison.is_garrisoned()
            || squad.is_cryo_frozen()
            || squad.is_raging()
            || squad.is_jumping()
            || squad.trained_air_birth.is_some()
            || squad.recovery.blocks(RecoveryType::Ability)
        {
            return None;
        }
        let member_ids = squad.unit_ids.clone();
        let leader_id = member_ids.iter().copied().find(|unit_id| {
            self.units
                .get(*unit_id)
                .is_some_and(|unit| unit.is_operational() && !unit.is_jumping())
        })?;
        let average = self.jump_member_average(squad_id)?;
        let radius = member_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .map(|unit| {
                let delta = unit.base.position - average;
                Vec3::new(delta.x, 0.0, delta.z).length() + unit.obstruction_radius()
            })
            .fold(0.0, f32::max);
        Some(JumpSquadContext {
            leader_id,
            average,
            forward: squad.base.forward,
            radius,
            position: squad.base.position,
            member_ids,
        })
    }

    fn jump_member_average(&self, squad_id: EntityId) -> Option<Vec3> {
        let squad = self.squads.get(squad_id)?;
        let mut sum = Vec3::ZERO;
        let mut count = 0.0f32;
        for unit in squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .filter(|unit| unit.is_alive())
        {
            sum += unit.base.position;
            count += 1.0;
        }
        (count > 0.0).then(|| sum / count)
    }

    fn enabled_jump_profile<'gameplay>(
        &self,
        player_id: PlayerId,
        unit: &Unit,
        kind: JumpOrderType,
        gameplay: &'gameplay GameplayCatalog,
    ) -> Option<&'gameplay JumpActionProfile> {
        let player = self.get_player(player_id)?;
        gameplay
            .jump_actions(&unit.proto_object_name)
            .iter()
            .find(|profile| {
                profile.kind() == kind
                    && !player.technologies.ability_disabled(
                        &unit.proto_object_name,
                        profile.ability_starts_disabled(),
                    )
                    && {
                        let player_enabled = player.technologies.action_enabled(
                            &unit.proto_object_name,
                            profile.action_name(),
                            !profile.starts_disabled(),
                        );
                        unit.actions
                            .is_enabled(profile.action_name(), !player_enabled)
                    }
            })
    }

    fn resolve_jump_ability(
        leader: &Unit,
        requested: Option<u8>,
        gameplay: &GameplayCatalog,
    ) -> Option<u8> {
        let command_id = gameplay.command_ability_id()?;
        let command = gameplay.resolve_order_ability(&leader.proto_object_name, command_id)?;
        if let Some(requested) = requested {
            let requested = gameplay.resolve_order_ability(&leader.proto_object_name, requested)?;
            (requested.database_id() == command.database_id()).then_some(command.database_id())
        } else {
            Some(command.database_id())
        }
    }

    fn effective_jump_weapon_range(
        &self,
        player_id: PlayerId,
        leader: &Unit,
        profile: &JumpActionProfile,
    ) -> f32 {
        let Some(weapon_name) = profile.weapon_name() else {
            return 0.0;
        };
        self.get_player(player_id)
            .map_or(profile.weapon_max_range(), |player| {
                player.technologies.weapon_range(
                    &leader.proto_object_name,
                    weapon_name,
                    profile.weapon_max_range(),
                )
            })
    }

    fn resolve_jump_target(
        &self,
        kind: JumpOrderType,
        target_id: Option<EntityId>,
        target_position: Vec3,
    ) -> Option<JumpTarget> {
        let Some(target_id) = target_id.filter(|id| !id.is_invalid()) else {
            return (kind == JumpOrderType::Jump).then_some(JumpTarget {
                id: None,
                position: target_position,
                obstruction_radius: 0.0,
            });
        };
        match target_id.class()? {
            EntityClass::Unit => {
                let unit = self.units.get(target_id).filter(|unit| unit.is_alive())?;
                Some(JumpTarget {
                    id: Some(target_id),
                    position: unit.base.position,
                    obstruction_radius: unit.obstruction_radius(),
                })
            }
            EntityClass::Squad => {
                let squad = self
                    .squads
                    .get(target_id)
                    .filter(|squad| squad.is_alive())?;
                Some(JumpTarget {
                    id: Some(target_id),
                    position: squad.base.position,
                    obstruction_radius: self.jump_squad_radius(target_id),
                })
            }
            EntityClass::Object | EntityClass::Projectile => {
                self.entity_position(target_id).map(|position| JumpTarget {
                    id: Some(target_id),
                    position,
                    obstruction_radius: 0.0,
                })
            }
            _ => None,
        }
    }

    fn jump_squad_radius(&self, squad_id: EntityId) -> f32 {
        let Some(average) = self.jump_member_average(squad_id) else {
            return 0.0;
        };
        self.squads.get(squad_id).map_or(0.0, |squad| {
            squad
                .unit_ids
                .iter()
                .filter_map(|unit_id| self.units.get(*unit_id))
                .map(|unit| {
                    let delta = unit.base.position - average;
                    Vec3::new(delta.x, 0.0, delta.z).length() + unit.obstruction_radius()
                })
                .fold(0.0, f32::max)
        })
    }

    fn jump_ground_position(&self, mut position: Vec3) -> Option<Vec3> {
        if self.has_terrain_simulation() {
            position.y = self.terrain_height(position, true)?;
        }
        Some(position)
    }

    fn resolve_jump_landing_obstruction(&mut self, unit_id: EntityId, gameplay: &GameplayCatalog) {
        let Some(source) = self.units.get(unit_id).cloned() else {
            return;
        };
        let obstructed = self.units.iter().any(|(other_id, other)| {
            other_id != unit_id
                && other.is_alive()
                && other.is_object_type("Obstruction")
                && planar_distance(source.base.position, other.base.position)
                    <= source.obstruction_radius() + other.obstruction_radius()
        });
        if !obstructed {
            return;
        }
        let Some(profile) = gameplay
            .physics_replacement(&source.proto_object_name)
            .cloned()
        else {
            return;
        };
        let ground_height = self
            .terrain_height(source.base.position, true)
            .unwrap_or(source.base.position.y.min(0.0));
        let replacement_id = self.units.allocate_id();
        let replacement =
            source.create_physics_replacement(replacement_id, &profile, ground_height);
        if self.remove_unit(unit_id).is_some() {
            self.units.insert(replacement_id, replacement);
        }
    }
}

fn planar_distance(left: Vec3, right: Vec3) -> f32 {
    let delta = left - right;
    delta.x.hypot(delta.z)
}

#[cfg(test)]
mod tests;
