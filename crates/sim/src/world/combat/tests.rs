use super::test_catalog::ability_catalog;
use super::*;
use crate::gameplay::{AreaDamageProfile, AttackAccuracyProfile, AttackAnimation};
use crate::random::SimRandom;
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::weapontypes::DamageModifier;
use pipeline::database::hw1::{Database, ProtoObject, WeaponType};

fn combat_catalog(area_damage: Option<AreaDamageProfile>) -> GameplayCatalog {
    combat_catalog_with_lead(area_damage, 0.0)
}

fn combat_catalog_with_lead(
    area_damage: Option<AreaDamageProfile>,
    max_velocity_lead: f32,
) -> GameplayCatalog {
    combat_catalog_with_tuning(
        area_damage,
        max_velocity_lead,
        AttackAccuracyProfile::default(),
    )
}

fn combat_catalog_with_tuning(
    area_damage: Option<AreaDamageProfile>,
    max_velocity_lead: f32,
    accuracy: AttackAccuracyProfile,
) -> GameplayCatalog {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "test_attacker".to_owned(),
            tactics: Some("test_attacker.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "test_target".to_owned(),
            damage_type: Some("Light".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "test_bullet".to_owned(),
            dbid: Some(70),
            object_class: Some("Projectile".to_owned()),
            velocity: Some(10.0),
            lifespan: Some(2.0),
            ..ProtoObject::default()
        },
    ]);
    database.weapon_types.push(WeaponType {
        name: "SmallArms".to_owned(),
        damage_modifiers: vec![DamageModifier {
            damage_type: "Light".to_owned(),
            modifier: 2.0,
            ..DamageModifier::default()
        }],
        ..WeaponType::default()
    });
    let tactics = TacticData {
        actions: vec![Action {
            name: "RifleAttack".to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some("Rifle".to_owned()),
            ..Action::default()
        }],
        weapons: vec![Weapon {
            name: "Rifle".to_owned(),
            max_range: Some(10.0),
            ..Weapon::default()
        }],
        ..TacticData::default()
    };
    let profile = AttackProfile {
        action_name: "RifleAttack".to_owned(),
        weapon_name: "Rifle".to_owned(),
        weapon_type: Some("SmallArms".to_owned()),
        projectile: Some("test_bullet".to_owned()),
        area_damage,
        friendly_fire: false,
        targets_foot_of_unit: false,
        max_range: 10.0,
        max_velocity_lead,
        accuracy,
        damage_per_attack: 5.0,
        animations: vec![AttackAnimation {
            asset_path: "test_attack.uax".to_owned(),
            weight: 1,
            duration: 0.1,
            attack_positions: vec![0.0],
        }],
        pre_attack_cooldown: [0.0, 0.0],
        post_attack_cooldown: [0.0, 0.0],
        reload_duration: 0.0,
        visual_ammo: 0,
        uses_height_bonus_damage: false,
    };
    GameplayCatalog::from_test_profiles(
        &database,
        [("test_attacker".to_owned(), tactics)],
        [("test_attacker".to_owned(), profile)],
    )
}

#[test]
fn moving_at_full_speed_selects_the_authored_moving_accuracy() {
    let gameplay = combat_catalog_with_tuning(
        None,
        0.0,
        AttackAccuracyProfile {
            accuracy: 1.0,
            moving_accuracy: -1.0,
            moving_max_deviation: 10.0,
            ..AttackAccuracyProfile::default()
        },
    );
    let (mut world, attacker_id, _) = combat_world();
    let attacker = world.get_unit_mut(attacker_id).unwrap();
    attacker.speed = 10.0;
    attacker.base.velocity = Vec3::X * 9.0;
    let profile = gameplay.initial_attack_profile("test_attacker").unwrap();
    let snapshot = world.attacker_snapshot(attacker_id, profile).unwrap();
    assert!(snapshot.moving_at_full_speed);
    let tuning = world.launch_tuning(&snapshot, profile);
    let mut rng = SimRandom::new();
    let deviation = deviation::projectile_deviation(
        &mut rng,
        Vec3::ZERO,
        Vec3::X * 10.0,
        Vec3::ZERO,
        10.0,
        tuning.accuracy,
    );
    assert!(deviation.length() > 0.0);
}

#[test]
fn moving_target_velocity_is_led_by_the_launched_projectile() {
    let gameplay = combat_catalog_with_lead(None, 2.0);
    let (mut world, _, target_id) = combat_world();
    let target = world.get_unit_mut(target_id).unwrap();
    target.base.velocity = Vec3::Z * 4.0;
    target.speed = 4.0;
    target.state = UnitState::Moving;
    target.move_target = Some(target.base.position + Vec3::Z * 10.0);

    world.update_entities_with_gameplay(0.05, &gameplay);

    let projectile = world.projectiles.iter().next().unwrap().1;
    assert!(projectile.target_position.z > 0.2);
    assert!(projectile.target_position.z < 0.21);
    assert!(projectile.base.velocity.z > 0.0);
}

fn combat_world() -> (World, EntityId, EntityId) {
    let mut world = World::with_seed(41);
    world.init_players(2);
    world.get_player_mut(1).expect("player 1").team_id = 1;
    world.get_player_mut(2).expect("player 2").team_id = 2;
    world.configure_standard_team_relations();
    let attacker_id = world.create_unit_at(1, Vec3::ZERO);
    let target_id = world.create_unit_at(2, Vec3::X);
    world
        .get_unit_mut(attacker_id)
        .expect("attacker")
        .proto_object_name = "test_attacker".to_owned();
    let target = world.get_unit_mut(target_id).expect("target");
    target.proto_object_name = "test_target".to_owned();
    target.damage_taken_multiplier = 0.5;
    assert!(world.issue_attack_order(1, attacker_id, target_id, 0.0));
    (world, attacker_id, target_id)
}

#[test]
fn authored_tag_launches_projectile_and_impact_mutates_sim_hitpoints() {
    let gameplay = combat_catalog(None);
    let (mut world, attacker_id, target_id) = combat_world();

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(world.projectiles.len(), 1);
    assert!((world.get_unit(target_id).expect("target").hitpoints - 100.0).abs() < f32::EPSILON);
    let projectile = world.projectiles.iter().next().expect("projectile").1;
    assert_eq!(projectile.source_id, attacker_id);
    assert_eq!(projectile.target_id, target_id);

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.projectiles.is_empty());
    assert!((world.get_unit(target_id).expect("target").hitpoints - 95.0).abs() < f32::EPSILON);

    let (mut repeat, _, repeat_target_id) = combat_world();
    repeat.update_entities_with_gameplay(0.05, &gameplay);
    repeat.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(world.checksum_with_rng(), repeat.checksum_with_rng());
    assert!(
        (repeat
            .get_unit(repeat_target_id)
            .expect("repeat target")
            .hitpoints
            - 95.0)
            .abs()
            < f32::EPSILON
    );
}

#[test]
fn guaranteed_projectile_miss_is_deterministic_and_does_not_damage_the_target() {
    let accuracy = AttackAccuracyProfile {
        accuracy: -1.0,
        max_deviation: 100.0,
        ..AttackAccuracyProfile::default()
    };
    let gameplay = combat_catalog_with_tuning(None, 0.0, accuracy);
    let (world, target_id, deviation) = run_guaranteed_miss(&gameplay);
    let (repeat, repeat_target_id, repeat_deviation) = run_guaranteed_miss(&gameplay);

    assert!(deviation.length() > 0.5);
    assert_eq!(deviation, repeat_deviation);
    assert!((world.get_unit(target_id).unwrap().hitpoints - 100.0).abs() < f32::EPSILON);
    assert!((repeat.get_unit(repeat_target_id).unwrap().hitpoints - 100.0).abs() < f32::EPSILON);
    assert_eq!(world.checksum_with_rng(), repeat.checksum_with_rng());
}

fn run_guaranteed_miss(gameplay: &GameplayCatalog) -> (World, EntityId, Vec3) {
    let (mut world, attacker_id, target_id) = combat_world();
    let target_position = Vec3::X * 10.0;
    let target = world.get_unit_mut(target_id).unwrap();
    target.base.position = target_position;
    target.obstruction_half_extents = Vec3::splat(0.25);
    world.update_entities_with_gameplay(0.05, gameplay);
    let projectile = world.projectiles.iter().next().unwrap().1;
    let deviation = projectile.target_position - target_position;
    world
        .get_unit_mut(attacker_id)
        .unwrap()
        .clear_attack_order();
    for _ in 0..50 {
        world.update_entities_with_gameplay(0.05, gameplay);
    }
    (world, target_id, deviation)
}

#[test]
fn projectile_impact_applies_its_launch_time_area_damage_to_world_state() {
    let area_damage = AreaDamageProfile {
        radius: 2.0,
        primary_target_factor: 0.0,
        distance_factor: 0.0,
        damage_factor: 0.0,
        linear_damage: false,
        ignores_y_axis: false,
        friendly_fire: false,
    };
    let gameplay = combat_catalog(Some(area_damage));
    let (mut world, _, target_id) = combat_world();
    let secondary_id = world.create_unit_at(2, Vec3::new(1.5, 0.0, 0.0));
    let secondary = world.get_unit_mut(secondary_id).unwrap();
    secondary.proto_object_name = "test_target".to_owned();
    secondary.damage_taken_multiplier = 0.5;

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(
        world.projectiles.iter().next().unwrap().1.area_damage,
        Some(area_damage)
    );
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert!(world.projectiles.is_empty());
    assert!((world.get_unit(target_id).unwrap().hitpoints - 97.5).abs() < f32::EPSILON);
    assert!((world.get_unit(secondary_id).unwrap().hitpoints - 97.5).abs() < f32::EPSILON);
}

#[test]
fn attack_move_acquires_an_enemy_then_resumes_its_destination() {
    let gameplay = combat_catalog(None);
    let mut world = World::with_seed(51);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let attacker_id = world.create_unit_at(1, Vec3::ZERO);
    world.get_unit_mut(attacker_id).unwrap().proto_object_name = "test_attacker".to_owned();
    assert!(world.attach_unit_to_squad(attacker_id, squad_id));
    let squad = world.get_squad_mut(squad_id).unwrap();
    squad.aggro_distance = 20.0;
    squad.leash_distance = 30.0;
    let target_id = world.create_unit_at(2, Vec3::new(5.0, 0.0, 0.0));
    world.get_unit_mut(target_id).unwrap().proto_object_name = "test_target".to_owned();
    let destination = Vec3::new(40.0, 0.0, 0.0);
    assert!(world.issue_squad_move_order_to_position(1, squad_id, destination, true, false,));

    world.update_entities_with_gameplay(0.05, &gameplay);
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.state, SquadState::Attacking);
    assert!(squad.is_auto_attack_engagement());
    assert_eq!(squad.attack_target, Some(target_id));

    world.get_unit_mut(target_id).unwrap().kill();
    world.update_entities_with_gameplay(0.05, &gameplay);
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.state, SquadState::Moving);
    assert_eq!(squad.move_target, Some(destination));
    assert_eq!(squad.attack_target, None);
}

#[test]
fn unbuilt_targets_use_the_database_construction_damage_multiplier() {
    let mut world = World::new();
    let target_id = world.create_building(1);
    let target = world.get_building_mut(target_id).unwrap();
    target.built = false;
    world.set_construction_damage_multiplier(Some(3.0));

    world.apply_weapon_damage(target_id, 10.0, None, None);
    assert!((world.get_building(target_id).unwrap().hitpoints - 70.0).abs() < f32::EPSILON);

    world.get_building_mut(target_id).unwrap().built = true;
    world.apply_weapon_damage(target_id, 10.0, None, None);
    assert!((world.get_building(target_id).unwrap().hitpoints - 60.0).abs() < f32::EPSILON);
}

#[test]
fn ability_context_changes_pursuit_range_and_runtime_attack_action() {
    let gameplay = ability_catalog();
    let (mut world, attacker_id, target_id) = combat_world();
    world.get_unit_mut(target_id).unwrap().base.position = Vec3::new(30.0, 0.0, 0.0);
    let splash_target_id = world.create_unit_at(2, Vec3::new(32.0, 0.0, 0.0));
    world
        .get_unit_mut(splash_target_id)
        .unwrap()
        .proto_object_name = "test_target".to_owned();

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_unit(attacker_id).unwrap().base.position.x > 0.0);
    assert!((world.get_unit(target_id).unwrap().hitpoints - 100.0).abs() < f32::EPSILON);

    assert!(world.issue_attack_order_with_context(1, attacker_id, target_id, 0.0, None, Some(0),));
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert_eq!(
        world.get_unit(attacker_id).unwrap().combat.action_name(),
        Some("GrenadeAttack")
    );
    assert!(world.get_unit(target_id).unwrap().hitpoints < 100.0);
    assert!(world.get_unit(splash_target_id).unwrap().hitpoints < 100.0);
}
