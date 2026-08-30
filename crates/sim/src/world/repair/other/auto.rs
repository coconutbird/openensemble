//! Retail idle-opportunity discovery for the hard-coded `RepairOther` action.

use super::{RepairTarget, World};
use crate::entities::{RepairOtherPhase, SquadState};
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, RepairOtherActionProfile};
use pipeline::database::hw1::Database;

#[derive(Debug, Clone)]
struct AutoRepairSource {
    squad_id: EntityId,
    player_id: u8,
    team_id: u8,
    leader_id: EntityId,
    profile: RepairOtherActionProfile,
}

impl World {
    pub(in crate::world) fn update_auto_repairs(
        &mut self,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let sources = self
            .squads
            .iter()
            .filter_map(|(squad_id, _)| self.auto_repair_source(squad_id, gameplay))
            .collect::<Vec<_>>();
        for source in sources {
            let Some(target) = self.best_auto_repair_target(&source, database, gameplay) else {
                continue;
            };
            let action_name = source.profile.action_name().to_owned();
            let _issued = self.connect_repair_other_order(
                source.player_id,
                source.squad_id,
                &target,
                &action_name,
                None,
            );
        }
    }

    fn auto_repair_source(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<AutoRepairSource> {
        let squad = self.squads.get(squad_id)?;
        if squad.state != SquadState::Idle
            || squad.move_target.is_some()
            || !squad.has_idle_action()
            || matches!(
                squad.repair_other.phase(),
                RepairOtherPhase::Moving | RepairOtherPhase::Working
            )
        {
            return None;
        }
        let leader_id = self.repair_other_source(squad.base.player_id, squad_id)?;
        let leader = self.units.get(leader_id)?;
        let profile = gameplay
            .repair_other_actions(&leader.proto_object_name)
            .iter()
            .find(|profile| profile.action_name().eq_ignore_ascii_case("RepairOther"))?
            .clone();
        let auto = profile.auto_repair()?;
        if squad.idle_duration() < auto.idle_time_ms()
            || !self.repair_other_profile_enabled(leader, &profile)
        {
            return None;
        }
        let team_id = self.get_player(squad.base.player_id)?.team_id;
        Some(AutoRepairSource {
            squad_id,
            player_id: squad.base.player_id,
            team_id,
            leader_id,
            profile,
        })
    }

    fn best_auto_repair_target(
        &self,
        source: &AutoRepairSource,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> Option<RepairTarget> {
        let auto = source.profile.auto_repair()?;
        self.squads
            .iter()
            .filter_map(|(target_id, _)| {
                let target = self.repair_target(target_id)?;
                if target.squad_id == source.squad_id
                    || self.get_player(target.player_id)?.team_id != source.team_id
                    || !self.is_entity_visible_to_team(source.team_id, target.squad_id)
                    || self.squad_hitpoint_fraction(target.squad_id, database) > auto.threshold()
                {
                    return None;
                }
                let distance = self.auto_repair_distance(source.squad_id, &target);
                if distance > auto.search_distance()
                    || !self.auto_repair_target_matches(source, &target, gameplay)
                {
                    return None;
                }
                Some((ordered_distance(distance), target.squad_id, target))
            })
            .min_by_key(|candidate| (candidate.0, candidate.1))
            .map(|candidate| candidate.2)
    }

    fn auto_repair_target_matches(
        &self,
        source: &AutoRepairSource,
        target: &RepairTarget,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(unit) = self.units.get(source.leader_id) else {
            return false;
        };
        let query = self.repair_other_query(source.squad_id, unit, target, None);
        gameplay
            .select_repair_other_action(&unit.proto_object_name, &query, |action| {
                self.repair_other_action_enabled(unit, action)
            })
            .is_some()
    }

    fn auto_repair_distance(&self, source_squad_id: EntityId, target: &RepairTarget) -> f32 {
        let Some(source) = self.squads.get(source_squad_id) else {
            return f32::MAX;
        };
        let delta = target.position - source.base.position;
        (delta.x.hypot(delta.z)
            - self.squad_obstruction_radius(source_squad_id)
            - self.squad_obstruction_radius(target.squad_id))
        .max(0.0)
    }
}

fn ordered_distance(distance: f32) -> u32 {
    if distance.is_finite() && distance >= 0.0 {
        distance.to_bits()
    } else {
        u32::MAX
    }
}
