use super::super::World;
use crate::entities::FlightControllerKind;
use crate::entities::units::{MoveAirActionState, MoveAirTacticState};
use crate::gameplay::{
    AttackAccuracyProfile, AttackAmmunition, AttackAnimation, AttackProfile, GameplayCatalog,
    ProjectileReactionFlags,
};
use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::{Database, ProtoObject};

const STEP: f32 = 0.05;

#[test]
fn move_air_is_deterministic_and_owns_individual_unit_motion() {
    let mut first = move_air_world(17, true);
    let mut second = move_air_world(17, true);

    for _ in 0..80 {
        first.update_move_air(STEP);
        second.update_move_air(STEP);
    }

    let first_unit = first.units.iter().next().unwrap().1;
    let second_unit = second.units.iter().next().unwrap().1;
    assert_eq!(first.checksum(), second.checksum());
    assert_eq!(first_unit.base.position, second_unit.base.position);
    assert!(first_unit.base.position.x > 0.0);
    assert!(first_unit.base.position.is_finite());
    assert!(first_unit.base.forward.y.abs() > 0.0);
}

#[test]
fn unlinked_move_air_refills_then_paths_before_working() {
    let mut world = move_air_world_unprimed(19, false);
    let unit_id = world.units.ids().next().unwrap();
    world
        .get_unit_mut(unit_id)
        .unwrap()
        .ammunition
        .configure(100.0, 0.0, true);
    world
        .get_unit_mut(unit_id)
        .unwrap()
        .ammunition
        .set_current(5.0);

    world.update_move_air(STEP);
    let unit = world.get_unit(unit_id).unwrap();
    assert_close(unit.ammunition.current(), 100.0);
    assert_eq!(
        unit.move_air_state().unwrap().action,
        MoveAirActionState::Pathing
    );
    assert_eq!(unit.base.velocity, Vec3::ZERO);

    world.update_move_air(STEP);
    assert_eq!(
        world
            .get_unit(unit_id)
            .unwrap()
            .move_air_state()
            .unwrap()
            .action,
        MoveAirActionState::Working
    );
    assert_eq!(world.get_unit(unit_id).unwrap().base.velocity, Vec3::ZERO);

    world.update_move_air(STEP);
    assert_ne!(world.get_unit(unit_id).unwrap().base.velocity, Vec3::ZERO);
}

#[test]
fn nonflood_move_air_eases_toward_sixty_percent_speed() {
    let mut world = move_air_world(23, false);
    for _ in 0..20 {
        world.update_move_air(STEP);
    }

    let unit = world.units.iter().next().unwrap().1;
    let planar_speed = Vec3::new(unit.base.velocity.x, 0.0, unit.base.velocity.z).length();
    assert!((planar_speed - 3.849_1).abs() < 0.001);
    assert!(
        unit.move_air_state()
            .is_some_and(|state| state.altitude_select_timer > 2.9)
    );
}

#[test]
fn move_air_combat_cycles_from_strafe_into_an_attack_hover() {
    let gameplay = move_air_combat_catalog();
    let (mut world, squad_id, attacker_id, target_id) = move_air_combat_world();
    assert!(world.issue_attack_order(1, squad_id, target_id, 0.0));
    let initial_hitpoints = world.get_unit(target_id).unwrap().hitpoints;

    world.update_entities_with_gameplay(STEP, &gameplay);
    let attacker = world.get_unit(attacker_id).unwrap();
    assert!(attacker.is_move_air_attack_blocked());
    assert_eq!(
        attacker.move_air_state().unwrap().tactic,
        MoveAirTacticState::Strafe
    );
    assert_close(
        world.get_unit(target_id).unwrap().hitpoints,
        initial_hitpoints,
    );

    world.update_entities_with_gameplay(STEP, &gameplay);
    let attacker = world.get_unit(attacker_id).unwrap();
    assert!(attacker.is_move_air_attack_blocked());
    assert_eq!(
        attacker.move_air_state().unwrap().tactic,
        MoveAirTacticState::LaunchHover
    );
    assert_close(
        world.get_unit(target_id).unwrap().hitpoints,
        initial_hitpoints,
    );

    world.update_entities_with_gameplay(STEP, &gameplay);
    assert!(
        !world
            .get_unit(attacker_id)
            .unwrap()
            .is_move_air_attack_blocked()
    );
    assert!(world.get_unit(target_id).unwrap().hitpoints < initial_hitpoints);

    for _ in 0..50 {
        world.update_entities_with_gameplay(STEP, &gameplay);
        if world
            .get_unit(attacker_id)
            .unwrap()
            .is_move_air_attack_blocked()
        {
            break;
        }
    }
    let blocked_hitpoints = world.get_unit(target_id).unwrap().hitpoints;
    let attacker = world.get_unit(attacker_id).unwrap();
    assert!(attacker.is_move_air_attack_blocked());
    assert_eq!(
        attacker.move_air_state().unwrap().tactic,
        MoveAirTacticState::ReturnToSquad
    );
    for _ in 0..5 {
        world.update_entities_with_gameplay(STEP, &gameplay);
    }
    assert_close(
        world.get_unit(target_id).unwrap().hitpoints,
        blocked_hitpoints,
    );
}

#[test]
fn move_air_stray_limit_forces_a_timed_return_to_squad() {
    let mut world = move_air_world(31, true);
    let (unit_id, squad_id) = {
        let (unit_id, unit) = world.units.iter().next().unwrap();
        (unit_id, unit.squad_id.unwrap())
    };
    world.get_unit_mut(unit_id).unwrap().base.position = Vec3::X * 80.0 + Vec3::Y * 10.0;
    world.get_squad_mut(squad_id).unwrap().leash_distance = 20.0;

    world.update_move_air_tactics(STEP, &GameplayCatalog::default());

    let unit = world.get_unit(unit_id).unwrap();
    let state = unit.move_air_state().unwrap();
    assert_eq!(state.tactic, MoveAirTacticState::ReturnToSquad);
    assert_eq!(
        state.goal_position,
        world.get_squad(squad_id).unwrap().base.position
    );
    assert!((state.hover_timer - 1.0).abs() < f32::EPSILON);
    assert!(unit.is_move_air_attack_blocked());
}

fn move_air_world(seed: u64, flood: bool) -> World {
    let mut world = move_air_world_unprimed(seed, flood);
    let unit_id = world.units.ids().next().unwrap();
    let unit = world.get_unit_mut(unit_id).unwrap();
    let mut state = unit.move_air_state().unwrap();
    state.lifecycle.set_initialized(true);
    state.action = MoveAirActionState::Working;
    unit.set_move_air_state(state);
    world
}

fn move_air_world_unprimed(seed: u64, flood: bool) -> World {
    let mut world = World::with_seed(seed);
    world.init_players(1);
    world.configure_flat_test_terrain(0.0);
    let squad_id = world.create_squad_at(1, Vec3::new(20.0, 0.0, 0.0));
    let unit_id = world.create_unit_at(1, Vec3::new(0.0, 10.0, 0.0));
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.flying = true;
    unit.speed = 10.0;
    unit.turn_rate_degrees = 120.0;
    unit.base.set_forward(Vec3::X);
    if flood {
        unit.object_types.push("Flood".to_owned());
    }
    unit.configure_flight_controller(FlightControllerKind::MoveAir, 0.0);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    world
}

fn move_air_combat_world() -> (World, crate::EntityId, crate::EntityId, crate::EntityId) {
    let mut world = World::with_seed(41);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world.configure_flat_test_terrain(0.0);
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let attacker_id = world.create_unit_at(1, Vec3::Y * 10.0);
    let attacker = world.get_unit_mut(attacker_id).unwrap();
    attacker.proto_object_name = "test_air".to_owned();
    attacker.flying = true;
    attacker.speed = 10.0;
    attacker.turn_rate_degrees = 120.0;
    attacker.configure_flight_controller(FlightControllerKind::MoveAir, 10.0);
    assert!(world.attach_unit_to_squad(attacker_id, squad_id));
    let attacker = world.get_unit_mut(attacker_id).unwrap();
    let mut state = attacker.move_air_state().unwrap();
    state.lifecycle.set_initialized(true);
    state.action = MoveAirActionState::Working;
    attacker.set_move_air_state(state);
    let target_id = world.create_unit_at(2, Vec3::X * 10.0);
    let target = world.get_unit_mut(target_id).unwrap();
    target.proto_object_name = "test_target".to_owned();
    target.hitpoints = 10_000.0;
    target.max_hitpoints = 10_000.0;
    (world, squad_id, attacker_id, target_id)
}

fn move_air_combat_catalog() -> GameplayCatalog {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "test_air".to_owned(),
            tactics: Some("test_air.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "test_target".to_owned(),
            hitpoints: Some(10_000.0),
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        actions: vec![Action {
            name: "RifleAttack".to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some("Rifle".to_owned()),
            ..Action::default()
        }],
        weapons: vec![Weapon {
            name: "Rifle".to_owned(),
            max_range: Some(100.0),
            ..Weapon::default()
        }],
        ..TacticData::default()
    };
    GameplayCatalog::from_test_profiles(
        &database,
        [("test_air".to_owned(), tactics)],
        [("test_air".to_owned(), move_air_attack_profile())],
    )
}

fn move_air_attack_profile() -> AttackProfile {
    AttackProfile {
        action_name: "RifleAttack".to_owned(),
        animation_type: "Attack".to_owned(),
        weapon_name: "Rifle".to_owned(),
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
        projectile_reactions: ProjectileReactionFlags::default(),
        max_range: 100.0,
        max_velocity_lead: 0.0,
        accuracy: AttackAccuracyProfile::default(),
        damage_per_attack: 5.0,
        ammunition: AttackAmmunition::None,
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

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < f32::EPSILON);
}
