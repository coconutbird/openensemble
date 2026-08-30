use crate::entities::squads::SquadPullPhase;
use crate::gameplay::{
    AttackAccuracyProfile, AttackAmmunition, AttackAnimation, AttackProfile,
    ChargedAttackAnimation, GameplayCatalog, ProjectileReactionFlags, PullAttackProfile,
};
use crate::player::TeamRelation;
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::tactics::{
    Action, AnimationRef, EndAnimationRef, ProtoObjectRef, TacticData, TacticRules, TargetRule,
    Weapon,
};
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper, TechEffect};
use pipeline::database::hw1::{Database, ProtoObject, Tech};

#[test]
fn charge_accumulates_from_the_logical_proto_after_a_transform() {
    let (mut database, gameplay) = charge_fixture(true);
    database.techs.push(action_enable_tech());
    let (mut world, attacker_id, _, _) = pull_world();
    {
        let attacker = world.get_unit_mut(attacker_id).unwrap();
        attacker.proto_object_name = "chief03".to_owned();
        attacker.logical_proto_object_name = "chief01".to_owned();
    }

    world.update_entities_with_gameplay(1.0, &gameplay);
    assert!(world.get_unit(attacker_id).unwrap().charge_seconds().abs() < f32::EPSILON);
    world
        .activate_technology(1, &database, "BrutePullUpgrade")
        .unwrap();
    world.update_entities_with_gameplay(1.0, &gameplay);

    let attacker = world.get_unit(attacker_id).unwrap();
    assert_eq!(attacker.charge_action_name(), Some("ChargeAction"));
    assert!((attacker.charge_seconds() - 1.0).abs() < 0.000_1);
}

#[test]
fn ready_pull_replaces_damage_and_flies_the_target_squad() {
    let (_database, gameplay) = charge_fixture(false);
    let (mut world, attacker_id, target_squad_id, target_id) = pull_world();

    world.update_entities_with_gameplay(1.0, &gameplay);
    let effect_id = world
        .get_unit(attacker_id)
        .unwrap()
        .charge_effect_entity_id()
        .expect("ready Charge effect");
    assert!(world.get_object(effect_id).is_some());
    assert!(world.issue_attack_order(1, attacker_id, target_squad_id, 0.0));

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!((world.get_unit(target_id).unwrap().hitpoints - 100.0).abs() < f32::EPSILON);
    assert_eq!(
        world.get_squad(target_squad_id).unwrap().pull_phase(),
        SquadPullPhase::Flying
    );
    assert_eq!(
        world.get_squad(target_squad_id).unwrap().pulled_by(),
        Some(attacker_id)
    );
    assert!(!world.get_unit(target_id).unwrap().is_attackable());
    assert!(
        world
            .get_unit(attacker_id)
            .unwrap()
            .combat
            .uses_charged_animation()
    );
    assert!(
        world
            .get_unit(attacker_id)
            .unwrap()
            .charge_effect_entity_id()
            .is_some()
    );

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(
        world
            .get_unit(attacker_id)
            .unwrap()
            .charge_effect_entity_id()
            .is_some()
    );
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(
        world
            .get_unit(attacker_id)
            .unwrap()
            .charge_effect_entity_id()
            .is_none()
    );

    for _ in 0..20 {
        world.update_entities_with_gameplay(0.05, &gameplay);
    }
    let target = world.get_unit(target_id).unwrap();
    assert!((target.hitpoints - 100.0).abs() < f32::EPSILON);
    assert!(target.base.position.x < 6.0, "target landed beside puller");
    assert_eq!(
        world.get_squad(target_squad_id).unwrap().pull_phase(),
        SquadPullPhase::Inactive
    );
    assert!(target.is_attackable());
    assert_eq!(
        world
            .entity_scripted_animation(target_id)
            .unwrap()
            .animation_type(),
        "Idle"
    );
}

fn charge_fixture(starts_disabled: bool) -> (Database, GameplayCatalog) {
    let database = Database {
        objects: vec![
            ProtoObject {
                name: "chief03".to_owned(),
                tactics: Some("chief.tactics".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "target".to_owned(),
                object_types: vec!["Infantry".to_owned()],
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "charged_fx".to_owned(),
                object_class: Some("Object".to_owned()),
                ..ProtoObject::default()
            },
        ],
        ..Database::default()
    };
    let charge = Action {
        name: "ChargeAction".to_owned(),
        action_type: Some("Charge".to_owned()),
        start_disabled: Some(starts_disabled),
        damage_charge: Some(1.0),
        anim: Some(AnimationRef {
            name: "Pull".to_owned(),
            ..AnimationRef::default()
        }),
        proto_object: Some(ProtoObjectRef {
            name: "charged_fx".to_owned(),
            bone: Some("BoneFX".to_owned()),
            ..ProtoObjectRef::default()
        }),
        ..Action::default()
    };
    let attack = Action {
        name: "StunPull".to_owned(),
        action_type: Some("HandAttack".to_owned()),
        weapon: Some("PullHammer".to_owned()),
        default: Some(true),
        invalid_targets: vec!["scarab".to_owned()],
        velocity_scalar: Some(40.0),
        end_anim: Some(EndAnimationRef {
            name: "Flail".to_owned(),
            ..EndAnimationRef::default()
        }),
        ..Action::default()
    };
    let tactics = TacticData {
        weapons: vec![Weapon {
            name: "PullHammer".to_owned(),
            damage_per_second: Some(10.0),
            max_range: Some(3.0),
            max_pull_range: Some(55.0),
            pull_units: Some(true),
            ..Weapon::default()
        }],
        actions: vec![charge, attack],
        tactic: Some(TacticRules {
            persistent_actions: vec!["ChargeAction".to_owned()],
            target_rules: vec![TargetRule {
                relation: Some("Enemy".to_owned()),
                action: Some("StunPull".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_test_profiles(
        &database,
        [("chief03".to_owned(), tactics)],
        [("chief03".to_owned(), pull_attack_profile())],
    );
    (database, catalog)
}

fn pull_attack_profile() -> AttackProfile {
    AttackProfile {
        action_name: "StunPull".to_owned(),
        animation_type: "Attack".to_owned(),
        weapon_name: "PullHammer".to_owned(),
        weapon_type: None,
        projectile: None,
        impact_effect: None,
        area_damage: None,
        pull: Some(PullAttackProfile {
            max_range: 55.0,
            invalid_targets: vec!["scarab".to_owned()],
            end_animation_type: Some("Flail".to_owned()),
            velocity_scalar: 40.0,
        }),
        hardpoint: None,
        orientation: crate::gameplay::AttackOrientationProfile::default(),
        charged_animation: Some(ChargedAttackAnimation {
            animation_type: "Pull".to_owned(),
            animations: vec![AttackAnimation {
                asset_path: "pull.uax".to_owned(),
                weight: 1,
                duration: 0.1,
                attack_positions: vec![0.0],
                events: Vec::new(),
                hardpoint_track: None,
            }],
        }),
        friendly_fire: false,
        targets_foot_of_unit: false,
        projectile_reactions: ProjectileReactionFlags::default(),
        max_range: 3.0,
        max_velocity_lead: 0.0,
        accuracy: AttackAccuracyProfile::default(),
        damage_per_attack: 10.0,
        ammunition: AttackAmmunition::None,
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

fn pull_world() -> (World, crate::EntityId, crate::EntityId, crate::EntityId) {
    let mut world = World::with_seed(7);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    assert!(world.set_mutual_team_relation(1, 2, TeamRelation::Enemy));
    let attacker_id = world.create_unit_at(1, Vec3::ZERO);
    let attacker = world.get_unit_mut(attacker_id).unwrap();
    attacker.proto_object_name = "chief03".to_owned();
    attacker.logical_proto_object_name = "chief03".to_owned();
    attacker.obstruction_half_extents = Vec3::ONE;

    let target_squad_id = world.create_squad_at(2, Vec3::X * 30.0);
    let target_id = world.create_unit_at(2, Vec3::X * 30.0);
    let target = world.get_unit_mut(target_id).unwrap();
    target.proto_object_name = "target".to_owned();
    target.logical_proto_object_name = "target".to_owned();
    target.object_types = vec!["Infantry".to_owned()];
    target.obstruction_half_extents = Vec3::ONE;
    assert!(world.attach_unit_to_squad(target_id, target_squad_id));
    (world, attacker_id, target_squad_id, target_id)
}

fn action_enable_tech() -> Tech {
    Tech {
        name: "BrutePullUpgrade".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![TechEffect {
                effect_type: "Data".to_owned(),
                subtype: Some("ActionEnable".to_owned()),
                amount: Some(1.0),
                relativity: Some("Absolute".to_owned()),
                action: Some("ChargeAction".to_owned()),
                target: Some(EffectTarget {
                    target_type: Some("ProtoUnit".to_owned()),
                    value: Some("chief01".to_owned()),
                }),
                ..TechEffect::default()
            }],
        }),
        ..Tech::default()
    }
}
