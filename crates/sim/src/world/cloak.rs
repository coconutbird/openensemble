//! Source-backed persistent squad cloak lifecycle and presentation state.

use super::World;
use crate::entities::squads::CloakModifiers;
use crate::entities::{RecoveryType, UnitDataScalar};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{CloakProfile, GameplayCatalog};
use crate::player::PlayerId;
use pipeline::database::hw1::Database;

#[derive(Debug, Clone)]
struct CloakContext {
    player_id: PlayerId,
    leader_id: EntityId,
    proto_object_name: String,
    member_ids: Vec<EntityId>,
    moving: bool,
    profile: CloakProfile,
}

#[derive(Debug, Clone)]
struct CloakCleanup {
    player_id: PlayerId,
    modifiers: CloakModifiers,
    unit_ids: Vec<EntityId>,
    effect_ids: Vec<EntityId>,
}

impl World {
    /// Request retail order 25 for one owned squad.
    pub fn issue_cloak_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        requested_ability_id: Option<u8>,
    ) -> bool {
        if !self.squads.get(squad_id).is_some_and(|squad| {
            squad.base.player_id == player_id
                && squad.is_alive()
                && !self.is_squad_incapacitated(squad_id)
                && !squad.garrison.is_garrisoned()
                && !squad.recovery.is_recovering()
                && !squad.is_cloaked()
        }) {
            return false;
        }
        let accepted = self
            .squads
            .get_mut(squad_id)
            .is_some_and(|squad| squad.cloak.request(requested_ability_id));
        if accepted {
            self.cancel_incoming_power_transport(squad_id);
        }
        accepted
    }

    /// Notify a cloaked squad that an enemy detector currently sees it.
    pub fn detect_cloaked_squad(&mut self, squad_id: EntityId) -> bool {
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        if !squad.is_cloaked() {
            return false;
        }
        squad.cloak.detect();
        true
    }

    pub(super) fn update_cloaks(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let squad_ids = self.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_cloak(squad_id, dt, database, gameplay);
        }
    }

    fn update_cloak(
        &mut self,
        squad_id: EntityId,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(context) = self.cloak_context(squad_id, gameplay) else {
            if self
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.cloak.is_initialized())
            {
                self.disconnect_cloak(squad_id);
            }
            return;
        };
        self.initialize_cloak(squad_id, &context.profile);
        if !self.cloak_action_enabled(&context) {
            return;
        }
        let cloaked = self
            .squads
            .get(squad_id)
            .is_some_and(crate::entities::Squad::is_cloaked);
        if context.profile.permanent() && !cloaked {
            self.activate_cloak(squad_id, &context, database, gameplay);
            return;
        }
        if cloaked {
            self.advance_active_cloak(squad_id, &context, dt, gameplay);
            return;
        }
        let ready = self
            .squads
            .get_mut(squad_id)
            .is_some_and(|squad| squad.cloak.activation_ready(dt));
        if ready {
            self.activate_cloak(squad_id, &context, database, gameplay);
        }
    }

    fn cloak_context(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<CloakContext> {
        let squad = self.squads.get(squad_id).filter(|squad| squad.is_alive())?;
        let leader_id = *squad.unit_ids.first()?;
        let leader = self.units.get(leader_id).filter(|unit| unit.is_alive())?;
        Some(CloakContext {
            player_id: squad.base.player_id,
            leader_id,
            proto_object_name: leader.proto_object_name.clone(),
            member_ids: squad.unit_ids.clone(),
            moving: squad.move_target.is_some(),
            profile: gameplay.cloak(&leader.proto_object_name)?.clone(),
        })
    }

    fn initialize_cloak(&mut self, squad_id: EntityId, profile: &CloakProfile) {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.cloak.initialize(
                profile.permanent(),
                profile.cloaking_delay(),
                profile.recloak_delay(),
            );
        }
    }

    fn cloak_action_enabled(&self, context: &CloakContext) -> bool {
        let Some(leader) = self.units.get(context.leader_id) else {
            return false;
        };
        let authored_enabled = !context.profile.starts_disabled();
        let player_enabled =
            self.get_player(context.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &context.proto_object_name,
                        context.profile.action_name(),
                        authored_enabled,
                    )
                });
        leader
            .actions
            .is_enabled(context.profile.action_name(), !player_enabled)
    }

    fn advance_active_cloak(
        &mut self,
        squad_id: EntityId,
        context: &CloakContext,
        dt: f32,
        gameplay: &GameplayCatalog,
    ) {
        let should_uncloak = self.squads.get_mut(squad_id).is_some_and(|squad| {
            squad.cloak.advance_detection(dt);
            !context.profile.permanent()
                && ((!context.profile.move_while_cloaked() && context.moving)
                    || squad.cloak.duration_expired(dt))
        });
        if should_uncloak {
            self.finish_cloak(squad_id, Some(gameplay), true, false);
        }
    }

    fn activate_cloak(
        &mut self,
        squad_id: EntityId,
        context: &CloakContext,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(command_id) = gameplay.command_ability_id() else {
            return;
        };
        let Some(ability) = gameplay.resolve_order_ability(&context.proto_object_name, command_id)
        else {
            return;
        };
        let modifiers = CloakModifiers {
            ability_id: Some(ability.database_id()),
            damage_taken: ability.damage_taken_modifier(),
            dodge: ability.dodge_modifier(),
        };
        let modified = self.apply_cloak_to_members(&context.member_ids, modifiers);
        let effects = self.create_cloak_effects(context, database);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad
                .cloak
                .activate(ability.duration(), modifiers, modified, effects);
        }
    }

    fn apply_cloak_to_members(
        &mut self,
        member_ids: &[EntityId],
        modifiers: CloakModifiers,
    ) -> Vec<EntityId> {
        member_ids
            .iter()
            .copied()
            .filter(|unit_id| {
                let Some(unit) = self.units.get_mut(*unit_id) else {
                    return false;
                };
                adjust_cloak_modifiers(unit, modifiers, false);
                unit.set_cloak_mesh_sections(true);
                true
            })
            .collect()
    }

    fn create_cloak_effects(
        &mut self,
        context: &CloakContext,
        database: &Database,
    ) -> Vec<(EntityId, EntityId)> {
        let Some(effect_name) = context.profile.effect_proto_object() else {
            return Vec::new();
        };
        let Some((prototype_id, prototype_name)) =
            self.cloak_effect_prototype(context.player_id, effect_name, database)
        else {
            return Vec::new();
        };
        context
            .member_ids
            .iter()
            .filter_map(|unit_id| {
                self.add_visual_attachment_to_unit(*unit_id, prototype_id, &prototype_name)
                    .map(|effect_id| (*unit_id, effect_id))
            })
            .collect()
    }

    fn cloak_effect_prototype(
        &self,
        player_id: PlayerId,
        logical_name: &str,
        database: &Database,
    ) -> Option<(i32, String)> {
        let effective_name = self.get_player(player_id).map_or(logical_name, |player| {
            player.technologies.resolved_unit_prototype(logical_name)
        });
        database
            .objects
            .iter()
            .enumerate()
            .find(|(_, prototype)| prototype.name.eq_ignore_ascii_case(effective_name))
            .or_else(|| {
                database
                    .objects
                    .iter()
                    .enumerate()
                    .find(|(_, prototype)| prototype.name.eq_ignore_ascii_case(logical_name))
            })
            .map(|(index, prototype)| {
                (
                    prototype
                        .dbid
                        .unwrap_or_else(|| i32::try_from(index).unwrap_or(-1)),
                    prototype.name.clone(),
                )
            })
    }

    fn finish_cloak(
        &mut self,
        squad_id: EntityId,
        gameplay: Option<&GameplayCatalog>,
        start_recovery: bool,
        disconnect: bool,
    ) {
        let Some(cleanup) = self.cloak_cleanup(squad_id) else {
            return;
        };
        for unit_id in cleanup.unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                adjust_cloak_modifiers(unit, cleanup.modifiers, true);
                unit.set_cloak_mesh_sections(false);
            }
        }
        for effect_id in cleanup.effect_ids {
            let _removed = self.remove_object(effect_id);
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            if disconnect {
                squad.cloak.disconnect();
            } else {
                squad.cloak.finish();
            }
        }
        if start_recovery {
            self.start_cloak_recovery(squad_id, cleanup.player_id, cleanup.modifiers, gameplay);
        }
    }

    fn cloak_cleanup(&self, squad_id: EntityId) -> Option<CloakCleanup> {
        let squad = self.squads.get(squad_id)?;
        Some(CloakCleanup {
            player_id: squad.base.player_id,
            modifiers: squad.cloak.modifiers(),
            unit_ids: squad.cloak.modified_unit_ids().to_vec(),
            effect_ids: squad
                .cloak
                .effect_attachments()
                .iter()
                .map(|(_, effect_id)| *effect_id)
                .collect(),
        })
    }

    fn start_cloak_recovery(
        &mut self,
        squad_id: EntityId,
        player_id: PlayerId,
        modifiers: CloakModifiers,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let Some(ability) = modifiers.ability_id.and_then(|id| gameplay?.ability(id)) else {
            return;
        };
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

    fn disconnect_cloak(&mut self, squad_id: EntityId) {
        self.finish_cloak(squad_id, None, false, true);
    }

    pub(super) fn notify_unit_cloak_damaged(&mut self, unit_id: EntityId) {
        let Some(squad_id) = self.units.get(unit_id).and_then(|unit| unit.squad_id) else {
            return;
        };
        let _detected = self.detect_cloaked_squad(squad_id);
    }

    pub(super) fn prepare_remove_squad_cloak(&mut self, squad_id: EntityId) {
        self.disconnect_cloak(squad_id);
    }

    pub(super) fn prepare_remove_unit_cloak(&mut self, unit_id: EntityId) {
        let Some(squad_id) = self.units.get(unit_id).and_then(|unit| unit.squad_id) else {
            return;
        };
        self.disconnect_cloak(squad_id);
    }

    pub(super) fn prepare_squad_membership_change_cloak(&mut self, squad_id: EntityId) {
        if self
            .squads
            .get(squad_id)
            .is_some_and(|squad| squad.cloak.is_initialized() || squad.wants_to_cloak())
        {
            self.disconnect_cloak(squad_id);
        }
    }
}

fn adjust_cloak_modifiers(
    unit: &mut crate::entities::Unit,
    modifiers: CloakModifiers,
    reset: bool,
) {
    let damage_taken = reset_factor(modifiers.damage_taken, reset);
    if let Some(factor) = damage_taken {
        unit.adjust_data_scalar(UnitDataScalar::DamageTaken, factor);
    }
    if let Some(factor) = reset_factor(modifiers.dodge, reset) {
        unit.dodge_scalar *= factor;
    }
}

fn reset_factor(value: f32, reset: bool) -> Option<f32> {
    if !value.is_finite() || value == 0.0 {
        return None;
    }
    Some(if reset { value.recip() } else { value })
}

#[cfg(test)]
mod tests;
