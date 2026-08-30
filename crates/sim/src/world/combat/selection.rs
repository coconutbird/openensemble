//! Tactic action selection shared by entity and position attacks.

use super::{ConcreteTargetSnapshot, World, ammunition};
use crate::entities::{SquadMode, Unit};
use crate::gameplay::{
    AttackQuery, AttackQueryFlags, GameplayCatalog, RangedAction, TacticRelation,
};
use crate::player::{PlayerId, TeamRelation};

impl World {
    pub(super) fn selected_ranged_action<'gameplay>(
        &self,
        unit: &Unit,
        target: &ConcreteTargetSnapshot,
        gameplay: &'gameplay GameplayCatalog,
        automatic: bool,
    ) -> Option<RangedAction<'gameplay>> {
        let (squad_mode, requested_ability_id) = unit
            .squad_id
            .and_then(|id| self.squads.get(id))
            .map_or((SquadMode::Normal, unit.attack_ability_id), |squad| {
                (
                    squad.mode,
                    (!squad.unit_completed_ability(unit.base.id))
                        .then_some(squad.attack_ability_id)
                        .flatten(),
                )
            });
        let ability_id = requested_ability_id.filter(|requested| {
            gameplay
                .resolve_order_ability(&unit.proto_object_name, *requested)
                .is_some()
        });
        let position_target = target.id.is_invalid();
        let mut flags = AttackQueryFlags::empty();
        if automatic {
            flags.insert(AttackQueryFlags::AUTO_TARGET);
        }
        if !position_target && target.player_id == 0 {
            flags.insert(AttackQueryFlags::TARGET_GAIA);
        }
        if target.damaged {
            flags.insert(AttackQueryFlags::TARGET_DAMAGED);
        }
        if target.unbuilt {
            flags.insert(AttackQueryFlags::TARGET_UNBUILT);
        }
        if target.in_cover {
            flags.insert(AttackQueryFlags::TARGET_IN_COVER);
        }
        let query = AttackQuery {
            relation: if position_target {
                TacticRelation::Enemy
            } else {
                self.tactic_relation(unit.base.player_id, target.player_id)
            },
            squad_mode,
            ability_id,
            target_proto_object_name: (!position_target)
                .then_some(target.proto_object_name.as_str()),
            tactic_state: unit.tactic_state(),
            flags,
        };
        gameplay.select_ranged_action(&unit.proto_object_name, &query, |action| {
            if position_target && action.target_air == Some(true) {
                return false;
            }
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
            let profile = gameplay
                .object(&unit.proto_object_name)
                .and_then(|object| object.attack_profile(&action.name));
            let technologies = self
                .get_player(unit.base.player_id)
                .map(|player| &player.technologies);
            unit.actions.is_enabled(&action.name, !player_enabled)
                && ammunition::can_select(unit, profile, technologies)
        })
    }

    fn tactic_relation(&self, source: PlayerId, target: PlayerId) -> TacticRelation {
        if source == target {
            return TacticRelation::SelfPlayer;
        }
        match self.player_relation(source, target) {
            Some(TeamRelation::Ally) => TacticRelation::Ally,
            Some(TeamRelation::Enemy) => TacticRelation::Enemy,
            Some(TeamRelation::Neutral) | None => TacticRelation::Neutral,
        }
    }
}
