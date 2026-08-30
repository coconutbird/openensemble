use super::*;
use crate::entities::{AircraftCrashPhase, FlightControllerKind};
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::player::TeamRelation;
use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, ImpactEffect, TacticData, TacticRules, Weapon};
use pipeline::database::hw1::{Database, ProtoObject};

const STEP: f32 = 0.05;

#[test]
fn no_gameplay_leaves_air_avoidance_inactive() {
    let mut world = World::new();
    world.init_players(1);
    let unit_id = world.create_unit(1);

    world.update_entities(STEP);

    assert!(!world.get_unit(unit_id).unwrap().is_crashing());
}

#[test]
fn birth_speed_limit_expires_after_retail_window() {
    let gameplay = air_gameplay();
    let (mut world, squad_id, unit_id) = one_aircraft_world(7, Vec3::ZERO);
    {
        let squad = world.get_squad_mut(squad_id).unwrap();
        squad.speed = 30.0;
        squad.base.set_forward(Vec3::X);
        squad.move_to(Vec3::X * 100.0);
    }

    world.update_entities_with_gameplay(STEP, &gameplay);

    let moving_squad = world.get_squad(squad_id).unwrap();
    assert!((moving_squad.base.position.x - 0.4).abs() < 0.000_1);
    assert_eq!(moving_squad.leash_position(), moving_squad.base.position);
    assert_eq!(moving_squad.anchor_position(), moving_squad.base.position);
    assert!(world.get_unit(unit_id).unwrap().is_air_speed_limited());

    for _ in 0..33 {
        world.update_entities_with_gameplay(STEP, &gameplay);
    }
    assert!(!world.get_unit(unit_id).unwrap().is_air_speed_limited());
    let before = world.get_squad(squad_id).unwrap().base.position.x;
    world.update_entities_with_gameplay(STEP, &gameplay);
    let step = world.get_squad(squad_id).unwrap().base.position.x - before;
    assert!((step - 1.5).abs() < 0.000_1);
}

#[test]
fn hover_flight_tracks_terrain_clearance_instead_of_order_altitude() {
    let gameplay = air_gameplay_with_hover_offset(Some(3.0));
    let terrain_height = 4.0;
    let (mut world, squad_id, _unit_id) =
        one_aircraft_world(13, Vec3::new(3.0, terrain_height, 3.0));
    world.configure_flat_test_terrain(terrain_height);
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .move_to(Vec3::new(100.0, -50.0, 3.0));

    for _ in 0..200 {
        world.update_entities_with_gameplay(STEP, &gameplay);
    }

    let squad = world.get_squad(squad_id).unwrap();
    assert!(
        (squad.base.position.y - 23.0).abs() < 0.5,
        "hover altitude was {}",
        squad.base.position.y
    );
    assert!(squad.base.position.x > 50.0);
}

#[test]
fn exact_overlap_separates_friendly_aircraft_deterministically() {
    let gameplay = air_gameplay();
    let first = overlap_result(91, &gameplay);
    let second = overlap_result(91, &gameplay);

    assert_eq!(first, second);
    assert!(first.0 != Vec3::ZERO);
    assert!(first.1 != Vec3::ZERO);
    assert!(first.2.distance(first.3) > 0.001);
}

#[test]
fn target_depression_uses_reverse_speed_or_lateral_strafe() {
    let reverse_fallback = target_depression_result(None, false);
    let no_reverse = target_depression_result(Some(0.0), false);

    assert!(reverse_fallback.x < 0.0);
    assert!(reverse_fallback.z.abs() <= f32::EPSILON);
    assert!(no_reverse.x.abs() <= f32::EPSILON);
    assert!(no_reverse.z > 0.0);
}

#[test]
fn target_depression_rejects_obstructs_air_destination() {
    assert_eq!(target_depression_result(Some(0.0), true), Vec3::Y * 20.0);
}

#[test]
fn lethal_kamikaze_crashes_into_forward_enemy_and_emits_impact() {
    let gameplay = air_gameplay();
    let (mut world, _squad_id, aircraft_id) = one_aircraft_world(23, Vec3::Y * 20.0);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    assert!(world.set_mutual_team_relation(1, 2, TeamRelation::Enemy));
    world.set_fog_of_war_enabled(false);
    let target_id = world.create_unit_at(2, Vec3::new(10.0, 20.0, 0.0));
    {
        let target = world.get_unit_mut(target_id).unwrap();
        target.proto_object_name = "target".to_owned();
        target.obstruction_half_extents = Vec3::ONE;
    }
    {
        let aircraft = world.get_unit_mut(aircraft_id).unwrap();
        aircraft.actions.set_enabled("AvoidCollisionAir", false);
        aircraft.actions.set_enabled("KamikazeOnDeath", true);
    }

    assert!(world.damage_unit_with_gameplay(aircraft_id, 1_000.0, &gameplay));
    let aircraft = world.get_unit(aircraft_id).unwrap();
    assert_eq!(
        aircraft.aircraft_crash_phase(),
        AircraftCrashPhase::PendingTarget
    );
    assert!((aircraft.hitpoints - 1.0).abs() <= f32::EPSILON);
    assert!(!aircraft.base.is_selectable());
    assert!(aircraft.is_attackable());

    world.update_entities_with_gameplay(STEP, &gameplay);
    let aircraft = world.get_unit(aircraft_id).expect("crash remains airborne");
    assert_eq!(
        aircraft.aircraft_crash_phase(),
        AircraftCrashPhase::Crashing
    );
    assert_eq!(aircraft.kamikaze_target(), Some(target_id));
    assert!(!aircraft.is_attackable());

    for _ in 0..20 {
        world.update_entities_with_gameplay(STEP, &gameplay);
        if world.get_unit(aircraft_id).is_none() {
            break;
        }
    }

    assert!(world.get_unit(aircraft_id).is_none());
    assert!(world.get_unit(target_id).is_none());
    let impact = world
        .impact_effect_requests_after(0)
        .next()
        .expect("authoritative crash impact");
    assert_eq!(impact.projectile_id(), aircraft_id);
    assert_eq!(impact.primary_target_id(), Some(target_id));
    assert_eq!(impact.effect().name, "Tankshell");
}

#[test]
fn crash_final_damage_credits_the_original_killer() {
    let gameplay = air_gameplay();
    let (mut world, _aircraft_squad, aircraft_id) = one_aircraft_world(25, Vec3::Y * 20.0);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    assert!(world.set_mutual_team_relation(1, 2, TeamRelation::Enemy));
    world.set_fog_of_war_enabled(false);
    let target_position = Vec3::new(10.0, 20.0, 0.0);
    let killer_squad_id = world.create_squad_at(2, target_position);
    let killer_id = world.create_unit_at(2, target_position);
    {
        let killer = world.get_unit_mut(killer_id).unwrap();
        killer.proto_object_name = "target".to_owned();
        killer.obstruction_half_extents = Vec3::ONE;
    }
    assert!(world.attach_unit_to_squad(killer_id, killer_squad_id));
    {
        let aircraft = world.get_unit_mut(aircraft_id).unwrap();
        aircraft.actions.set_enabled("AvoidCollisionAir", false);
        aircraft.actions.set_enabled("KamikazeOnDeath", true);
    }

    assert!(
        world.apply_reflected_collision_damage(killer_id, 2, aircraft_id, 1_000.0, &gameplay) > 0.0
    );
    assert!(
        (world
            .get_squad(killer_squad_id)
            .unwrap()
            .banked_experience()
            - 49.5)
            .abs()
            < 0.001
    );
    world.prepare_aircraft_crashes(&gameplay);
    world.detonate_crashing_aircraft(aircraft_id, Some(killer_id), &gameplay);

    let aircraft = world
        .get_unit(aircraft_id)
        .expect("forced crash death remains observable until cleanup");
    assert!(!aircraft.is_alive());
    assert_eq!(aircraft.killed_by_entity_id(), Some(killer_id));
    assert_eq!(aircraft.killed_by_player_id(), Some(2));
    assert_eq!(aircraft.killed_by_team_id(), Some(2));
    assert_eq!(aircraft.killed_by_weapon_type(), Some("Basic"));
    assert!(
        (world
            .get_squad(killer_squad_id)
            .unwrap()
            .banked_experience()
            - 50.0)
            .abs()
            < 0.001
    );
}

#[test]
fn kamikaze_target_query_rejects_unrevealed_enemies() {
    let gameplay = air_gameplay();
    let (mut world, _squad_id, aircraft_id) = one_aircraft_world(27, Vec3::Y * 20.0);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    assert!(world.set_mutual_team_relation(1, 2, TeamRelation::Enemy));
    let hidden_target = world.create_unit_at(2, Vec3::new(10.0, 20.0, 0.0));
    world.get_unit_mut(hidden_target).unwrap().proto_object_name = "target".to_owned();
    let aircraft = world.get_unit_mut(aircraft_id).unwrap();
    aircraft.actions.set_enabled("AvoidCollisionAir", false);
    aircraft.actions.set_enabled("KamikazeOnDeath", true);

    assert!(world.damage_unit_with_gameplay(aircraft_id, 1_000.0, &gameplay));
    world.prepare_aircraft_crashes(&gameplay);

    let aircraft = world.get_unit(aircraft_id).unwrap();
    assert_eq!(
        aircraft.aircraft_crash_phase(),
        AircraftCrashPhase::Crashing
    );
    assert_eq!(aircraft.kamikaze_target(), None);
}

#[test]
fn wave_kamikaze_suppression_survives_first_profile_reconciliation() {
    let gameplay = air_gameplay();
    let (mut suppressed_world, _squad_id, suppressed_id) = one_aircraft_world(29, Vec3::Y * 20.0);
    suppressed_world
        .get_unit_mut(suppressed_id)
        .unwrap()
        .set_aircraft_can_kamikaze(false);

    assert!(suppressed_world.damage_unit_with_gameplay(suppressed_id, 1_000.0, &gameplay));
    let suppressed = suppressed_world.get_unit(suppressed_id).unwrap();
    assert!(suppressed.hitpoints.abs() <= f32::EPSILON);
    assert_eq!(
        suppressed.aircraft_crash_phase(),
        AircraftCrashPhase::Inactive
    );

    let (mut released_world, _squad_id, released_id) = one_aircraft_world(31, Vec3::Y * 20.0);
    let released = released_world.get_unit_mut(released_id).unwrap();
    released.set_aircraft_can_kamikaze(false);
    released.set_aircraft_can_kamikaze(true);

    assert!(released_world.damage_unit_with_gameplay(released_id, 1_000.0, &gameplay));
    assert_eq!(
        released_world
            .get_unit(released_id)
            .unwrap()
            .aircraft_crash_phase(),
        AircraftCrashPhase::PendingTarget
    );
}

#[test]
fn displaced_aircraft_returns_to_its_preserved_anchor() {
    let anchor = Vec3::new(10.0, 20.0, 10.0);
    let (position, leash) = anchor_return_result(anchor, false);

    assert_eq!(position, anchor);
    assert_eq!(leash, anchor);
}

#[test]
fn friendly_claimed_spot_blocks_aircraft_anchor_return() {
    let anchor = Vec3::new(10.0, 20.0, 10.0);
    let displaced = anchor + Vec3::X * 5.0;
    let (position, leash) = anchor_return_result(anchor, true);

    assert_eq!(position, displaced);
    assert_eq!(leash, displaced);
}

#[test]
fn steep_target_blocks_return_to_attack_anchor() {
    let gameplay = air_gameplay();
    let anchor = Vec3::new(1.0, 20.0, 0.0);
    let displaced = Vec3::new(30.0, 20.0, 0.0);
    let (mut world, squad_id, aircraft_id) = one_aircraft_world(41, anchor);
    let target_id = world.create_unit_at(1, Vec3::ZERO);
    world.get_unit_mut(aircraft_id).unwrap().attack_target = Some(target_id);
    world.configure_air_avoidance_for_damage(aircraft_id, &gameplay);
    {
        let squad = world.get_squad_mut(squad_id).unwrap();
        squad.leash_distance = 2.0;
        squad.configure_leash_profile(1.0, 2_500);
        squad.base.position = displaced;
        squad.set_leash_position(displaced, false);
    }

    world.update_aircraft_steering(STEP, &gameplay);

    assert_eq!(world.get_squad(squad_id).unwrap().base.position, displaced);
}

fn overlap_result(seed: u64, gameplay: &GameplayCatalog) -> (Vec3, Vec3, Vec3, Vec3) {
    let (mut world, first_squad, first_unit) = one_aircraft_world(seed, Vec3::ZERO);
    let (second_squad, second_unit) = spawn_aircraft(&mut world, 1, Vec3::ZERO);

    world.update_entities_with_gameplay(STEP, gameplay);

    (
        world.get_unit(first_unit).unwrap().air_avoidance_vector(),
        world.get_unit(second_unit).unwrap().air_avoidance_vector(),
        world.get_squad(first_squad).unwrap().base.position,
        world.get_squad(second_squad).unwrap().base.position,
    )
}

fn target_depression_result(reverse_speed: Option<f32>, has_blocker: bool) -> Vec3 {
    let gameplay = air_gameplay();
    let (mut world, squad_id, aircraft_id) = one_aircraft_world(19, Vec3::Y * 20.0);
    let target_id = world.create_unit_at(1, Vec3::X);
    {
        let aircraft = world.get_unit_mut(aircraft_id).unwrap();
        aircraft.configure_air_navigation(reverse_speed, false);
        aircraft.attack_target = Some(target_id);
    }
    if has_blocker {
        let blocker_id = world.create_building_at(1, Vec3::new(0.0, 20.0, 20.0));
        let blocking_unit = world.get_unit_mut(blocker_id).unwrap();
        blocking_unit.obstruction_half_extents = Vec3::splat(3.0);
        blocking_unit.configure_air_navigation(None, true);
    }
    world.configure_air_avoidance_for_damage(aircraft_id, &gameplay);

    world.update_aircraft_steering(STEP, &gameplay);

    world.get_squad(squad_id).unwrap().base.position
}

fn anchor_return_result(anchor: Vec3, occupied: bool) -> (Vec3, Vec3) {
    let gameplay = air_gameplay();
    let (mut world, squad_id, aircraft_id) = one_aircraft_world(37, anchor);
    world.configure_air_avoidance_for_damage(aircraft_id, &gameplay);
    let displaced = anchor + Vec3::X * 5.0;
    {
        let squad = world.get_squad_mut(squad_id).unwrap();
        squad.leash_distance = 2.0;
        squad.configure_leash_profile(1.0, 2_500);
        squad.base.position = displaced;
        squad.set_leash_position(displaced, false);
    }
    if occupied {
        let _neighbor = spawn_aircraft(&mut world, 1, anchor);
    }

    world.update_aircraft_steering(STEP, &gameplay);

    let squad = world.get_squad(squad_id).unwrap();
    (squad.base.position, squad.leash_position())
}

fn one_aircraft_world(seed: u64, position: Vec3) -> (World, crate::EntityId, crate::EntityId) {
    let mut world = World::with_seed(seed);
    world.init_players(1);
    let (squad_id, unit_id) = spawn_aircraft(&mut world, 1, position);
    (world, squad_id, unit_id)
}

fn spawn_aircraft(
    world: &mut World,
    player_id: u8,
    position: Vec3,
) -> (crate::EntityId, crate::EntityId) {
    let squad_id = world.create_squad_at(player_id, position);
    let unit_id = world.create_unit_at(player_id, position);
    {
        let squad = world.get_squad_mut(squad_id).unwrap();
        squad.speed = 10.0;
        squad.base.set_forward(Vec3::X);
    }
    {
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_id = 1;
        unit.proto_object_name = "banshee".to_owned();
        unit.logical_proto_object_name = "banshee".to_owned();
        unit.flying = true;
        unit.configure_flight_controller(FlightControllerKind::PhysicsHover, 0.0);
        unit.speed = 10.0;
        unit.obstruction_half_extents = Vec3::ONE;
        unit.base.set_forward(Vec3::X);
    }
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

fn air_gameplay() -> GameplayCatalog {
    air_gameplay_with_hover_offset(None)
}

fn air_gameplay_with_hover_offset(hover_altitude_offset: Option<f32>) -> GameplayCatalog {
    let database = Database {
        objects: vec![ProtoObject {
            name: "banshee".to_owned(),
            tactics: Some("banshee.tactics".to_owned()),
            hitpoints: Some(100.0),
            bounty: Some(50.0),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let actions = vec![
        Action {
            name: "AvoidCollisionAir".to_owned(),
            action_type: Some("AvoidCollisionAir".to_owned()),
            hover_altitude_offset,
            ..Action::default()
        },
        Action {
            name: "KamikazeOnDeath".to_owned(),
            action_type: Some("AvoidCollisionAir".to_owned()),
            start_disabled: Some(true),
            weapon: Some("KamikazeDive".to_owned()),
            ..Action::default()
        },
    ];
    let weapons = vec![Weapon {
        name: "KamikazeDive".to_owned(),
        damage_per_second: Some(1_500.0),
        max_range: Some(65.0),
        weapon_type: Some("Basic".to_owned()),
        aoe_radius: Some(6.0),
        aoe_primary_target_factor: Some(0.5),
        aoe_distance_factor: Some(0.2),
        aoe_damage_factor: Some(0.2),
        impact_effect: Some(ImpactEffect {
            name: "Tankshell".to_owned(),
            size: Some("Medium".to_owned()),
            ..ImpactEffect::default()
        }),
        ..Weapon::default()
    }];
    GameplayCatalog::from_tactics(
        &database,
        [(
            "banshee".to_owned(),
            TacticData {
                actions,
                weapons,
                tactic: Some(TacticRules {
                    persistent_actions: vec![
                        "AvoidCollisionAir".to_owned(),
                        "KamikazeOnDeath".to_owned(),
                    ],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    )
}
