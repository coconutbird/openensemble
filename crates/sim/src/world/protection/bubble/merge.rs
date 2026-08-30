//! Immediate Merge joins and target-side synthetic squad lifecycle.

use super::{GameplayCatalog, JoinActionProfile, JoinTargetSnapshot, World};
use crate::entities::{Squad, SquadMergeState, Unit};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::scenario::refresh_squad_member_settings;

#[cfg(test)]
mod tests;

impl World {
    pub(super) fn update_merge_join(
        &mut self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        target: &JoinTargetSnapshot,
        action: &JoinActionProfile,
        gameplay: &GameplayCatalog,
    ) {
        if !self.update_join_follow(
            source_squad_id,
            target_squad_id,
            target.position,
            action.work_range(),
        ) {
            return;
        }
        if self.join_channel_occupied(source_squad_id, target_squad_id, action.merge_type()) {
            self.disconnect_join_owner(source_squad_id);
            return;
        }
        let Some(profile) = self.merge_profile(source_squad_id, target_squad_id, gameplay) else {
            self.disconnect_join_owner(source_squad_id);
            return;
        };
        let Some(joining_unit_id) = self.single_merge_unit(source_squad_id, target_squad_id) else {
            self.disconnect_join_owner(source_squad_id);
            return;
        };
        let Some(state) = self.build_merge_state(
            source_squad_id,
            target_squad_id,
            joining_unit_id,
            action,
            gameplay,
        ) else {
            self.disconnect_join_owner(source_squad_id);
            return;
        };
        if !self.attach_unit_to_squad(joining_unit_id, target_squad_id) {
            self.disconnect_join_owner(source_squad_id);
            return;
        }

        let joining_costs = state.joining_population_costs().to_vec();
        let target_costs = state.target_population_costs().to_vec();
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.population_costs.clear();
        }
        if let Some(target) = self.squads.get_mut(target_squad_id) {
            target.population_costs = target_costs;
            target.population_costs.extend(joining_costs);
            target.proto_squad_id = profile.0;
            target.proto_squad_name = profile.1;
            target.set_merge_state(state);
        }
        self.apply_merge_buffs(target_squad_id);
        refresh_squad_member_settings(self, target_squad_id);
        let _removed = self.remove_squad(source_squad_id);
    }

    pub(super) fn update_merged_squads(&mut self) {
        let merged_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| squad.merge_state().is_some().then_some(id))
            .collect::<Vec<_>>();
        for squad_id in merged_ids {
            self.update_merged_squad(squad_id);
        }
    }

    fn update_merged_squad(&mut self, squad_id: EntityId) {
        let Some((joining_unit_id, live_members)) = self.squads.get(squad_id).and_then(|squad| {
            let joining_unit_id = squad.merge_state()?.joining_unit_id();
            let live_members = squad
                .unit_ids
                .iter()
                .filter(|unit_id| self.units.get(**unit_id).is_some_and(Entity::is_alive))
                .copied()
                .collect::<Vec<_>>();
            Some((joining_unit_id, live_members))
        }) else {
            return;
        };
        if !live_members.contains(&joining_unit_id) {
            self.revert_merged_squad(squad_id, false);
        } else if live_members.len() == 1 {
            self.revert_merged_squad(squad_id, true);
        } else {
            self.apply_merge_buffs(squad_id);
        }
    }

    fn merge_profile(
        &self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<(i32, String)> {
        let source_name = &self.squads.get(source_squad_id)?.proto_squad_name;
        let target = self.squads.get(target_squad_id)?;
        if target.merge_state().is_some() {
            return None;
        }
        gameplay
            .merged_squad_profile(source_name, &target.proto_squad_name)
            .map(|profile| {
                (
                    profile.proto_squad_id(),
                    profile.proto_squad_name().to_owned(),
                )
            })
    }

    fn single_merge_unit(
        &self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
    ) -> Option<EntityId> {
        let source = self.squads.get(source_squad_id)?;
        let target = self.squads.get(target_squad_id)?;
        if source.base.player_id != target.base.player_id || source.unit_ids.len() != 1 {
            return None;
        }
        let unit_id = source.unit_ids[0];
        self.units
            .get(unit_id)
            .is_some_and(Unit::is_operational)
            .then_some(unit_id)
    }

    fn build_merge_state(
        &self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        joining_unit_id: EntityId,
        action: &JoinActionProfile,
        gameplay: &GameplayCatalog,
    ) -> Option<SquadMergeState> {
        let source = self.squads.get(source_squad_id)?;
        let target = self.squads.get(target_squad_id)?;
        let joining_proto_object = &self.units.get(joining_unit_id)?.proto_object_name;
        let target_proto_squad = self.get_player(target.base.player_id).map_or(
            target.proto_squad_name.as_str(),
            |player| {
                player
                    .technologies
                    .resolved_squad_prototype(&target.proto_squad_name)
            },
        );
        let modifiers = gameplay.resolve_join_damage_modifiers(
            action,
            joining_proto_object,
            target_proto_squad,
        );
        Some(SquadMergeState::new(
            joining_unit_id,
            action.merge_type(),
            source,
            target,
            modifiers,
        ))
    }

    fn apply_merge_buffs(&mut self, squad_id: EntityId) {
        let Some((joiner, damage, damage_taken, unit_ids, already_buffed)) =
            self.squads.get(squad_id).and_then(|squad| {
                let state = squad.merge_state()?;
                Some((
                    state.joining_unit_id(),
                    state.damage_modifier(),
                    state.damage_taken_modifier(),
                    squad.unit_ids.clone(),
                    state.buffed_unit_ids().to_vec(),
                ))
            })
        else {
            return;
        };
        for unit_id in unit_ids {
            if unit_id == joiner || already_buffed.binary_search(&unit_id).is_ok() {
                continue;
            }
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            unit.set_join_damage_modifiers(damage, damage_taken);
            if let Some(state) = self
                .squads
                .get_mut(squad_id)
                .and_then(Squad::merge_state_mut)
            {
                let _inserted = state.mark_buffed(unit_id);
            }
        }
    }

    fn revert_merged_squad(&mut self, squad_id: EntityId, to_joining_squad: bool) {
        let Some((player_id, state)) = self.squads.get_mut(squad_id).and_then(|squad| {
            let player_id = squad.base.player_id;
            squad.take_merge_state().map(|state| (player_id, state))
        }) else {
            return;
        };
        for unit_id in state.buffed_unit_ids() {
            if let Some(unit) = self.units.get_mut(*unit_id) {
                unit.clear_join_damage_modifiers();
            }
        }
        let discarded_costs = if to_joining_squad {
            state.target_population_costs()
        } else {
            state.joining_population_costs()
        };
        if let Some(player) = self.get_player_mut(player_id) {
            player.release_population(discarded_costs);
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            if to_joining_squad {
                squad.proto_squad_id = state.joining_proto_squad_id();
                state
                    .joining_proto_squad_name()
                    .clone_into(&mut squad.proto_squad_name);
                squad.population_costs = state.joining_population_costs().to_vec();
                state.restore_joining_settings(squad);
            } else {
                squad.proto_squad_id = state.target_proto_squad_id();
                state
                    .target_proto_squad_name()
                    .clone_into(&mut squad.proto_squad_name);
                squad.population_costs = state.target_population_costs().to_vec();
            }
        }
        refresh_squad_member_settings(self, squad_id);
    }

    pub(super) fn abandon_merged_squad(&mut self, squad_id: EntityId) {
        let state = self
            .squads
            .get_mut(squad_id)
            .and_then(Squad::take_merge_state);
        let Some(state) = state else {
            return;
        };
        for unit_id in state.buffed_unit_ids() {
            if let Some(unit) = self.units.get_mut(*unit_id) {
                unit.clear_join_damage_modifiers();
            }
        }
    }
}
