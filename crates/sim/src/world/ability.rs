//! Squad command-ability execution and recovery lifecycle.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AbilityRecoveryStart, GameplayCatalog};

impl World {
    pub(super) fn active_ability_squad(
        &self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<EntityId> {
        let unit = self.units.get(unit_id)?;
        let squad_id = unit.squad_id?;
        let squad = self.squads.get(squad_id)?;
        let requested = squad.attack_ability_id?;
        if squad.unit_completed_ability(unit_id) {
            return None;
        }
        gameplay.resolve_order_ability(&unit.proto_object_name, requested)?;
        Some(squad_id)
    }

    pub(super) fn finish_completed_ability_attacks(&mut self, gameplay: &GameplayCatalog) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| squad.attack_ability_id.map(|_| id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            let Some((player_id, requested, participants)) =
                self.squads.get(squad_id).map(|squad| {
                    let participants = squad
                        .unit_ids
                        .iter()
                        .copied()
                        .filter(|unit_id| self.units.get(*unit_id).is_some_and(Entity::is_alive))
                        .collect::<Vec<_>>();
                    (squad.base.player_id, squad.attack_ability_id, participants)
                })
            else {
                continue;
            };
            let Some(requested) = requested else {
                continue;
            };
            if !self
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.ability_complete_for(&participants))
            {
                continue;
            }
            let Some(source_proto) = participants
                .iter()
                .find_map(|unit_id| self.units.get(*unit_id))
                .map(|unit| unit.proto_object_name.as_str())
            else {
                continue;
            };
            let Some(ability) = gameplay.resolve_order_ability(source_proto, requested) else {
                continue;
            };
            let recovery_time =
                self.get_player(player_id)
                    .map_or(ability.recovery_time(), |player| {
                        player
                            .technologies
                            .ability_recovery_time(ability.name(), ability.recovery_time())
                    });
            let recovery_type = (ability.recovery_start() == Some(AbilityRecoveryStart::Attack))
                .then(|| ability.recovery_type())
                .flatten();
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.finish_ability_execution(
                    recovery_type,
                    recovery_time,
                    Some(ability.database_id()),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, CommandType, EntityType};
    use crate::command_queue::{CommandEntry, QueuedCommand};
    use crate::commands::WorkCommand;
    use crate::entities::RecoveryType;
    use crate::executor::CommandExecutor;
    use crate::gameplay::{AttackAccuracyProfile, AttackAnimation, AttackProfile};
    use crate::order::OrderType;
    use glam::Vec3;
    use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule, Weapon};
    use pipeline::database::hw1::{Ability, Database, ProtoObject};

    #[test]
    fn completed_ability_volley_starts_recovery_and_falls_back_to_rifle() {
        let (database, gameplay) = ability_fixture();
        let (mut world, squad_id, _, first_target, second_target) = ability_world();
        assert!(world.issue_attack_order_with_context(
            1,
            squad_id,
            first_target,
            0.0,
            None,
            Some(0),
        ));

        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(world.get_unit(first_target).unwrap().hitpoints < 100.0);
        assert!(!world.get_squad(squad_id).unwrap().recovery.is_recovering());

        world.update_entities_with_gameplay(0.05, &gameplay);
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.attack_ability_id, None);
        assert_eq!(squad.recovery.recovery_type(), Some(RecoveryType::Ability));
        assert_eq!(squad.recovery.ability_id(), Some(1));
        assert!((squad.recovery.remaining() - 1.95).abs() < 0.000_1);

        let blocked = ability_command(squad_id, second_target, 0);
        CommandExecutor::with_database(&database).execute(&mut world, &blocked);
        assert_eq!(
            world.get_squad(squad_id).unwrap().attack_target,
            Some(first_target)
        );

        let normal_attack = ability_command(squad_id, second_target, -1);
        CommandExecutor::with_database(&database).execute(&mut world, &normal_attack);
        assert_eq!(
            world.get_squad(squad_id).unwrap().attack_target,
            Some(second_target)
        );
        world.update_entities_with_gameplay(0.05, &gameplay);
        let attacker = world
            .get_squad(squad_id)
            .unwrap()
            .unit_ids
            .first()
            .and_then(|unit_id| world.get_unit(*unit_id))
            .unwrap();
        assert_eq!(attacker.combat.action_name(), Some("RifleAttack"));

        world.update_entities_with_gameplay(1.90, &gameplay);
        assert!(!world.get_squad(squad_id).unwrap().recovery.is_recovering());
    }

    fn ability_world() -> (World, EntityId, EntityId, EntityId, EntityId) {
        let mut world = World::with_seed(9);
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let attacker_id = world.create_unit_at(1, Vec3::ZERO);
        world.get_unit_mut(attacker_id).unwrap().proto_object_name = "attacker".to_owned();
        assert!(world.attach_unit_to_squad(attacker_id, squad_id));
        let first_target = world.create_unit_at(2, Vec3::X);
        let second_target = world.create_unit_at(2, Vec3::new(2.0, 0.0, 0.0));
        for target_id in [first_target, second_target] {
            world.get_unit_mut(target_id).unwrap().proto_object_name = "target".to_owned();
        }
        (world, squad_id, attacker_id, first_target, second_target)
    }

    fn ability_fixture() -> (Database, GameplayCatalog) {
        let mut database = Database::new();
        database.abilities.extend([
            Ability {
                name: "Command".to_owned(),
                ..Ability::default()
            },
            Ability {
                name: "TestRocket".to_owned(),
                recover_start: Some("Attack".to_owned()),
                recover_type: Some("Ability".to_owned()),
                recover_time: Some(2.0),
                ..Ability::default()
            },
        ]);
        database.objects.extend([
            ProtoObject {
                name: "attacker".to_owned(),
                tactics: Some("attacker.tactics".to_owned()),
                ability_command: Some("TestRocket".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "target".to_owned(),
                object_types: vec!["NonFlying".to_owned()],
                ..ProtoObject::default()
            },
        ]);
        let tactics = TacticData {
            weapons: vec![weapon("Rifle", 10.0), weapon("Rocket", 40.0)],
            actions: vec![
                ranged_action("RifleAttack", "Rifle"),
                ranged_action("RocketAttack", "Rocket"),
            ],
            tactic: Some(TacticRules {
                target_rules: vec![
                    TargetRule {
                        relation: Some("Enemy".to_owned()),
                        squad_mode: Some("Normal".to_owned()),
                        action: Some("RifleAttack".to_owned()),
                        ..TargetRule::default()
                    },
                    TargetRule {
                        relation: Some("Enemy".to_owned()),
                        squad_mode: Some("Normal".to_owned()),
                        target_types: vec!["NonFlying".to_owned()],
                        ability: Some("Command".to_owned()),
                        action: Some("RocketAttack".to_owned()),
                        ..TargetRule::default()
                    },
                ],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        let gameplay = GameplayCatalog::from_test_profiles(
            &database,
            [("attacker".to_owned(), tactics)],
            [
                ("attacker".to_owned(), attack_profile("RifleAttack", 10.0)),
                ("attacker".to_owned(), attack_profile("RocketAttack", 40.0)),
            ],
        );
        (database, gameplay)
    }

    fn ability_command(squad_id: EntityId, target_id: EntityId, ability_id: i32) -> CommandEntry {
        CommandEntry {
            command: QueuedCommand::Work(WorkCommand {
                base: Command {
                    id: OrderType::Attack as i32,
                    player_id: 1,
                    sender_type: EntityType::Player,
                    recipient_type: EntityType::Squad,
                    recipients: vec![squad_id],
                    command_type: CommandType::Work,
                    ..Command::default()
                },
                unit_id: target_id,
                ability_id,
                ..WorkCommand::default()
            }),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        }
    }

    fn weapon(name: &str, max_range: f32) -> Weapon {
        Weapon {
            name: name.to_owned(),
            max_range: Some(max_range),
            ..Weapon::default()
        }
    }

    fn ranged_action(name: &str, weapon: &str) -> Action {
        Action {
            name: name.to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some(weapon.to_owned()),
            ..Action::default()
        }
    }

    fn attack_profile(action_name: &str, max_range: f32) -> AttackProfile {
        AttackProfile {
            action_name: action_name.to_owned(),
            animation_type: "Attack".to_owned(),
            weapon_name: action_name.to_owned(),
            weapon_type: None,
            projectile: None,
            impact_effect: None,
            area_damage: None,
            pull: None,
            hardpoint: None,
            orientation: crate::gameplay::AttackOrientationProfile::default(),
            charged_animation: None,
            friendly_fire: false,
            targets_foot_of_unit: false,
            projectile_reactions: crate::gameplay::ProjectileReactionFlags::default(),
            max_range,
            max_velocity_lead: 0.0,
            accuracy: AttackAccuracyProfile::default(),
            damage_per_attack: 5.0,
            ammunition: crate::gameplay::AttackAmmunition::None,
            animations: vec![AttackAnimation {
                asset_path: "attack.uax".to_owned(),
                weight: 1,
                duration: 0.1,
                attack_positions: vec![0.0],
                events: Vec::new(),
                hardpoint_track: None,
            }],
            pre_attack_cooldown: [0.0, 0.0],
            post_attack_cooldown: [0.0, 0.0],
            reload_duration: 0.0,
            visual_ammo: 0,
            uses_height_bonus_damage: false,
        }
    }
}
