//! Authoritative Follow/FollowAttack joins and optional `BubbleShield` support.

use super::World;
use crate::entities::{JoinKind, JoinMergeType, Squad};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{
    AttackQuery, AttackQueryFlags, BubbleShieldSquadProfile, GameplayCatalog, JoinActionProfile,
    TacticRelation,
};
use crate::player::{PlayerId, TeamRelation};
use crate::scenario::configure_unit_from_proto;
use glam::Vec3;

mod board;
mod merge;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
struct JoinTargetSnapshot {
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    speed: f32,
    proto_object_name: String,
    proto_squad_name: String,
    query_flags: AttackQueryFlags,
    last_damaged_time: u32,
    leader_regen_delay: f32,
}

impl World {
    /// Issue retail Join order 9 against a live non-building squad.
    pub fn issue_join_order(
        &mut self,
        player_id: PlayerId,
        source_squad_id: EntityId,
        requested_target_id: EntityId,
        ability_id: Option<u8>,
    ) -> bool {
        self.issue_join_order_internal(
            player_id,
            source_squad_id,
            requested_target_id,
            ability_id,
            false,
        )
    }

    pub(crate) fn issue_auto_join_order(
        &mut self,
        player_id: PlayerId,
        source_squad_id: EntityId,
        requested_target_id: EntityId,
    ) -> bool {
        self.issue_join_order_internal(player_id, source_squad_id, requested_target_id, None, true)
    }

    fn issue_join_order_internal(
        &mut self,
        player_id: PlayerId,
        source_squad_id: EntityId,
        requested_target_id: EntityId,
        ability_id: Option<u8>,
        allow_multiple: bool,
    ) -> bool {
        let Some(target_squad_id) = self.join_target_squad(requested_target_id) else {
            return false;
        };
        if source_squad_id == target_squad_id
            || !self.valid_join_source(player_id, source_squad_id)
            || !self.valid_join_target(target_squad_id)
        {
            return false;
        }
        let _cancelled = self.cancel_capture_order(source_squad_id);
        let _repair_cancelled = self.cancel_repair_other_order(source_squad_id);
        let Some(source) = self.squads.get_mut(source_squad_id) else {
            return false;
        };
        source.begin_join(target_squad_id, ability_id, allow_multiple);
        self.cancel_incoming_power_transport(source_squad_id);
        true
    }

    pub(in crate::world) fn update_protection(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.update_plasma_shields(dt, gameplay);
        self.update_joins(dt, gameplay);
    }

    fn update_joins(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.update_merged_squads();
        let source_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| squad.join_target().map(|_| id))
            .collect::<Vec<_>>();
        for source_squad_id in source_ids {
            self.update_join(source_squad_id, dt, gameplay);
        }
    }

    fn update_join(&mut self, source_squad_id: EntityId, dt: f32, gameplay: &GameplayCatalog) {
        let Some(target_squad_id) = self
            .squads
            .get(source_squad_id)
            .and_then(Squad::join_target)
        else {
            return;
        };
        let Some(target) = self.join_target_snapshot(target_squad_id) else {
            if self
                .squads
                .get(source_squad_id)
                .and_then(Squad::board_state)
                .is_some()
            {
                self.disconnect_board_join(source_squad_id);
            } else {
                let _killed = self.kill_squad(source_squad_id, false);
            }
            return;
        };
        let Some(action) = self.join_action_for_source(source_squad_id, &target, gameplay) else {
            self.disconnect_join_owner(source_squad_id);
            return;
        };
        match action.kind() {
            JoinKind::Merge => {
                self.update_merge_join(
                    source_squad_id,
                    target_squad_id,
                    &target,
                    &action,
                    gameplay,
                );
                return;
            }
            JoinKind::Board => {
                self.update_board_join(
                    source_squad_id,
                    target_squad_id,
                    &target,
                    &action,
                    gameplay,
                    dt,
                );
                return;
            }
            JoinKind::Follow | JoinKind::FollowAttack => {}
        }
        let owns_bubble = self
            .bubble_action_for_source(source_squad_id, gameplay)
            .is_some();

        let connected = self
            .squads
            .get(source_squad_id)
            .is_some_and(Squad::join_is_connected);
        let in_range = self.update_join_follow(
            source_squad_id,
            target_squad_id,
            target.position,
            action.work_range(),
        );
        if !connected
            && (!in_range
                || !self.connect_follow_join(
                    source_squad_id,
                    target_squad_id,
                    target.speed,
                    &action,
                    owns_bubble,
                ))
        {
            return;
        }
        self.update_follow_attack(
            source_squad_id,
            target_squad_id,
            dt,
            action.kind(),
            gameplay,
        );
        if !owns_bubble {
            return;
        }
        let Some(profile) = gameplay.bubble_shield_profile(&target.proto_squad_name) else {
            self.disconnect_join_owner(source_squad_id);
            return;
        };
        if self.synchronize_existing_bubble(source_squad_id, &target, profile) {
            return;
        }
        if !self.bubble_rebuild_ready(source_squad_id, &target, gameplay) {
            return;
        }
        self.create_bubble_shield(source_squad_id, target_squad_id, &target, profile);
    }

    fn update_join_follow(
        &mut self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        target_position: Vec3,
        work_range: f32,
    ) -> bool {
        let source_radius = self.squad_obstruction_radius(source_squad_id);
        let target_radius = self.squad_obstruction_radius(target_squad_id);
        let Some(source) = self.squads.get_mut(source_squad_id) else {
            return false;
        };
        let delta = Vec3::new(
            target_position.x - source.base.position.x,
            0.0,
            target_position.z - source.base.position.z,
        );
        let surface_distance = delta.length() - source_radius - target_radius;
        let in_range = surface_distance <= work_range.max(0.0);
        source.follow_join_target(target_position, work_range);
        in_range
    }

    fn connect_follow_join(
        &mut self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        target_speed: f32,
        action: &JoinActionProfile,
        requires_unproxied_target: bool,
    ) -> bool {
        if (requires_unproxied_target
            && self
                .squads
                .get(target_squad_id)
                .and_then(Squad::damage_proxy)
                .is_some())
            || self.join_channel_occupied(source_squad_id, target_squad_id, action.merge_type())
        {
            self.disconnect_join_owner(source_squad_id);
            return false;
        }
        let unit_ids = self
            .squads
            .get(source_squad_id)
            .map(|squad| squad.unit_ids.clone())
            .unwrap_or_default();
        if unit_ids.is_empty() {
            return false;
        }
        let follow_speed = target_speed * 2.0;
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.mark_join_connected(action.kind(), action.merge_type());
            source.base.set_selectable(false);
            if follow_speed.is_finite() && follow_speed >= 0.0 {
                source.speed = follow_speed;
            }
        }
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                let velocity_scalar = follow_speed / unit.speed;
                if unit.speed > 0.0 && velocity_scalar.is_finite() {
                    unit.velocity_scalar = velocity_scalar;
                }
                unit.set_invulnerable(true);
                unit.base.set_selectable(false);
            }
        }
        true
    }

    fn synchronize_existing_bubble(
        &mut self,
        source_squad_id: EntityId,
        target: &JoinTargetSnapshot,
        profile: &BubbleShieldSquadProfile,
    ) -> bool {
        let shield_squad_id = self
            .squads
            .get(source_squad_id)
            .and_then(Squad::bubble_shield_squad);
        let Some(shield_squad_id) = shield_squad_id else {
            return false;
        };
        let matches = self.squads.get(shield_squad_id).is_some_and(|shield| {
            shield.is_alive()
                && shield
                    .proto_squad_name
                    .eq_ignore_ascii_case(profile.proto_squad_name())
                && shield
                    .unit_ids
                    .first()
                    .and_then(|id| self.units.get(*id))
                    .is_some_and(Entity::is_alive)
        });
        if !matches {
            let damage_time = self
                .squads
                .get(shield_squad_id)
                .map_or(0, |shield| shield.last_damaged_time);
            if let Some(source) = self.squads.get_mut(source_squad_id) {
                source.note_bubble_damage_time(damage_time);
                source.set_bubble_shield_squad(None);
            }
            let _removed = self.kill_squad(shield_squad_id, true);
            return false;
        }
        self.place_bubble_squad(shield_squad_id, target.position, target.forward);
        true
    }

    fn bubble_rebuild_ready(
        &self,
        source_squad_id: EntityId,
        target: &JoinTargetSnapshot,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let source_damage_time = self
            .squads
            .get(source_squad_id)
            .map_or(0, Squad::last_bubble_damage_time);
        if source_damage_time == 0 {
            return true;
        }
        let last_damage_time = source_damage_time.max(target.last_damaged_time);
        let base_delay =
            self.get_player(target.player_id)
                .map_or(gameplay.shield_regen_delay(), |player| {
                    player
                        .technologies
                        .shield_regen_delay(gameplay.shield_regen_delay())
                });
        let required_seconds = base_delay.max(0.0) * target.leader_regen_delay.max(0.0);
        if !required_seconds.is_finite() {
            return false;
        }
        let required_ms = std::time::Duration::from_secs_f32(required_seconds).as_millis();
        u128::from(self.game_time_ms.wrapping_sub(last_damage_time)) > required_ms
    }

    fn create_bubble_shield(
        &mut self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        target: &JoinTargetSnapshot,
        profile: &BubbleShieldSquadProfile,
    ) {
        let shield_squad_id = self.create_squad_at(target.player_id, target.position);
        if let Some(shield) = self.squads.get_mut(shield_squad_id) {
            shield.proto_squad_id = profile.proto_squad_id();
            profile
                .proto_squad_name()
                .clone_into(&mut shield.proto_squad_name);
            shield.base.set_forward(target.forward);
            shield.base.set_selectable(false);
        }
        for member in profile.members() {
            for _ in 0..member.count() {
                let unit_id = self.create_unit_at(target.player_id, target.position);
                configure_unit_from_proto(
                    self,
                    unit_id,
                    member.proto_object_name(),
                    member.proto_object_index(),
                    member.proto_object(),
                );
                if !self.attach_unit_to_squad(unit_id, shield_squad_id) {
                    let _removed = self.remove_unit(unit_id);
                    continue;
                }
                if let Some(unit) = self.units.get_mut(unit_id) {
                    unit.base.set_forward(target.forward);
                    unit.base.set_selectable(false);
                    unit.set_auto_attackable(false);
                    unit.set_external_shield(true);
                    unit.shields.set_current(0.1);
                }
            }
        }
        if self
            .squads
            .get(shield_squad_id)
            .is_none_or(|shield| shield.unit_ids.is_empty())
        {
            let _removed = self.kill_squad(shield_squad_id, true);
            return;
        }
        if let Some(target_squad) = self.squads.get_mut(target_squad_id) {
            target_squad.set_damage_proxy(shield_squad_id);
        }
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.set_bubble_shield_squad(Some(shield_squad_id));
        }
        self.place_bubble_squad(shield_squad_id, target.position, target.forward);
    }

    fn place_bubble_squad(&mut self, squad_id: EntityId, position: Vec3, forward: Vec3) {
        let unit_ids = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
            .unwrap_or_default();
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.base.set_position(position);
            squad.base.set_forward(forward);
        }
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.base.set_position(position);
                unit.base.set_forward(forward);
            }
        }
    }

    fn join_target_snapshot(&self, target_squad_id: EntityId) -> Option<JoinTargetSnapshot> {
        let squad = self
            .squads
            .get(target_squad_id)
            .filter(|squad| squad.is_alive())?;
        let leader = squad
            .unit_ids
            .first()
            .and_then(|unit_id| self.units.get(*unit_id))
            .filter(|unit| unit.is_alive() && !unit.is_building())?;
        self.get_player(squad.base.player_id)?;
        let mut query_flags = AttackQueryFlags::empty();
        if squad.base.player_id == 0 {
            query_flags.insert(AttackQueryFlags::TARGET_GAIA);
        }
        if leader.hitpoints < leader.max_hitpoints {
            query_flags.insert(AttackQueryFlags::TARGET_DAMAGED);
        }
        if leader.is_building() && !leader.built {
            query_flags.insert(AttackQueryFlags::TARGET_UNBUILT);
        }
        if leader.shields.current > 0.0 {
            query_flags.insert(AttackQueryFlags::TARGET_SHIELDED);
        }
        if leader.is_invulnerable() {
            query_flags.insert(AttackQueryFlags::TARGET_INVULNERABLE);
        }
        Some(JoinTargetSnapshot {
            player_id: squad.base.player_id,
            position: squad.base.position,
            forward: squad.base.forward,
            speed: squad.speed,
            proto_object_name: leader.proto_object_name.clone(),
            proto_squad_name: squad.proto_squad_name.clone(),
            query_flags,
            last_damaged_time: squad.last_damaged_time,
            leader_regen_delay: leader.shields.regen_delay_scalar(),
        })
    }

    fn join_action_for_source(
        &self,
        source_squad_id: EntityId,
        target: &JoinTargetSnapshot,
        gameplay: &GameplayCatalog,
    ) -> Option<JoinActionProfile> {
        let source_squad = self.squads.get(source_squad_id)?;
        let contained_board = source_squad.board_state().is_some();
        let source = source_squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .find(|unit| unit.is_operational() || (contained_board && unit.is_alive()))?;
        let query = AttackQuery {
            relation: self.join_relation(source.base.player_id, target.player_id),
            squad_mode: source_squad.mode,
            ability_id: source_squad.join_ability_id(),
            target_proto_object_name: Some(&target.proto_object_name),
            tactic_state: source.tactic_state(),
            flags: target.query_flags,
        };
        gameplay.select_join_action_for_squads(
            &source.proto_object_name,
            Some(&source_squad.proto_squad_name),
            Some(&target.proto_squad_name),
            &query,
            |action| {
                let authored_enabled = action.start_disabled != Some(true);
                let player_enabled =
                    self.get_player(source.base.player_id)
                        .map_or(authored_enabled, |player| {
                            player.technologies.action_enabled(
                                &source.proto_object_name,
                                &action.name,
                                authored_enabled,
                            )
                        });
                source.actions.is_enabled(&action.name, !player_enabled)
            },
        )
    }

    fn bubble_action_for_source<'a>(
        &self,
        source_squad_id: EntityId,
        gameplay: &'a GameplayCatalog,
    ) -> Option<&'a crate::gameplay::BubbleShieldActionProfile> {
        self.squads
            .get(source_squad_id)?
            .unit_ids
            .iter()
            .find_map(|unit_id| self.units.get(*unit_id))
            .and_then(|unit| gameplay.bubble_shield_action(&unit.proto_object_name))
    }

    fn join_channel_occupied(
        &self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        merge_type: JoinMergeType,
    ) -> bool {
        if self
            .squads
            .get(source_squad_id)
            .is_some_and(Squad::join_allows_multiple)
        {
            return false;
        }
        self.squads
            .get(target_squad_id)
            .and_then(Squad::merge_state)
            .is_some_and(|state| state.merge_type() == merge_type)
            || self.squads.iter().any(|(other_id, squad)| {
                other_id != source_squad_id
                    && squad.join_target() == Some(target_squad_id)
                    && squad.join_occupies_channel()
                    && squad.join_merge_type() == Some(merge_type)
            })
    }

    fn update_follow_attack(
        &mut self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        dt: f32,
        kind: JoinKind,
        gameplay: &GameplayCatalog,
    ) {
        if kind != JoinKind::FollowAttack
            || !self
                .squads
                .get_mut(source_squad_id)
                .is_some_and(|squad| squad.advance_follow_attack_refresh(dt))
        {
            return;
        }
        let attack_target = self.validated_squad_attack_target(target_squad_id, gameplay);
        let unit_ids = self
            .squads
            .get(source_squad_id)
            .map(|squad| squad.unit_ids.clone())
            .unwrap_or_default();
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.set_join_attack_target(attack_target);
        }
        if attack_target.is_none() {
            self.stop_unit_firing(&unit_ids);
        }
    }

    fn join_relation(&self, source: PlayerId, target: PlayerId) -> TacticRelation {
        if source == target {
            return TacticRelation::SelfPlayer;
        }
        match self.player_relation(source, target) {
            Some(TeamRelation::Ally) => TacticRelation::Ally,
            Some(TeamRelation::Enemy) => TacticRelation::Enemy,
            Some(TeamRelation::Neutral) | None => TacticRelation::Neutral,
        }
    }

    fn join_target_squad(&self, requested_target_id: EntityId) -> Option<EntityId> {
        if self.squads.get(requested_target_id).is_some() {
            Some(requested_target_id)
        } else {
            self.units.get(requested_target_id)?.squad_id
        }
    }

    fn valid_join_source(&self, player_id: PlayerId, squad_id: EntityId) -> bool {
        self.squads.get(squad_id).is_some_and(|squad| {
            squad.base.player_id == player_id
                && squad.is_alive()
                && !self.is_squad_incapacitated(squad_id)
                && !squad.garrison.is_garrisoned()
                && squad.join_target().is_none()
                && !squad.unit_ids.is_empty()
        })
    }

    fn valid_join_target(&self, squad_id: EntityId) -> bool {
        self.squads.get(squad_id).is_some_and(|squad| {
            squad.is_alive()
                && squad.unit_ids.first().is_some_and(|unit_id| {
                    self.units.get(*unit_id).is_some_and(|unit| {
                        unit.is_alive()
                            && !unit.is_building()
                            && !unit.is_object_type("BuildingSocket")
                    })
                })
        })
    }

    fn disconnect_join_owner(&mut self, source_squad_id: EntityId) {
        if self
            .squads
            .get(source_squad_id)
            .and_then(Squad::board_state)
            .is_some()
        {
            self.disconnect_board_join(source_squad_id);
            return;
        }
        let shield_squad_id = self
            .squads
            .get(source_squad_id)
            .and_then(Squad::bubble_shield_squad);
        if let Some(shield_squad_id) = shield_squad_id {
            let _removed = self.kill_squad(shield_squad_id, true);
        }
        let unit_ids = self
            .squads
            .get(source_squad_id)
            .map(|squad| squad.unit_ids.clone())
            .unwrap_or_default();
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.cancel_join();
            source.set_bubble_shield_squad(None);
            source.base.set_selectable(true);
        }
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.set_invulnerable(false);
                unit.base.set_selectable(true);
            }
        }
    }

    pub(in crate::world) fn prepare_remove_squad_protection(&mut self, removed: EntityId) {
        self.abandon_merged_squad(removed);
        if self
            .squads
            .get(removed)
            .and_then(Squad::board_state)
            .is_some()
        {
            self.disconnect_board_join(removed);
        }
        let damage_time = self
            .squads
            .get(removed)
            .map_or(0, |squad| squad.last_damaged_time);
        let owners = self
            .squads
            .iter()
            .filter_map(|(id, squad)| (squad.bubble_shield_squad() == Some(removed)).then_some(id))
            .collect::<Vec<_>>();
        for owner_id in owners {
            if let Some(owner) = self.squads.get_mut(owner_id) {
                owner.note_bubble_damage_time(damage_time);
                owner.set_bubble_shield_squad(None);
            }
        }
        let owned_bubble = self
            .squads
            .get(removed)
            .and_then(Squad::bubble_shield_squad);
        if let Some(shield_squad_id) = owned_bubble.filter(|id| *id != removed) {
            let _removed = self.kill_squad(shield_squad_id, true);
        }
        let followers = self
            .squads
            .iter()
            .filter_map(|(id, squad)| {
                (id != removed && squad.join_target() == Some(removed)).then_some(id)
            })
            .collect::<Vec<_>>();
        for follower_id in followers {
            if self
                .squads
                .get(follower_id)
                .and_then(Squad::board_state)
                .is_some()
            {
                self.disconnect_board_join(follower_id);
            } else {
                let _removed = self.kill_squad(follower_id, true);
            }
        }
    }
}
