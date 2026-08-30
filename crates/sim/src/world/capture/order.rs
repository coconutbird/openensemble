//! Capture-order validation, target-rule selection, and payment connection.

use super::costs::target_prototype;
use super::{CaptureTarget, World, stop_capture_unit};
use crate::entities::Unit;
use crate::entity_id::EntityId;
use crate::gameplay::{AttackQuery, AttackQueryFlags, GameplayCatalog};
use crate::player::{PlayerId, Resources};
use pipeline::database::hw1::Database;

#[derive(Debug, Clone)]
struct CaptureSelection {
    unit_id: EntityId,
    action_name: String,
}

impl World {
    /// Issue a squad capture order using scenario-layered database and tactic data.
    pub fn issue_capture_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        requested_target_id: EntityId,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(target_id) = self.capture_target_unit_id(requested_target_id) else {
            return false;
        };
        let Some(unit_ids) = self.capture_order_source(player_id, squad_id) else {
            return false;
        };
        let Some(target) = self.capture_target(target_id) else {
            return false;
        };
        if target.player_id == player_id
            || !self.capture_target_accepts(&target, player_id, squad_id)
            || unit_ids.binary_search(&target_id).is_ok()
        {
            return false;
        }
        let selections = self.capture_selections(player_id, squad_id, &unit_ids, &target, gameplay);
        if selections.is_empty() {
            return false;
        }
        let Some(prototype) = target_prototype(database, &target.proto_object_name) else {
            return false;
        };
        let Some(cost) = self.capture_cost(database, player_id, prototype) else {
            return false;
        };
        let _cancelled = self.cancel_capture_order(squad_id);
        let _repair_cancelled = self.cancel_repair_other_order(squad_id);
        if !self.connect_capture_payment(target_id, player_id, squad_id, cost) {
            return false;
        }
        let Some(squad) = self.squads.get_mut(squad_id) else {
            self.disconnect_capture_link(target_id, player_id, squad_id, false);
            return false;
        };
        squad.begin_capture(player_id, target_id);
        for unit_id in unit_ids {
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            stop_capture_unit(unit);
            if let Some(selection) = selections
                .iter()
                .find(|selection| selection.unit_id == unit_id)
            {
                unit.capture.action.start(target_id, &selection.action_name);
            } else {
                unit.cancel_capture_action();
            }
        }
        self.cancel_incoming_power_transport(squad_id);
        true
    }

    fn capture_selections(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
        unit_ids: &[EntityId],
        target: &CaptureTarget,
        gameplay: &GameplayCatalog,
    ) -> Vec<CaptureSelection> {
        unit_ids
            .iter()
            .filter_map(|&unit_id| {
                let unit = self
                    .units
                    .get(unit_id)
                    .filter(|unit| unit.is_operational())?;
                let query = self.capture_query(player_id, squad_id, unit, target);
                let profile =
                    gameplay.select_capture_action(&unit.proto_object_name, &query, |action| {
                        self.capture_action_enabled(unit, action)
                    })?;
                Some(CaptureSelection {
                    unit_id,
                    action_name: profile.action_name().to_owned(),
                })
            })
            .collect()
    }

    fn capture_query<'target>(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
        unit: &Unit,
        target: &'target CaptureTarget,
    ) -> AttackQuery<'target> {
        let mut flags = AttackQueryFlags::empty();
        flags.insert(AttackQueryFlags::TARGET_CAPTURABLE);
        if target.player_id == crate::player::GAIA_PLAYER {
            flags.insert(AttackQueryFlags::TARGET_GAIA);
        }
        if target.invulnerable {
            flags.insert(AttackQueryFlags::TARGET_INVULNERABLE);
        }
        AttackQuery {
            relation: self.capture_relation(player_id, target.player_id),
            squad_mode: self
                .squads
                .get(squad_id)
                .map_or(crate::entities::SquadMode::Normal, |squad| squad.mode),
            ability_id: None,
            target_proto_object_name: Some(&target.proto_object_name),
            tactic_state: unit.tactic_state(),
            flags,
        }
    }

    pub(super) fn capture_action_enabled(
        &self,
        unit: &Unit,
        action: &pipeline::database::hw1::tactics::Action,
    ) -> bool {
        let authored_enabled = action.start_disabled != Some(true);
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        &action.name,
                        authored_enabled,
                    )
                });
        unit.actions.is_enabled(&action.name, !player_enabled)
    }

    fn connect_capture_payment(
        &mut self,
        target_id: EntityId,
        player_id: PlayerId,
        squad_id: EntityId,
        cost: Resources,
    ) -> bool {
        let already_paid = self
            .units
            .get(target_id)
            .is_some_and(|target| target.capture.target.has_player_link(player_id));
        if !already_paid {
            let Some(player) = self.get_player_mut(player_id) else {
                return false;
            };
            if !player.resources.can_afford(&cost) {
                return false;
            }
            player.resources.pay(&cost);
        }
        let Some(target) = self.units.get_mut(target_id) else {
            if !already_paid && let Some(player) = self.get_player_mut(player_id) {
                player.resources.refund(&cost);
            }
            return false;
        };
        target
            .capture
            .target
            .connect_squad(player_id, squad_id, cost);
        true
    }
}
