//! Retail unit-opportunity completion and squad attack aggregation.

use super::{ConcreteTargetSnapshot, World};
use crate::entities::{AttackActionResult, SquadState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AttackProfile, GameplayCatalog};

#[derive(Debug)]
struct SquadTerminalResolution {
    squad_id: EntityId,
    unit_ids: Vec<EntityId>,
    failed: bool,
    restart: bool,
}

impl World {
    pub(super) fn attack_profile_or_record_depletion<'gameplay>(
        &mut self,
        attacker_id: EntityId,
        target: &ConcreteTargetSnapshot,
        gameplay: &'gameplay GameplayCatalog,
        ability_squad_id: Option<EntityId>,
    ) -> Option<&'gameplay AttackProfile> {
        if let Some(profile) = self.selected_unit_attack_profile(attacker_id, target, gameplay) {
            return Some(profile);
        }
        self.record_depleted_attack_failure(attacker_id, target, gameplay, ability_squad_id);
        None
    }

    fn record_depleted_attack_failure(
        &mut self,
        attacker_id: EntityId,
        target: &ConcreteTargetSnapshot,
        gameplay: &GameplayCatalog,
        ability_squad_id: Option<EntityId>,
    ) {
        let Some(profile) = self.depleted_attack_profile(attacker_id, target, gameplay) else {
            return;
        };
        if let Some(unit) = self.units.get_mut(attacker_id) {
            unit.combat.fail_to_start(profile, target.id);
        }
        self.record_attack_result(
            attacker_id,
            profile,
            ability_squad_id,
            Some(AttackActionResult::Failed),
        );
    }

    pub(super) fn record_attack_result(
        &mut self,
        attacker_id: EntityId,
        profile: &AttackProfile,
        ability_squad_id: Option<EntityId>,
        result: Option<AttackActionResult>,
    ) {
        let Some(result) = result else {
            return;
        };
        if !profile.allows_opportunity_completion() {
            if let Some(unit) = self.units.get_mut(attacker_id) {
                unit.combat.reset();
            }
            return;
        }
        if result == AttackActionResult::Done
            && let Some(squad_id) = ability_squad_id
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.mark_unit_ability_complete(attacker_id);
        }
    }

    pub(super) fn finish_terminal_attack_actions(&mut self, gameplay: &GameplayCatalog) {
        self.finish_terminal_standalone_attacks();
        let resolutions = self
            .squads
            .iter()
            .filter_map(|(squad_id, _)| self.squad_terminal_resolution(squad_id, gameplay))
            .collect::<Vec<_>>();
        for resolution in resolutions {
            for unit_id in &resolution.unit_ids {
                if let Some(unit) = self.units.get_mut(*unit_id) {
                    unit.combat.reset();
                }
            }
            if (resolution.failed || !resolution.restart)
                && let Some(squad) = self.squads.get_mut(resolution.squad_id)
            {
                squad.clear_attack_order();
            }
        }
    }

    fn finish_terminal_standalone_attacks(&mut self) {
        let unit_ids = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (unit.squad_id.is_none()
                    && unit.combat.terminal_result().is_some()
                    && unit.state == crate::entities::UnitState::Attacking)
                    .then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.clear_attack_order();
            }
        }
    }

    fn squad_terminal_resolution(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<SquadTerminalResolution> {
        let squad = self
            .squads
            .get(squad_id)
            .filter(|squad| squad.state == SquadState::Attacking)?;
        let unit_ids = squad
            .unit_ids
            .iter()
            .copied()
            .filter(|unit_id| self.units.get(*unit_id).is_some_and(Entity::is_alive))
            .collect::<Vec<_>>();
        if unit_ids.is_empty() {
            return None;
        }
        let mut failed = false;
        let mut restart = true;
        for unit_id in &unit_ids {
            let unit = self.units.get(*unit_id)?;
            let result = unit.combat.terminal_result()?;
            failed |= result == AttackActionResult::Failed;
            restart &= result == AttackActionResult::Done
                && unit.combat.action_name().is_some_and(|action_name| {
                    gameplay
                        .object(unit.logical_proto_object_name())
                        .and_then(|object| object.attack_profile(action_name))
                        .is_some_and(AttackProfile::dont_loop_attack_animation)
                });
        }
        Some(SquadTerminalResolution {
            squad_id,
            unit_ids,
            failed,
            restart,
        })
    }

    fn depleted_attack_profile<'gameplay>(
        &self,
        attacker_id: EntityId,
        target: &ConcreteTargetSnapshot,
        gameplay: &'gameplay GameplayCatalog,
    ) -> Option<&'gameplay AttackProfile> {
        let unit = self.units.get(attacker_id)?;
        let action =
            self.selected_ranged_action_ignoring_ammunition(unit, target, gameplay, false)?;
        gameplay
            .object(&unit.proto_object_name)?
            .attack_profile(&action.action.name)
            .filter(|profile| profile.ammunition.is_used())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::AttackAmmunition;
    use crate::world::combat::test_catalog::ammunition_ability_catalog;
    use glam::Vec3;

    #[test]
    fn monitored_depletion_done_starts_recovery_while_failure_does_not() {
        let complete_gameplay =
            ammunition_ability_catalog(AttackAmmunition::CompleteWhenDepleted, true);
        let (mut complete, complete_squad, _, complete_target) = attack_world(&[5.0]);
        complete.update_entities_with_gameplay(0.11, &complete_gameplay);
        let squad = complete.get_squad(complete_squad).unwrap();
        assert_eq!(squad.state, SquadState::Idle);
        assert_eq!(squad.attack_target, None);
        assert!(squad.recovery.is_recovering());
        assert!(complete.get_unit(complete_target).unwrap().hitpoints < 100.0);

        let failed_gameplay = ammunition_ability_catalog(AttackAmmunition::FailWhenDepleted, true);
        let (mut failed, failed_squad, _, failed_target) = attack_world(&[5.0]);
        failed.update_entities_with_gameplay(0.11, &failed_gameplay);
        let squad = failed.get_squad(failed_squad).unwrap();
        assert_eq!(squad.state, SquadState::Idle);
        assert_eq!(squad.attack_target, None);
        assert!(!squad.recovery.is_recovering());
        assert!(failed.get_unit(failed_target).unwrap().hitpoints < 100.0);
    }

    #[test]
    fn complete_when_depleted_still_fails_if_the_action_starts_empty() {
        let gameplay = ammunition_ability_catalog(AttackAmmunition::CompleteWhenDepleted, true);
        let (mut world, squad_id, _, target_id) = attack_world(&[0.0]);

        world.update_entities_with_gameplay(0.01, &gameplay);

        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.state, SquadState::Idle);
        assert!(!squad.recovery.is_recovering());
        assert!((world.get_unit(target_id).unwrap().hitpoints - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn unmonitored_looping_opportunity_retries_after_ammunition_returns() {
        let gameplay = ammunition_ability_catalog(AttackAmmunition::FailWhenDepleted, false);
        let (mut world, squad_id, attacker_ids, target_id) = attack_world(&[5.0]);

        world.update_entities_with_gameplay(0.11, &gameplay);
        assert_eq!(
            world.get_squad(squad_id).unwrap().attack_target,
            Some(target_id)
        );
        assert!(!world.get_squad(squad_id).unwrap().recovery.is_recovering());
        let first_hitpoints = world.get_unit(target_id).unwrap().hitpoints;

        world
            .get_unit_mut(attacker_ids[0])
            .unwrap()
            .ammunition
            .set_current(5.0);
        world.update_entities_with_gameplay(0.11, &gameplay);
        assert!(world.get_unit(target_id).unwrap().hitpoints < first_hitpoints);
        assert_eq!(
            world.get_squad(squad_id).unwrap().attack_target,
            Some(target_id)
        );
    }

    #[test]
    fn squad_waits_for_every_monitored_unit_before_resolving_failure() {
        let gameplay = ammunition_ability_catalog(AttackAmmunition::FailWhenDepleted, true);
        let (mut world, squad_id, _, target_id) = attack_world(&[0.0, 5.0]);

        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(
            world.get_squad(squad_id).unwrap().attack_target,
            Some(target_id)
        );

        world.update_entities_with_gameplay(0.06, &gameplay);
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.state, SquadState::Idle);
        assert_eq!(squad.attack_target, None);
        assert!(!squad.recovery.is_recovering());
    }

    fn attack_world(ammunition: &[f32]) -> (World, EntityId, Vec<EntityId>, EntityId) {
        let mut world = World::with_seed(29);
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let mut attacker_ids = Vec::new();
        for &current in ammunition {
            let attacker_id = world.create_unit_at(1, Vec3::ZERO);
            let attacker = world.get_unit_mut(attacker_id).unwrap();
            attacker.proto_object_name = "test_attacker".to_owned();
            attacker.ammunition.set_current(current);
            assert!(world.attach_unit_to_squad(attacker_id, squad_id));
            attacker_ids.push(attacker_id);
        }
        let target_id = world.create_unit_at(2, Vec3::X);
        world.get_unit_mut(target_id).unwrap().proto_object_name = "test_target".to_owned();
        assert!(world.issue_attack_order_with_context(1, squad_id, target_id, 0.0, None, Some(0),));
        (world, squad_id, attacker_ids, target_id)
    }
}
