//! Authoritative squad and member lifecycle for retail `ChangeMode` orders.

mod automation;
mod lockdown_menu;

use super::World;
use crate::EntityId;
use crate::entities::{SquadChangeModePhase, SquadMode, UnitChangeModePhase};
use crate::entity::Entity;
use crate::gameplay::{AttackQuery, AttackQueryFlags, GameplayCatalog, TacticRelation};
use crate::player::PlayerId;
use num_traits::ToPrimitive;
use pipeline::database::hw1::tactics::Action;

#[derive(Debug, Clone, Copy)]
struct ChangeModeRequest {
    target_mode: SquadMode,
    ability_id: Option<u8>,
}

#[derive(Debug, Clone)]
struct MemberAnimation {
    unit_id: EntityId,
    animation_type: String,
    asset_path: String,
    duration_ms: u32,
}

impl World {
    /// Start one uninterruptible squad-mode transition.
    ///
    /// Explicit modes are used by direct building and trigger orders. When a
    /// work command omits the mode, retail selects a `ChangeMode` tactic action
    /// and falls back to toggling the concrete command ability's authored mode.
    pub fn issue_change_mode_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        requested_mode: Option<SquadMode>,
        requested_ability_id: Option<u8>,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(request) = self.resolve_change_mode_request(
            player_id,
            squad_id,
            requested_mode,
            requested_ability_id,
            gameplay,
        ) else {
            return false;
        };
        let linked_squads = self
            .squads
            .get(squad_id)
            .map_or_else(Vec::new, |squad| squad.associated_wall_towers().to_vec());
        if !self.start_change_mode(player_id, squad_id, request) {
            return false;
        }
        for linked_squad_id in linked_squads {
            let _linked = self.start_change_mode(
                player_id,
                linked_squad_id,
                ChangeModeRequest {
                    target_mode: request.target_mode,
                    ability_id: request.ability_id,
                },
            );
        }
        true
    }

    fn resolve_change_mode_request(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
        requested_mode: Option<SquadMode>,
        requested_ability_id: Option<u8>,
        gameplay: &GameplayCatalog,
    ) -> Option<ChangeModeRequest> {
        let squad = self.squads.get(squad_id)?;
        if squad.base.player_id != player_id || !self.change_mode_context_valid(squad_id) {
            return None;
        }
        let unit = squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .find(|unit| unit.is_operational())?;
        let Some(requested_ability_id) = requested_ability_id else {
            return Some(ChangeModeRequest {
                target_mode: requested_mode?,
                ability_id: None,
            });
        };
        let ability =
            gameplay.resolve_order_ability(&unit.proto_object_name, requested_ability_id)?;
        if !ability
            .ability_type()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("ChangeMode"))
            || self.change_mode_ability_disabled(player_id, unit, gameplay)
        {
            return None;
        }
        let ability_id = ability.database_id();
        let target_mode = requested_mode
            .or_else(|| self.tactic_change_mode(unit, squad.mode, ability_id, gameplay))
            .or_else(|| {
                ability.squad_mode().map(|authored| {
                    if squad.mode == authored {
                        SquadMode::Normal
                    } else {
                        authored
                    }
                })
            })?;
        Some(ChangeModeRequest {
            target_mode,
            ability_id: Some(ability_id),
        })
    }

    fn tactic_change_mode(
        &self,
        unit: &crate::entities::Unit,
        current_mode: SquadMode,
        ability_id: u8,
        gameplay: &GameplayCatalog,
    ) -> Option<SquadMode> {
        let query = AttackQuery {
            relation: TacticRelation::SelfPlayer,
            squad_mode: current_mode,
            ability_id: Some(ability_id),
            target_proto_object_name: Some(&unit.proto_object_name),
            tactic_state: unit.tactic_state(),
            flags: AttackQueryFlags::empty(),
        };
        gameplay
            .select_work_action(&unit.proto_object_name, &query, |action| {
                is_change_mode_action(action) && self.change_mode_action_enabled(unit, action)
            })
            .filter(|action| is_change_mode_action(action))
            .and_then(|action| {
                action
                    .squad_mode
                    .as_deref()
                    .or(action.new_squad_mode.as_deref())
            })
            .and_then(SquadMode::from_authored)
    }

    fn change_mode_ability_disabled(
        &self,
        player_id: PlayerId,
        unit: &crate::entities::Unit,
        gameplay: &GameplayCatalog,
    ) -> bool {
        self.get_player(player_id).is_some_and(|player| {
            player.technologies.ability_disabled(
                unit.logical_proto_object_name(),
                gameplay.change_mode_ability_starts_disabled(&unit.proto_object_name),
            )
        })
    }

    fn change_mode_action_enabled(&self, unit: &crate::entities::Unit, action: &Action) -> bool {
        let authored_enabled = action.start_disabled != Some(true);
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        unit.logical_proto_object_name(),
                        &action.name,
                        authored_enabled,
                    )
                });
        unit.actions.is_enabled(&action.name, !player_enabled)
    }

    fn change_mode_context_valid(&self, squad_id: EntityId) -> bool {
        self.squads.get(squad_id).is_some_and(|squad| {
            squad.is_alive()
                && !self.is_squad_incapacitated(squad_id)
                && !squad.garrison.is_garrisoned()
                && !squad.is_cryo_frozen()
                && !squad.is_raging()
                && !squad.is_jumping()
                && !squad.is_changing_mode()
                && squad.trained_air_birth.is_none()
                && squad.unit_ids.iter().any(|unit_id| {
                    self.units
                        .get(*unit_id)
                        .is_some_and(crate::entities::Unit::is_operational)
                })
        })
    }

    fn start_change_mode(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        request: ChangeModeRequest,
    ) -> bool {
        if !self.squads.get(squad_id).is_some_and(|squad| {
            squad.base.player_id == player_id
                && squad.mode != request.target_mode
                && self.change_mode_context_valid(squad_id)
        }) {
            return false;
        }
        let _capture_cancelled = self.cancel_capture_order(squad_id);
        let _repair_cancelled = self.cancel_repair_other_order(squad_id);
        self.cancel_incoming_power_transport(squad_id);
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        squad.remove_all_orders();
        squad.begin_change_mode(request.target_mode, request.ability_id)
    }

    pub(super) fn update_change_modes(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.update_mode_order_transitions(gameplay);
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| squad.is_changing_mode().then_some(squad_id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            let pending = self
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.change_mode_phase() == SquadChangeModePhase::Pending);
            if pending {
                self.start_change_mode_members(squad_id, gameplay);
                // Retail installs the unit action after visual advancement for
                // this update. Its clip cannot consume this update's elapsed
                // time until the following unit update.
                continue;
            }
            self.advance_change_mode_members(squad_id, dt, gameplay);
        }
    }

    fn start_change_mode_members(&mut self, squad_id: EntityId, gameplay: &GameplayCatalog) {
        let Some((source_mode, target_mode, member_ids)) =
            self.squads.get(squad_id).and_then(|squad| {
                Some((
                    squad.mode,
                    squad.changing_to_mode()?,
                    squad.unit_ids.clone(),
                ))
            })
        else {
            return;
        };
        let mut animations = Vec::new();
        for unit_id in member_ids {
            let Some(prototype) = self
                .units
                .get(unit_id)
                .filter(|unit| unit.is_operational())
                .map(|unit| unit.proto_object_name.clone())
            else {
                continue;
            };
            if source_mode == SquadMode::Lockdown
                && target_mode != SquadMode::Lockdown
                && gameplay.has_lockdown_menu(&prototype)
            {
                self.clear_building_production_queue(unit_id);
            }
            let Some(animation_type) = gameplay
                .squad_mode_animation(&prototype, target_mode)
                .map(str::to_owned)
            else {
                continue;
            };
            let Some(clip) = gameplay.scripted_animation_clip(&prototype, &animation_type) else {
                continue;
            };
            if clip.duration_ms() == 0 {
                continue;
            }
            animations.push(MemberAnimation {
                unit_id,
                animation_type,
                asset_path: clip.asset_path().to_owned(),
                duration_ms: clip.duration_ms(),
            });
        }
        for animation in animations {
            let duration = animation.duration_ms.to_f32().unwrap_or(f32::MAX) / 1_000.0;
            let started = self.units.get_mut(animation.unit_id).is_some_and(|unit| {
                unit.begin_change_mode(target_mode, Some(&animation.animation_type), duration)
            });
            if started {
                let _played = self.play_entity_animation(
                    animation.unit_id,
                    animation.animation_type,
                    Some(animation.asset_path),
                    animation.duration_ms,
                );
            }
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.change_mode.mark_working();
        }
    }

    fn advance_change_mode_members(
        &mut self,
        squad_id: EntityId,
        dt: f32,
        gameplay: &GameplayCatalog,
    ) {
        let member_ids = self
            .squads
            .get(squad_id)
            .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
        let mut completed = Vec::new();
        for unit_id in &member_ids {
            let Some(unit) = self.units.get_mut(*unit_id) else {
                continue;
            };
            if unit.change_mode_phase() == UnitChangeModePhase::Working
                && unit.advance_change_mode(dt)
            {
                completed.push(*unit_id);
            }
        }
        for unit_id in completed {
            let _played = self.play_entity_animation(unit_id, "Idle".to_owned(), None, 0);
        }
        let working = member_ids.iter().any(|unit_id| {
            self.units
                .get(*unit_id)
                .is_some_and(|unit| unit.change_mode_phase() == UnitChangeModePhase::Working)
        });
        if working {
            return;
        }
        let source_mode = self.squads.get(squad_id).map(|squad| squad.mode);
        let completion = self
            .squads
            .get_mut(squad_id)
            .and_then(crate::entities::Squad::finish_change_mode);
        let Some((mode, ability_id)) = completion else {
            return;
        };
        if let Some(source_mode) = source_mode {
            self.apply_lockdown_menu_mode(squad_id, source_mode, mode, gameplay);
        }
        let modifier = ability_id
            .and_then(|ability_id| gameplay.ability(ability_id))
            .and_then(crate::gameplay::AbilityGameplay::mode_movement_modifier);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.apply_mode_movement_modifier(modifier);
        }
    }
}

fn is_change_mode_action(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ChangeMode"))
}

#[cfg(test)]
mod tests;
