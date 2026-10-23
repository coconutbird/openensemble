use crate::gameplay::{
    AreaDamageProfile, AttackAccuracyProfile, AttackAmmunition, AttackAnimation,
    AttackAnimationAnchor, AttackAnimationEvent, AttackAnimationEventKind, AttackAttachmentPose,
    AttackHardpointProfile, AttackProfile, GameplayCatalog,
};
use glam::Mat4;
use pipeline::database::hw1::tactics::{
    Action, DpsRamp, TacticData, TacticRules, TargetRule, Weapon,
};
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
    let mut grenade_action = ranged_action("GrenadeAttack", "Grenade");
    grenade_action.dont_loop_attack_anim = Some(true);
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
        actions: vec![ranged_action("RifleAttack", "Rifle"), grenade_action],
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
    grenade_profile.control.dont_loop_attack_animation = true;
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

pub(super) fn ammunition_ability_catalog(
    ammunition: AttackAmmunition,
    monitored: bool,
) -> GameplayCatalog {
    let mut database = Database::new();
    database.abilities.extend([
        Ability {
            name: "Command".to_owned(),
            ..Ability::default()
        },
        Ability {
            name: "AmmoAbility".to_owned(),
            recover_start: Some("Attack".to_owned()),
            recover_type: Some("Ability".to_owned()),
            recover_time: Some(2.0),
            ..Ability::default()
        },
    ]);
    database.objects.extend([
        ProtoObject {
            name: "test_attacker".to_owned(),
            tactics: Some("test_attacker.tactics".to_owned()),
            ability_command: Some("AmmoAbility".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "test_target".to_owned(),
            object_types: vec!["NonFlying".to_owned()],
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        weapons: vec![Weapon {
            name: "AmmoWeapon".to_owned(),
            max_range: Some(10.0),
            uses_ammo: Some(true),
            dps_ramp: monitored.then_some(DpsRamp {
                shield_drain: Some(true),
                ..DpsRamp::default()
            }),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "AmmoAttack".to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some("AmmoWeapon".to_owned()),
            stop_attacking_when_ammo_depleted: Some(
                ammunition == AttackAmmunition::CompleteWhenDepleted,
            ),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                relation: Some("Enemy".to_owned()),
                squad_mode: Some("Normal".to_owned()),
                target_types: vec!["NonFlying".to_owned()],
                action: Some("AmmoAttack".to_owned()),
                ability: Some("Command".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let mut profile = instant_profile("AmmoAttack", 10.0);
    profile.ammunition = ammunition;
    profile.control.shield_drain = monitored;
    GameplayCatalog::from_test_profiles(
        &database,
        [("test_attacker".to_owned(), tactics)],
        [("test_attacker".to_owned(), profile)],
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

pub(super) fn secondary_turret_catalog() -> GameplayCatalog {
    secondary_turret_catalog_with_minimum(0.0)
}

pub(super) fn secondary_turret_catalog_with_minimum(min_range: f32) -> GameplayCatalog {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "test_turret".to_owned(),
            tactics: Some("test_turret.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "test_target".to_owned(),
            ..ProtoObject::default()
        },
    ]);
    let action = Action {
        name: "TurretAttack".to_owned(),
        action_type: Some("SecondaryTurretAttack".to_owned()),
        weapon: Some("TurretGun".to_owned()),
        ..Action::default()
    };
    let tactics = TacticData {
        weapons: vec![Weapon {
            name: "TurretGun".to_owned(),
            min_range: Some(min_range),
            max_range: Some(20.0),
            ..Weapon::default()
        }],
        actions: vec![action],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                relation: Some("Enemy".to_owned()),
                squad_mode: Some("Normal".to_owned()),
                action: Some("TurretAttack".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let anchor = AttackAnimationAnchor {
        links: vec![AttackAttachmentPose {
            child_component: "turret".to_owned(),
            to_bone: Some(Mat4::IDENTITY),
            from_bone: Some(Mat4::IDENTITY),
            disregard_orientation: false,
        }],
        bone_to_component: Some(Mat4::IDENTITY),
        single_bone_poses: Vec::new(),
    };
    let mut profile = instant_profile("TurretAttack", 20.0);
    profile.weapon_name = "TurretGun".to_owned();
    profile.min_range = min_range;
    profile.hardpoint = Some(AttackHardpointProfile::for_test(Some("turret"), None));
    profile.animations[0].events = vec![AttackAnimationEvent {
        position: 0.5,
        kind: AttackAnimationEventKind::Attack { to_bone: None },
        anchor: Some(anchor),
    }];
    profile.animations[0].attack_positions = vec![0.5];
    GameplayCatalog::from_test_profiles(
        &database,
        [("test_turret".to_owned(), tactics)],
        [("test_turret".to_owned(), profile)],
    )
}

pub(in crate::world) fn ranged_action(name: &str, weapon: &str) -> Action {
    Action {
        name: name.to_owned(),
        action_type: Some("RangedAttack".to_owned()),
        weapon: Some(weapon.to_owned()),
        ..Action::default()
    }
}

pub(in crate::world) fn instant_profile(action_name: &str, max_range: f32) -> AttackProfile {
    AttackProfile {
        action_name: action_name.to_owned(),
        animation_type: "Attack".to_owned(),
        weapon_name: action_name.to_owned(),
        weapon_type: None,
        projectile: None,
        impact_effect: None,
        area_damage: None,
        knockback: None,
        death_throw: crate::gameplay::DeathThrowProfile::default(),
        weapon_effects: None,
        pull: None,
        hardpoint: None,
        orientation: crate::gameplay::AttackOrientationProfile::default(),
        charged_animation: None,
        friendly_fire: false,
        targets_foot_of_unit: false,
        projectile_reactions: crate::gameplay::ProjectileReactionFlags::default(),
        min_range: 0.0,
        max_range,
        max_velocity_lead: 0.0,
        accuracy: AttackAccuracyProfile::default(),
        damage_per_attack: 5.0,
        control: crate::gameplay::AttackControlProfile::default(),
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
