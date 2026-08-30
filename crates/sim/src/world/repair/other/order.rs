//! Repair-order validation and tactic target-rule selection.

use super::{RepairTarget, World};
use crate::entities::{SquadMode, Unit};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AttackQuery, AttackQueryFlags, GameplayCatalog};
use crate::player::PlayerId;
use pipeline::database::hw1::Database;

impl World {
    /// Issue a squad `RepairOther` order through scenario-layered gameplay.
    pub fn issue_repair_other_order(
        &mut self,
        player_id: PlayerId,
        source_squad_id: EntityId,
        requested_target_id: EntityId,
        ability_id: Option<u8>,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(target_squad_id) = self.repair_other_target_squad_id(requested_target_id) else {
            return false;
        };
        if source_squad_id == target_squad_id {
            return false;
        }
        let Some(source_id) = self.repair_other_source(player_id, source_squad_id) else {
            return false;
        };
        let Some(target) = self.repair_target(target_squad_id) else {
            return false;
        };
        if !self.players_are_allied(player_id, target.player_id)
            || self.squad_hitpoint_fraction(target_squad_id, database) >= 1.0
        {
            return false;
        }
        let Some(source) = self.units.get(source_id) else {
            return false;
        };
        let query = self.repair_other_query(source_squad_id, source, &target, ability_id);
        let Some(profile) =
            gameplay.select_repair_other_action(&source.proto_object_name, &query, |action| {
                self.repair_other_action_enabled(source, action)
            })
        else {
            return false;
        };
        let action_name = profile.action_name().to_owned();
        self.connect_repair_other_order(
            player_id,
            source_squad_id,
            &target,
            &action_name,
            ability_id,
        )
    }

    pub(super) fn repair_other_source(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
    ) -> Option<EntityId> {
        let squad = self.squads.get(squad_id)?;
        if squad.base.player_id != player_id
            || !squad.is_alive()
            || self.is_squad_incapacitated(squad_id)
            || squad.garrison.is_garrisoned()
            || squad.is_cryo_frozen()
            || squad.is_raging()
            || squad.trained_air_birth.is_some()
        {
            return None;
        }
        squad.unit_ids.iter().find_map(|unit_id| {
            self.units
                .get(*unit_id)
                .filter(|unit| unit.is_operational())
                .map(|_| *unit_id)
        })
    }

    pub(super) fn repair_other_query<'target>(
        &self,
        source_squad_id: EntityId,
        source: &Unit,
        target: &'target RepairTarget,
        ability_id: Option<u8>,
    ) -> AttackQuery<'target> {
        let mut flags = AttackQueryFlags::empty();
        flags.insert(AttackQueryFlags::TARGET_DAMAGED);
        AttackQuery {
            relation: self.repair_relation(source.base.player_id, target.player_id),
            squad_mode: self
                .squads
                .get(source_squad_id)
                .map_or(SquadMode::Normal, |squad| squad.mode),
            ability_id,
            target_proto_object_name: Some(&target.proto_object_name),
            tactic_state: source.tactic_state(),
            flags,
        }
    }

    pub(super) fn repair_other_action_enabled(
        &self,
        source: &Unit,
        action: &pipeline::database::hw1::tactics::Action,
    ) -> bool {
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
    }
}
