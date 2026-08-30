//! Source-backed execution of persistent squad `AmbientLife` actions.

use super::World;
use crate::entities::squads::{AmbientLifePhase, DEVOUR_DURATION_MS, countdown_due};
use crate::entities::{AmbientLifeBehavior, SquadMode, SquadState, Unit};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{
    AmbientLifeProfile, AttackQuery, AttackQueryFlags, GameplayCatalog, TacticRelation,
};
use crate::player::{GAIA_PLAYER, PlayerId, TeamRelation};
use glam::Vec3;
use num_traits::ToPrimitive;

const LEAVING_MAP_DISTANCE: f32 = 20_000.0;

#[derive(Debug, Clone)]
struct AmbientContext {
    player_id: PlayerId,
    leader_id: EntityId,
    profile: AmbientLifeProfile,
}

impl World {
    pub(super) fn update_ambient_life(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let Some(elapsed_ms) = elapsed_milliseconds(dt) else {
            return;
        };
        let squad_ids = self.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_ambient_squad(squad_id, elapsed_ms, gameplay);
        }
    }

    fn update_ambient_squad(
        &mut self,
        squad_id: EntityId,
        elapsed_ms: u32,
        gameplay: &GameplayCatalog,
    ) {
        let Some(context) = self.ambient_context(squad_id, gameplay) else {
            self.disconnect_ambient_life(squad_id);
            return;
        };
        if !self.ambient_action_enabled(&context) {
            self.disconnect_ambient_life(squad_id);
            return;
        }
        self.initialize_ambient_life(squad_id, &context.profile);
        if self.start_ambient_life(squad_id) || !self.reconcile_ambient_child(squad_id) {
            return;
        }
        self.update_ambient_opportunity_timer(squad_id, elapsed_ms, gameplay, &context.profile);
        self.update_ambient_behavior(squad_id, elapsed_ms, &context.profile);
    }

    fn ambient_context(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<AmbientContext> {
        let squad = self.squads.get(squad_id).filter(|squad| squad.is_alive())?;
        let leader_id = *squad.unit_ids.first()?;
        let leader = self.units.get(leader_id).filter(|unit| unit.is_alive())?;
        Some(AmbientContext {
            player_id: squad.base.player_id,
            leader_id,
            profile: gameplay.ambient_life(&leader.proto_object_name)?.clone(),
        })
    }

    fn ambient_action_enabled(&self, context: &AmbientContext) -> bool {
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

    pub(super) fn initialize_ambient_life(
        &mut self,
        squad_id: EntityId,
        profile: &AmbientLifeProfile,
    ) {
        if self
            .squads
            .get(squad_id)
            .is_some_and(|squad| squad.ambient_life.is_initialized())
        {
            return;
        }
        let wander_timer_ms = self.trigger_random_index(profile.max_wander_frequency_ms());
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.initialize(
                wander_timer_ms,
                profile.predator_check_frequency_ms(),
                profile.prey_check_frequency_ms(),
            );
        }
    }

    fn start_ambient_life(&mut self, squad_id: EntityId) -> bool {
        let Some(state) = self
            .squads
            .get_mut(squad_id)
            .map(|squad| &mut squad.ambient_life)
        else {
            return false;
        };
        if state.phase != AmbientLifePhase::Starting {
            return false;
        }
        state.phase = AmbientLifePhase::Working;
        state.behavior = AmbientLifeBehavior::Wander;
        true
    }

    fn reconcile_ambient_child(&mut self, squad_id: EntityId) -> bool {
        let Some((phase, owned_move, current_prey, state, move_target, attack_target)) =
            self.squads.get(squad_id).map(|squad| {
                (
                    squad.ambient_life.phase,
                    squad.ambient_life.owned_move_target,
                    squad.ambient_life.current_prey_unit,
                    squad.state,
                    squad.move_target,
                    squad.attack_target,
                )
            })
        else {
            return false;
        };
        match phase {
            AmbientLifePhase::Moving if state == SquadState::Idle && move_target.is_none() => {
                self.finish_ambient_move(squad_id)
            }
            AmbientLifePhase::Moving
                if state != SquadState::Moving || move_target != owned_move =>
            {
                self.fail_ambient_move(squad_id)
            }
            AmbientLifePhase::Attacking
                if current_prey
                    .is_some_and(|id| self.units.get(id).is_none_or(|unit| !unit.is_alive())) =>
            {
                self.enter_ambient_devour(squad_id);
                true
            }
            AmbientLifePhase::Attacking
                if state != SquadState::Attacking || attack_target != current_prey =>
            {
                self.finish_ambient_attack(squad_id);
                true
            }
            _ => true,
        }
    }

    fn update_ambient_opportunity_timer(
        &mut self,
        squad_id: EntityId,
        elapsed_ms: u32,
        gameplay: &GameplayCatalog,
        profile: &AmbientLifeProfile,
    ) {
        let should_scan = self.squads.get_mut(squad_id).is_some_and(|squad| {
            let state = &mut squad.ambient_life;
            state.behavior != AmbientLifeBehavior::Devour
                && countdown_due(&mut state.predator_timer_ms, elapsed_ms)
        });
        if !should_scan {
            return;
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.predator_timer_ms = profile.predator_check_frequency_ms();
        }
        self.scan_ambient_opportunities(squad_id, gameplay, profile.opportunity_check_radius());
        self.select_ambient_opportunity_behavior(squad_id);
    }

    fn scan_ambient_opportunities(
        &mut self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
        radius: f32,
    ) {
        let Some((center, leaving_map)) = self
            .squads
            .get(squad_id)
            .map(|squad| (squad.base.position, squad.ambient_life.leaving_map))
        else {
            return;
        };
        if leaving_map {
            return;
        }
        let mut dangerous = None;
        let mut prey = None;
        for (other_id, other) in self.squads.iter() {
            if other_id == squad_id
                || !other.is_alive()
                || other.unit_ids.is_empty()
                || !inside_square(center, other.base.position, radius)
            {
                continue;
            }
            if other.base.player_id != GAIA_PLAYER
                || self.ambient_squad_can_attack(other_id, squad_id, gameplay)
            {
                dangerous = Some(other_id);
            }
            if self.ambient_squad_can_attack(squad_id, other_id, gameplay) {
                prey = Some(other_id);
            }
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.dangerous_squad = dangerous;
            squad.ambient_life.prey_squad = prey;
        }
    }

    fn select_ambient_opportunity_behavior(&mut self, squad_id: EntityId) {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            if squad.ambient_life.dangerous_squad.is_some() {
                squad.ambient_life.behavior = AmbientLifeBehavior::Flee;
            } else if squad.ambient_life.prey_squad.is_some() {
                squad.ambient_life.behavior = AmbientLifeBehavior::Hunt;
            }
        }
    }

    fn ambient_squad_can_attack(
        &self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(source_squad) = self.squads.get(source_squad_id) else {
            return false;
        };
        let Some(source) = source_squad
            .unit_ids
            .first()
            .and_then(|id| self.units.get(*id))
            .filter(|unit| unit.is_alive())
        else {
            return false;
        };
        let Some(target) = self.ambient_target_unit(target_squad_id) else {
            return false;
        };
        let query = self.ambient_attack_query(source_squad.mode, source, target);
        gameplay
            .select_ranged_action(&source.proto_object_name, &query, |action| {
                self.ambient_action_available(source, action.name.as_str(), action.start_disabled)
            })
            .is_some()
    }

    fn ambient_target_unit(&self, squad_id: EntityId) -> Option<&Unit> {
        self.squads
            .get(squad_id)?
            .unit_ids
            .iter()
            .find_map(|id| self.units.get(*id).filter(|unit| unit.is_alive()))
    }

    fn ambient_attack_query<'target>(
        &self,
        squad_mode: SquadMode,
        source: &Unit,
        target: &'target Unit,
    ) -> AttackQuery<'target> {
        let mut flags = AttackQueryFlags::empty();
        flags.insert(AttackQueryFlags::AUTO_TARGET);
        if target.base.player_id == GAIA_PLAYER {
            flags.insert(AttackQueryFlags::TARGET_GAIA);
        }
        if target.hitpoints < target.max_hitpoints {
            flags.insert(AttackQueryFlags::TARGET_DAMAGED);
        }
        if !target.built {
            flags.insert(AttackQueryFlags::TARGET_UNBUILT);
        }
        AttackQuery {
            relation: self.ambient_tactic_relation(source.base.player_id, target.base.player_id),
            squad_mode,
            ability_id: None,
            target_proto_object_name: Some(&target.proto_object_name),
            tactic_state: source.tactic_state(),
            flags,
        }
    }

    fn ambient_action_available(
        &self,
        source: &Unit,
        action_name: &str,
        start_disabled: Option<bool>,
    ) -> bool {
        let authored_enabled = start_disabled != Some(true);
        let player_enabled =
            self.get_player(source.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &source.proto_object_name,
                        action_name,
                        authored_enabled,
                    )
                });
        source.actions.is_enabled(action_name, !player_enabled)
    }

    fn ambient_tactic_relation(&self, source: PlayerId, target: PlayerId) -> TacticRelation {
        if source == target {
            return TacticRelation::SelfPlayer;
        }
        match self.player_relation(source, target) {
            Some(TeamRelation::Ally) => TacticRelation::Ally,
            Some(TeamRelation::Enemy) => TacticRelation::Enemy,
            Some(TeamRelation::Neutral) | None => TacticRelation::Neutral,
        }
    }

    fn update_ambient_behavior(
        &mut self,
        squad_id: EntityId,
        elapsed_ms: u32,
        profile: &AmbientLifeProfile,
    ) {
        let behavior = self
            .squads
            .get(squad_id)
            .map(|squad| squad.ambient_life.behavior);
        match behavior {
            Some(AmbientLifeBehavior::Wander) => {
                self.update_ambient_wander(squad_id, elapsed_ms, profile);
            }
            Some(AmbientLifeBehavior::Flee) => self.begin_ambient_flee(squad_id, profile),
            Some(AmbientLifeBehavior::Hunt) => {
                self.update_ambient_hunt(squad_id, elapsed_ms, profile);
            }
            Some(AmbientLifeBehavior::Devour) => {
                self.update_ambient_devour(squad_id, elapsed_ms);
            }
            Some(AmbientLifeBehavior::Idle) | None => {}
        }
    }

    fn update_ambient_wander(
        &mut self,
        squad_id: EntityId,
        elapsed_ms: u32,
        profile: &AmbientLifeProfile,
    ) {
        let due = self.squads.get_mut(squad_id).is_some_and(|squad| {
            countdown_due(&mut squad.ambient_life.wander_timer_ms, elapsed_ms)
        });
        if !due {
            return;
        }
        let next_timer = self.trigger_random_index(profile.max_wander_frequency_ms());
        let Some(leader_position) = self.ambient_leader_position(squad_id) else {
            return;
        };
        let target = self.random_circular_position(
            leader_position,
            profile.maximum_wander_distance(),
            profile.minimum_wander_distance(),
        );
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.wander_timer_ms = next_timer;
        }
        self.begin_ambient_move(squad_id, target, false, profile.flee_movement_modifier());
    }

    fn begin_ambient_flee(&mut self, squad_id: EntityId, profile: &AmbientLifeProfile) {
        let Some((leader_position, danger_position, leaving_map)) =
            self.ambient_flee_positions(squad_id)
        else {
            return;
        };
        let mut direction = leader_position - danger_position;
        direction.y = 0.0;
        direction = direction.normalize_or_zero();
        let deviation = self.trigger_random_float(-4.0, 4.0) * direction.cross(Vec3::Y);
        direction = (direction + deviation).normalize_or_zero();
        let distance = if leaving_map {
            LEAVING_MAP_DISTANCE
        } else {
            self.trigger_random_float(profile.flee_distance() * 0.5, profile.flee_distance())
        };
        self.begin_ambient_move(
            squad_id,
            leader_position + direction * distance,
            true,
            profile.flee_movement_modifier(),
        );
    }

    fn ambient_flee_positions(&self, squad_id: EntityId) -> Option<(Vec3, Vec3, bool)> {
        let squad = self.squads.get(squad_id)?;
        let danger = self.squads.get(squad.ambient_life.dangerous_squad?)?;
        Some((
            self.ambient_leader_position(squad_id)?,
            self.ambient_leader_position(danger.base.id)?,
            squad.ambient_life.leaving_map,
        ))
    }

    fn ambient_leader_position(&self, squad_id: EntityId) -> Option<Vec3> {
        self.squads
            .get(squad_id)?
            .unit_ids
            .first()
            .and_then(|id| self.units.get(*id))
            .filter(|unit| unit.is_alive())
            .map(|unit| unit.base.position)
    }

    fn begin_ambient_move(
        &mut self,
        squad_id: EntityId,
        target: Vec3,
        fleeing: bool,
        movement_modifier: f32,
    ) {
        let target = self.clamp_ambient_target(target);
        let accepted = self.squads.get_mut(squad_id).is_some_and(|squad| {
            squad.remove_all_orders();
            squad.issue_scripted_move(target, false, false)
        });
        if let Some(squad) = self.squads.get_mut(squad_id) {
            let state = &mut squad.ambient_life;
            state.target_position = Some(target);
            state.owned_move_target = accepted.then_some(target);
            state.phase = if accepted {
                AmbientLifePhase::Moving
            } else {
                AmbientLifePhase::Working
            };
            state.behavior = AmbientLifeBehavior::Idle;
            if fleeing && !state.fleeing {
                state.fleeing = true;
                state.movement_modifier = movement_modifier;
            }
        }
    }

    fn clamp_ambient_target(&self, mut target: Vec3) -> Vec3 {
        if let Some(bounds) = self.terrain_bounds() {
            target.x = target.x.clamp(bounds.min_x(), bounds.max_x());
            target.z = target.z.clamp(bounds.min_z(), bounds.max_z());
        }
        target
    }

    fn update_ambient_hunt(
        &mut self,
        squad_id: EntityId,
        elapsed_ms: u32,
        profile: &AmbientLifeProfile,
    ) {
        let due = self
            .squads
            .get_mut(squad_id)
            .is_some_and(|squad| countdown_due(&mut squad.ambient_life.prey_timer_ms, elapsed_ms));
        if !due {
            return;
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.prey_timer_ms = profile.prey_check_frequency_ms();
        }
        self.begin_ambient_attack(squad_id);
    }

    fn begin_ambient_attack(&mut self, squad_id: EntityId) {
        let Some((position, prey_squad_id, previous)) =
            self.squads.get(squad_id).and_then(|squad| {
                Some((
                    squad.base.position,
                    squad.ambient_life.prey_squad?,
                    squad.ambient_life.current_prey_unit,
                ))
            })
        else {
            return;
        };
        let selected = self.nearest_ambient_prey_unit(prey_squad_id, position);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.current_prey_unit = selected;
            squad.ambient_life.target_position = None;
        }
        if selected.is_none() || selected == previous {
            return;
        }
        let accepted = self.squads.get_mut(squad_id).is_some_and(|squad| {
            squad.attack(
                selected.expect("selected prey was checked"),
                0.0,
                None,
                None,
            )
        });
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.phase = if accepted {
                AmbientLifePhase::Attacking
            } else {
                AmbientLifePhase::Working
            };
            squad.ambient_life.behavior = AmbientLifeBehavior::Hunt;
        }
    }

    fn nearest_ambient_prey_unit(&self, squad_id: EntityId, position: Vec3) -> Option<EntityId> {
        self.squads
            .get(squad_id)?
            .unit_ids
            .iter()
            .filter_map(|id| {
                self.units
                    .get(*id)
                    .filter(|unit| unit.is_alive())
                    .map(|unit| (*id, planar_distance_squared(position, unit.base.position)))
            })
            .min_by(|(first_id, first), (second_id, second)| {
                first.total_cmp(second).then(first_id.cmp(second_id))
            })
            .map(|(id, _)| id)
    }

    fn update_ambient_devour(&mut self, squad_id: EntityId, elapsed_ms: u32) {
        let due = self.squads.get_mut(squad_id).is_some_and(|squad| {
            countdown_due(&mut squad.ambient_life.devour_timer_ms, elapsed_ms)
        });
        if due && let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.devour_timer_ms = DEVOUR_DURATION_MS;
            squad.ambient_life.behavior = AmbientLifeBehavior::Wander;
        }
    }

    fn finish_ambient_move(&mut self, squad_id: EntityId) -> bool {
        let Some((was_fleeing, leaving_map)) = self.squads.get_mut(squad_id).map(|squad| {
            let state = &mut squad.ambient_life;
            let status = (state.fleeing, state.leaving_map);
            state.phase = AmbientLifePhase::Working;
            state.behavior = AmbientLifeBehavior::Wander;
            state.owned_move_target = None;
            if state.fleeing {
                state.fleeing = false;
                state.movement_modifier = 1.0;
            }
            status
        }) else {
            return false;
        };
        if !was_fleeing && leaving_map {
            return self.kill_squad(squad_id, false);
        }
        true
    }

    fn fail_ambient_move(&mut self, squad_id: EntityId) -> bool {
        let Some((was_fleeing, leaving_map)) = self.squads.get_mut(squad_id).map(|squad| {
            let state = &mut squad.ambient_life;
            let status = (state.fleeing, state.leaving_map);
            state.phase = AmbientLifePhase::Starting;
            state.owned_move_target = None;
            if state.fleeing {
                state.fleeing = false;
                state.movement_modifier = 1.0;
            }
            status
        }) else {
            return false;
        };
        if !was_fleeing && leaving_map {
            return self.kill_squad(squad_id, false);
        }
        true
    }

    fn finish_ambient_attack(&mut self, squad_id: EntityId) {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ambient_life.phase = AmbientLifePhase::Working;
            squad.ambient_life.behavior = AmbientLifeBehavior::Wander;
        }
    }

    fn enter_ambient_devour(&mut self, squad_id: EntityId) {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.clear_attack_order();
            let state = &mut squad.ambient_life;
            state.prey_squad = None;
            state.current_prey_unit = None;
            state.target_position = None;
            state.phase = AmbientLifePhase::Working;
            state.behavior = AmbientLifeBehavior::Devour;
        }
    }

    pub(super) fn notify_ambient_life_damaged(
        &mut self,
        damaged_unit_id: EntityId,
        attacker_unit_id: Option<EntityId>,
    ) {
        let Some(squad_id) = self
            .units
            .get(damaged_unit_id)
            .and_then(|unit| unit.squad_id)
        else {
            return;
        };
        let attacker_squad = attacker_unit_id
            .and_then(|id| self.units.get(id))
            .and_then(|unit| unit.squad_id);
        if let Some(squad) = self.squads.get_mut(squad_id)
            && squad.ambient_life.is_initialized()
            && !squad.ambient_life.fleeing
        {
            squad.ambient_life.dangerous_squad = attacker_squad;
            squad.ambient_life.behavior = AmbientLifeBehavior::Flee;
        }
    }

    pub(super) fn notify_ambient_life_killed_unit(
        &mut self,
        attacker_unit_id: EntityId,
        killed_unit_id: EntityId,
    ) {
        let Some(squad_id) = self
            .units
            .get(attacker_unit_id)
            .and_then(|unit| unit.squad_id)
        else {
            return;
        };
        let owns_attack = self.squads.get(squad_id).is_some_and(|squad| {
            squad.ambient_life.phase == AmbientLifePhase::Attacking
                && squad.ambient_life.current_prey_unit == Some(killed_unit_id)
        });
        if owns_attack {
            self.enter_ambient_devour(squad_id);
        }
    }

    /// Ask a connected ambient-life action to flee toward the map boundary.
    pub fn flee_ambient_life_from_map(
        &mut self,
        squad_id: EntityId,
        dangerous_squad_id: Option<EntityId>,
    ) -> bool {
        let valid_danger =
            dangerous_squad_id.filter(|id| self.squads.get(*id).is_some_and(Entity::is_alive));
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        if !squad.ambient_life.is_initialized() {
            return false;
        }
        squad.ambient_life.leaving_map = true;
        squad.ambient_life.dangerous_squad = valid_danger;
        true
    }

    fn disconnect_ambient_life(&mut self, squad_id: EntityId) {
        let Some((owned_move, owned_attack, active_move, active_attack)) =
            self.squads.get_mut(squad_id).map(|squad| {
                let (owned_move, owned_attack) = squad.ambient_life.disconnect();
                (
                    owned_move,
                    owned_attack,
                    squad.move_target,
                    squad.attack_target,
                )
            })
        else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            if owned_move.is_some() && owned_move == active_move {
                squad.stop();
            }
            if owned_attack.is_some() && owned_attack == active_attack {
                squad.clear_attack_order();
            }
        }
    }

    pub(super) fn prepare_remove_squad_ambient_life(&mut self, squad_id: EntityId) {
        self.disconnect_ambient_life(squad_id);
        let observer_ids = self.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for observer_id in observer_ids {
            let clear_attack = self.squads.get_mut(observer_id).is_some_and(|squad| {
                let state = &mut squad.ambient_life;
                if state.dangerous_squad == Some(squad_id) {
                    state.dangerous_squad = None;
                }
                if state.prey_squad == Some(squad_id) {
                    state.prey_squad = None;
                    state.current_prey_unit = None;
                    return state.phase == AmbientLifePhase::Attacking;
                }
                false
            });
            if clear_attack {
                self.finish_ambient_attack(observer_id);
                if let Some(squad) = self.squads.get_mut(observer_id) {
                    squad.clear_attack_order();
                }
            }
        }
    }

    pub(super) fn prepare_remove_unit_ambient_life(&mut self, unit_id: EntityId) {
        if let Some(squad_id) = self.units.get(unit_id).and_then(|unit| unit.squad_id) {
            self.disconnect_ambient_life(squad_id);
        }
        let observer_ids = self.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for observer_id in observer_ids {
            let clear_attack = self.squads.get_mut(observer_id).is_some_and(|squad| {
                if squad.ambient_life.current_prey_unit != Some(unit_id) {
                    return false;
                }
                squad.ambient_life.current_prey_unit = None;
                squad.ambient_life.prey_squad = None;
                squad.ambient_life.phase == AmbientLifePhase::Attacking
            });
            if clear_attack {
                self.finish_ambient_attack(observer_id);
                if let Some(squad) = self.squads.get_mut(observer_id) {
                    squad.clear_attack_order();
                }
            }
        }
    }

    pub(super) fn prepare_squad_membership_change_ambient_life(&mut self, squad_id: EntityId) {
        self.disconnect_ambient_life(squad_id);
    }
}

fn elapsed_milliseconds(seconds: f32) -> Option<u32> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    (seconds * 1_000.0).round_ties_even().to_u32()
}

fn inside_square(center: Vec3, candidate: Vec3, radius: f32) -> bool {
    radius.is_finite()
        && radius >= 0.0
        && (candidate.x - center.x).abs() <= radius
        && (candidate.z - center.z).abs() <= radius
}

fn planar_distance_squared(first: Vec3, second: Vec3) -> f32 {
    let delta = first - second;
    delta.x.mul_add(delta.x, delta.z * delta.z)
}

#[cfg(test)]
mod tests;

mod spawner;
