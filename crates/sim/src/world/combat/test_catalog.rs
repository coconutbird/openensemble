use crate::gameplay::{
    AreaDamageProfile, AttackAccuracyProfile, AttackAnimation, AttackProfile, GameplayCatalog,
};
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule, Weapon};
use pipeline::database::hw1::{Ability, Database, ProtoObject};

pub(super) fn ability_catalog() -> GameplayCatalog {
    let mut database = Database::new();
    database.abilities.push(Ability {
        name: "Command".to_owned(),
        ..Ability::default()
    });
    database.abilities.push(Ability {
        name: "TestGrenade".to_owned(),
        recover_start: Some("Attack".to_owned()),
        recover_type: Some("Ability".to_owned()),
        recover_time: Some(20.0),
        ..Ability::default()
    });
    database.objects.extend([
        ProtoObject {
            name: "test_attacker".to_owned(),
            tactics: Some("test_attacker.tactics".to_owned()),
            ability_command: Some("TestGrenade".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "test_target".to_owned(),
            object_types: vec!["NonFlying".to_owned()],
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        weapons: vec![
            Weapon {
                name: "Rifle".to_owned(),
                max_range: Some(10.0),
                ..Weapon::default()
            },
            Weapon {
                name: "Grenade".to_owned(),
                max_range: Some(40.0),
                ..Weapon::default()
            },
        ],
        actions: vec![
            ranged_action("RifleAttack", "Rifle"),
            ranged_action("GrenadeAttack", "Grenade"),
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
                    action: Some("GrenadeAttack".to_owned()),
                    ability: Some("Command".to_owned()),
                    ..TargetRule::default()
                },
            ],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let mut grenade_profile = instant_profile("GrenadeAttack", 40.0);
    grenade_profile.area_damage = Some(AreaDamageProfile {
        radius: 4.0,
        primary_target_factor: 0.0,
        distance_factor: 0.0,
        damage_factor: 0.0,
        linear_damage: false,
        ignores_y_axis: false,
        friendly_fire: false,
    });
    GameplayCatalog::from_test_profiles(
        &database,
        [("test_attacker".to_owned(), tactics)],
        [
            (
                "test_attacker".to_owned(),
                instant_profile("RifleAttack", 10.0),
            ),
            ("test_attacker".to_owned(), grenade_profile),
        ],
    )
}

pub(super) fn hand_attack_catalog() -> GameplayCatalog {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "test_attacker".to_owned(),
            tactics: Some("test_attacker.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "test_target".to_owned(),
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        weapons: vec![Weapon {
            name: "Hammer".to_owned(),
            max_range: Some(3.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "HammerAttack".to_owned(),
            action_type: Some("HandAttack".to_owned()),
            weapon: Some("Hammer".to_owned()),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    GameplayCatalog::from_test_profiles(
        &database,
        [("test_attacker".to_owned(), tactics)],
        [(
            "test_attacker".to_owned(),
            instant_profile("HammerAttack", 3.0),
        )],
    )
}

fn ranged_action(name: &str, weapon: &str) -> Action {
    Action {
        name: name.to_owned(),
        action_type: Some("RangedAttack".to_owned()),
        weapon: Some(weapon.to_owned()),
        ..Action::default()
    }
}

fn instant_profile(action_name: &str, max_range: f32) -> AttackProfile {
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
            asset_path: "test_attack.uax".to_owned(),
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
