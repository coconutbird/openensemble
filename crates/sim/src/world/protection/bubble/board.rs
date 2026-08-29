//! Timed Board joins, ownership transfer, and Spartan containment.

use super::{GameplayCatalog, JoinActionProfile, JoinTargetSnapshot, World};
use crate::entities::{Squad, SquadBoardState, Unit};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use glam::Vec3;

#[cfg(test)]
mod tests;

impl World {
    pub(super) fn update_board_join(
        &mut self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        target: &JoinTargetSnapshot,
        action: &JoinActionProfile,
        gameplay: &GameplayCatalog,
        dt: f32,
    ) {
        let board = self
            .squads
            .get(source_squad_id)
            .and_then(Squad::board_state)
            .map(|state| (state.target_unit_id(), state.is_complete()));
        match board {
            Some((target_unit_id, true)) => {
                if self.board_target_is_valid(target_squad_id, target_unit_id) {
                    self.apply_board_buffs(source_squad_id, target_squad_id);
                } else {
                    self.disconnect_board_join(source_squad_id);
                }
            }
            Some((target_unit_id, false)) => {
                if !self.board_target_is_valid(target_squad_id, target_unit_id) {
                    self.disconnect_board_join(source_squad_id);
                    return;
                }
                self.synchronize_boarding_source(source_squad_id, target);
                let ready = self
                    .squads
                    .get_mut(source_squad_id)
                    .and_then(Squad::board_state_mut)
                    .is_some_and(|state| state.advance(dt));
                if ready {
                    self.complete_board_join(source_squad_id, target_squad_id);
                }
            }
            None => {
                self.start_board_join(source_squad_id, target_squad_id, target, action, gameplay);
            }
        }
    }

    fn start_board_join(
        &mut self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
        target: &JoinTargetSnapshot,
        action: &JoinActionProfile,
        gameplay: &GameplayCatalog,
    ) {
        if !self.update_join_follow(source_squad_id, target.position, action.work_range()) {
            return;
        }
        if self.join_channel_occupied(source_squad_id, target_squad_id, action.merge_type()) {
            self.disconnect_join_owner(source_squad_id);
            return;
        }
        let Some((joining_unit_id, target_unit_id, former_owner)) =
            self.board_participants(source_squad_id, target_squad_id)
        else {
            self.disconnect_join_owner(source_squad_id);
            return;
        };
        let joining_proto_object = &self.units.get(joining_unit_id).unwrap().proto_object_name;
        let target_proto_squad = self
            .squads
            .get(target_squad_id)
            .map(|squad| {
                self.get_player(squad.base.player_id).map_or(
                    squad.proto_squad_name.as_str(),
                    |player| {
                        player
                            .technologies
                            .resolved_squad_prototype(&squad.proto_squad_name)
                    },
                )
            })
            .unwrap_or_default();
        let modifiers = gameplay.resolve_join_damage_modifiers(
            action,
            joining_proto_object,
            target_proto_squad,
        );
        let veterancy_enabled = self.veterancy_enabled();
        let source_veterancy_level = if veterancy_enabled {
            self.squads
                .get(source_squad_id)
                .map_or(0, Squad::veterancy_level)
        } else {
            0
        };
        let veterancy_modifiers = if veterancy_enabled {
            gameplay.join_veterancy_modifiers(joining_proto_object, 0, source_veterancy_level)
        } else {
            Default::default()
        };
        let attachment = gameplay.resolve_join_attachment(action).map(|profile| {
            (
                profile.proto_object_id(),
                profile.proto_object_name().to_owned(),
            )
        });
        let state = SquadBoardState::new(
            target_unit_id,
            former_owner,
            (
                action.board_time(),
                action.revert_damage_fraction(),
                action.unjoin_max_distance(),
            ),
            (
                veterancy_enabled && action.veterancy_override(),
                source_veterancy_level,
                if veterancy_enabled {
                    action.levels()
                } else {
                    0
                },
            ),
            veterancy_modifiers,
            modifiers,
            attachment,
        );
        if let Some(target_unit) = self.units.get_mut(target_unit_id) {
            target_unit.set_being_boarded(true);
        }
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.mark_boarding(action.merge_type(), state);
        }
        self.synchronize_boarding_source(source_squad_id, target);
        if action.board_time() <= 0.0 {
            self.complete_board_join(source_squad_id, target_squad_id);
        } else if let Some(unit) = self.units.get_mut(joining_unit_id) {
            unit.stop();
        }
    }

    fn board_participants(
        &self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
    ) -> Option<(EntityId, EntityId, crate::player::PlayerId)> {
        let source = self.squads.get(source_squad_id)?;
        if source.unit_ids.len() != 1
            || !self
                .units
                .get(source.unit_ids[0])
                .is_some_and(Unit::is_operational)
        {
            return None;
        }
        let target = self.squads.get(target_squad_id)?;
        let target_unit_id = target
            .unit_ids
            .iter()
            .find(|unit_id| self.units.get(**unit_id).is_some_and(Unit::is_operational))
            .copied()?;
        (!self.units.get(target_unit_id)?.is_being_boarded()).then_some((
            source.unit_ids[0],
            target_unit_id,
            target.base.player_id,
        ))
    }

    fn board_target_is_valid(&self, target_squad_id: EntityId, target_unit_id: EntityId) -> bool {
        self.squads
            .get(target_squad_id)
            .is_some_and(|squad| squad.contains_unit(target_unit_id))
            && self.units.get(target_unit_id).is_some_and(Entity::is_alive)
    }

    fn synchronize_boarding_source(
        &mut self,
        source_squad_id: EntityId,
        target: &JoinTargetSnapshot,
    ) {
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.base.position = target.position;
            source.base.forward = target.forward;
            source.stop();
        }
        self.place_squad_members(source_squad_id, target.position, target.forward, false);
    }

    fn complete_board_join(&mut self, source_squad_id: EntityId, target_squad_id: EntityId) {
        let Some((source_owner, target_owner, target_unit_id)) =
            self.squads.get(source_squad_id).and_then(|source| {
                let board = source.board_state()?;
                Some((
                    source.base.player_id,
                    self.squads.get(target_squad_id)?.base.player_id,
                    board.target_unit_id(),
                ))
            })
        else {
            return;
        };
        if self.players_are_enemies(source_owner, target_owner)
            && !self.change_squad_owner(target_squad_id, source_owner)
        {
            self.disconnect_board_join(source_squad_id);
            return;
        }
        if self.contain_join_squad(source_squad_id, target_squad_id) != Some(target_unit_id) {
            self.disconnect_board_join(source_squad_id);
            return;
        }
        if let Some(target) = self.units.get_mut(target_unit_id) {
            target.set_being_boarded(false);
        }
        let unit_ids = self
            .squads
            .get(source_squad_id)
            .map(|squad| squad.unit_ids.clone())
            .unwrap_or_default();
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.mark_board_complete();
            source.base.set_selectable(false);
        }
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.set_invulnerable(true);
                unit.base.set_selectable(false);
            }
        }
        self.create_board_attachment(source_squad_id, target_unit_id);
        self.apply_board_veterancy(source_squad_id, target_unit_id);
        self.apply_board_buffs(source_squad_id, target_squad_id);
    }

    fn apply_board_veterancy(&mut self, source_squad_id: EntityId, target_unit_id: EntityId) {
        if !self.veterancy_enabled() {
            return;
        }
        let modifiers = self
            .squads
            .get(source_squad_id)
            .and_then(Squad::board_state)
            .filter(|board| board.veterancy_override() && !board.veterancy_was_applied())
            .map(SquadBoardState::veterancy_modifiers);
        let Some(modifiers) = modifiers else {
            return;
        };
        let Some(target) = self.units.get_mut(target_unit_id) else {
            return;
        };
        modifiers.apply(target);
        if let Some(board) = self
            .squads
            .get_mut(source_squad_id)
            .and_then(Squad::board_state_mut)
        {
            board.mark_veterancy_applied();
        }
    }

    fn create_board_attachment(&mut self, source_squad_id: EntityId, target_unit_id: EntityId) {
        let specification = self
            .squads
            .get(source_squad_id)
            .and_then(Squad::board_state)
            .and_then(SquadBoardState::attachment_spec)
            .map(|(prototype_id, name)| (prototype_id, name.to_owned()));
        let Some((prototype_id, prototype_name)) = specification else {
            return;
        };
        let Some(attachment_id) =
            self.add_visual_attachment_to_unit(target_unit_id, prototype_id, &prototype_name)
        else {
            return;
        };
        if let Some(board) = self
            .squads
            .get_mut(source_squad_id)
            .and_then(Squad::board_state_mut)
        {
            board.set_attachment_entity_id(attachment_id);
        }
    }

    fn apply_board_buffs(&mut self, source_squad_id: EntityId, target_squad_id: EntityId) {
        let Some((damage, damage_taken, already_buffed)) =
            self.squads.get(source_squad_id).and_then(|source| {
                let state = source.board_state()?;
                if !state.is_complete() {
                    return None;
                }
                Some((
                    state.damage_modifier(),
                    state.damage_taken_modifier(),
                    state.buffed_unit_ids().to_vec(),
                ))
            })
        else {
            return;
        };
        let target_units = self
            .squads
            .get(target_squad_id)
            .map(|squad| squad.unit_ids.clone())
            .unwrap_or_default();
        for unit_id in target_units {
            if already_buffed.binary_search(&unit_id).is_ok() {
                continue;
            }
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            unit.set_join_damage_modifiers(damage, damage_taken);
            if let Some(state) = self
                .squads
                .get_mut(source_squad_id)
                .and_then(Squad::board_state_mut)
            {
                let _inserted = state.mark_buffed(unit_id);
            }
        }
    }

    pub(super) fn disconnect_board_join(&mut self, source_squad_id: EntityId) {
        let Some((target_squad_id, state, source_position, source_forward, unit_ids)) =
            self.squads.get_mut(source_squad_id).and_then(|source| {
                let target_squad_id = source.join_target()?;
                let position = source.base.position;
                let forward = source.base.forward;
                let unit_ids = source.unit_ids.clone();
                source
                    .take_board_state()
                    .map(|state| (target_squad_id, state, position, forward, unit_ids))
            })
        else {
            return;
        };
        if let Some(target) = self.units.get_mut(state.target_unit_id()) {
            target.set_being_boarded(false);
        }
        if let Some(attachment_id) = state.attachment_entity_id() {
            let _removed = self.kill_entity(attachment_id, true);
        }
        for unit_id in state.buffed_unit_ids() {
            if let Some(unit) = self.units.get_mut(*unit_id) {
                unit.clear_join_damage_modifiers();
            }
        }
        let (target_position, target_forward) = self
            .squads
            .get(target_squad_id)
            .map_or((source_position, source_forward), |target| {
                (target.base.position, target.base.forward)
            });
        if state.is_complete() {
            let exit_position =
                board_exit_position(target_position, target_forward, state.unjoin_max_distance());
            self.release_join_passenger(source_squad_id, exit_position, target_forward);
        }
        if let Some(source) = self.squads.get_mut(source_squad_id) {
            source.cancel_join();
            source.base.set_selectable(true);
        }
        for unit_id in &unit_ids {
            if let Some(unit) = self.units.get_mut(*unit_id) {
                unit.set_invulnerable(false);
                unit.base.set_selectable(true);
            }
        }
        if state.is_complete() && state.revert_damage_fraction() > 0.0 {
            self.damage_boarding_unit(&unit_ids, state.revert_damage_fraction());
        }
    }

    fn damage_boarding_unit(&mut self, unit_ids: &[EntityId], fraction: f32) {
        let Some(unit_id) = unit_ids
            .iter()
            .find(|unit_id| self.units.get(**unit_id).is_some_and(Entity::is_alive))
            .copied()
        else {
            return;
        };
        let damage = self
            .units
            .get(unit_id)
            .map_or(0.0, |unit| unit.max_hitpoints * fraction.max(0.0));
        if let Some(unit) = self.units.get_mut(unit_id) {
            let _damaged = unit.damage(damage);
        }
    }
}

fn board_exit_position(position: Vec3, forward: Vec3, maximum_distance: f32) -> Vec3 {
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::Z);
    position - forward * maximum_distance.clamp(0.0, 2.0)
}
