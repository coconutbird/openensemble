use super::*;
use crate::entities::ShieldCoverage;
use crate::world::combat::AttackDamage;
use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Tech};

const ACTION: &str = "FiendishReturn";

#[test]
fn reflection_uses_truncated_nominal_damage_and_never_recurses() {
    let (_database, gameplay) = fixture(false, false);
    let (mut world, attacker_id, target_id) = combat_world();
    let target = world.get_unit_mut(target_id).unwrap();
    target.shields.configure(ShieldCoverage::Full, 50.0);
    target.shields.set_current(50.0);
    world
        .get_unit_mut(attacker_id)
        .unwrap()
        .damage_taken_multiplier = 2.0;

    apply_attack(&mut world, attacker_id, target_id, 19.9, &gameplay);

    let target = world.get_unit(target_id).unwrap();
    assert_close(target.hitpoints, 100.0);
    assert_close(target.shields.current, 30.1);
    assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 81.0);
}

#[test]
fn start_disabled_reflection_accepts_a_live_action_override() {
    let (_database, gameplay) = fixture(true, false);
    let (mut world, attacker_id, target_id) = combat_world();

    apply_attack(&mut world, attacker_id, target_id, 20.0, &gameplay);
    assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 100.0);

    world
        .get_unit_mut(target_id)
        .unwrap()
        .actions
        .set_enabled(ACTION, true);
    apply_attack(&mut world, attacker_id, target_id, 20.0, &gameplay);
    assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 90.0);
}

#[test]
fn player_technology_enables_and_modifies_reflection_work_rate() {
    let (database, gameplay) = fixture(true, true);
    let (mut world, attacker_id, target_id) = combat_world();

    apply_attack(&mut world, attacker_id, target_id, 19.9, &gameplay);
    assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 100.0);
    assert_eq!(
        world.activate_technology(2, &database, "ReturnUpgrade"),
        Ok(true)
    );
    apply_attack(&mut world, attacker_id, target_id, 19.9, &gameplay);
    assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 81.0);
}

#[test]
fn invulnerable_defender_still_emits_the_retail_damage_event() {
    let (_database, gameplay) = fixture(false, false);
    let (mut world, attacker_id, target_id) = combat_world();
    world
        .get_unit_mut(target_id)
        .unwrap()
        .set_invulnerable(true);

    apply_attack(&mut world, attacker_id, target_id, 19.9, &gameplay);

    assert_close(world.get_unit(target_id).unwrap().hitpoints, 100.0);
    assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 90.5);
}

#[test]
fn damage_proxy_squad_owns_the_reflection_event() {
    let (_database, gameplay) = fixture(false, false);
    let mut world = base_world();
    let (_attacker_squad, attacker_id) = create_squad_unit(&mut world, 1, Vec3::ZERO, "reflector");
    let (protected_squad, protected_id) = create_squad_unit(&mut world, 2, Vec3::X, "plain");
    let (proxy_squad, proxy_id) = create_squad_unit(&mut world, 2, Vec3::X, "reflector");
    world
        .get_squad_mut(protected_squad)
        .unwrap()
        .set_damage_proxy(proxy_squad);

    apply_attack(&mut world, attacker_id, protected_id, 20.0, &gameplay);

    assert_close(world.get_unit(protected_id).unwrap().hitpoints, 100.0);
    assert_close(world.get_unit(proxy_id).unwrap().hitpoints, 80.0);
    assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 90.0);
}

fn fixture(starts_disabled: bool, include_technology: bool) -> (Database, GameplayCatalog) {
    let mut database = Database {
        objects: vec![ProtoObject {
            name: "reflector".to_owned(),
            tactics: Some("reflector.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    if include_technology {
        database.techs.push(reflection_technology());
    }
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [(
            "reflector".to_owned(),
            TacticData {
                actions: vec![Action {
                    name: ACTION.to_owned(),
                    action_type: Some("ReflectDamage".to_owned()),
                    work_rate: Some(0.5),
                    start_disabled: Some(starts_disabled),
                    ..Action::default()
                }],
                tactic: Some(TacticRules {
                    persistent_squad_actions: vec![ACTION.to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    );
    (database, gameplay)
}

fn reflection_technology() -> Tech {
    Tech {
        name: "ReturnUpgrade".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![
                technology_effect("ActionEnable", 1.0, "Absolute"),
                technology_effect("WorkRate", 2.0, "Percent"),
            ],
        }),
        ..Tech::default()
    }
}

fn technology_effect(
    subtype: &str,
    amount: f32,
    relativity: &str,
) -> pipeline::database::hw1::techs::TechEffect {
    pipeline::database::hw1::techs::TechEffect {
        effect_type: "Data".to_owned(),
        subtype: Some(subtype.to_owned()),
        amount: Some(amount),
        relativity: Some(relativity.to_owned()),
        action: Some(ACTION.to_owned()),
        target: Some(EffectTarget {
            target_type: Some("ProtoUnit".to_owned()),
            value: Some("reflector".to_owned()),
        }),
        ..pipeline::database::hw1::techs::TechEffect::default()
    }
}

fn combat_world() -> (World, EntityId, EntityId) {
    let mut world = base_world();
    let (_, attacker_id) = create_squad_unit(&mut world, 1, Vec3::ZERO, "reflector");
    let (_, target_id) = create_squad_unit(&mut world, 2, Vec3::X, "reflector");
    (world, attacker_id, target_id)
}

fn base_world() -> World {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world
}

fn create_squad_unit(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    proto_object: &str,
) -> (EntityId, EntityId) {
    let squad_id = world.create_squad_at(player_id, position);
    let unit_id = world.create_unit_at(player_id, position);
    world.get_unit_mut(unit_id).unwrap().proto_object_name = proto_object.to_owned();
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

fn apply_attack(
    world: &mut World,
    attacker_id: EntityId,
    target_id: EntityId,
    damage: f32,
    gameplay: &GameplayCatalog,
) {
    let _dealt = world.apply_attack_damage(
        &AttackDamage {
            attacker_id,
            attacker_player_id: 1,
            primary_target_id: Some(target_id),
            ground_zero: Vec3::ZERO,
            direction: Vec3::X,
            damage,
            weapon_type: None,
            area_damage: None,
        },
        Some(gameplay),
    );
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.000_1,
        "expected {expected}, got {actual}"
    );
}
