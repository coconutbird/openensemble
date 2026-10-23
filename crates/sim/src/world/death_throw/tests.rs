use super::{METHANE_EXPLOSION, ThrowSource, World};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{
    AreaDamageProfile, DeathChance, DeathThrowProfile, GameplayCatalog, ImpactEffectSize,
};
use crate::physics::test_support::{
    TERRAIN_HEIGHT, box_template, clamshell_template, world_with_flat_terrain,
};
use crate::physics::{ClamshellAnimation, PhysicsTemplate};
use crate::world::combat::{AttackDamage, AttackThrow};
use glam::Vec3;
use pipeline::database::hw1::physics::PhysicsInfoFlags;
use pipeline::database::hw1::{Database, ProtoObject};
use std::f32::consts::TAU;
use std::sync::Arc;

const MARINE: &str = "marine";
const POSITION: Vec3 = Vec3::new(50.0, TERRAIN_HEIGHT, 50.0);
/// The attacker stands 5 units on the negative X side, so the throw goes along +X.
const ATTACKER_OFFSET: Vec3 = Vec3::new(-5.0, 0.0, 0.0);
const MASS: f32 = 2.0;
const PART_MASS: f32 = 10.0;
const TOLERANCE: f32 = 0.000_1;

#[test]
fn the_rocket_death_chance_resets_to_the_minimum_and_grows_after_a_failure() {
    let mut gameplay = GameplayCatalog::from_tactics(&Database::new(), []);
    gameplay.set_test_rocket_death(
        "grunt",
        Some(DeathChance {
            max_chance: 1.5,
            min_chance: -0.06,
            chance_growth: 0.06,
        }),
    );
    assert!(gameplay.rocket_on_death("Grunt"));
    let mut world = World::new();
    assert!(world.check_rocket_death_chance(&gameplay));
    assert_eq!(world.rocket_death_chance, Some(-0.06));
    assert!(!world.check_rocket_death_chance(&gameplay));
    assert!(world.rocket_death_chance.unwrap().abs() < 1.0e-6);
}

#[test]
fn without_death_manager_data_there_is_no_roll() {
    let gameplay = GameplayCatalog::from_tactics(&Database::new(), []);
    let mut world = World::new();
    let before = world.sim_rng;
    assert!(!world.check_rocket_death_chance(&gameplay));
    assert_eq!(world.sim_rng, before);
}

/// A direct hit has the distance factor 1: the force and launch factors clamp to 1, so the
/// throw has the largest force and the largest launch angle.
#[test]
fn a_direct_weapon_kill_throws_the_replacement_with_the_largest_force_and_angle() {
    let (mut world, gameplay, marine, attacker) = box_fixture();
    let _dealt = world.apply_attack_damage(&direct_attack(attacker, marine), Some(&gameplay));

    assert!(world.get_unit(marine).is_none());
    let replacement = thrown_replacement(&world);
    let unit = world.get_unit(replacement).unwrap();
    assert!(!unit.is_alive());
    assert_eq!(unit.hitpoints.to_bits(), 0.0_f32.to_bits());
    assert!(!unit.base.is_selectable());
    let expected = launch(profile().force_max, profile().launch_angle_max) / MASS;
    let velocity = world.unit_body_linear_velocity(replacement).unwrap();
    assert!(
        velocity.abs_diff_eq(expected, TOLERANCE),
        "expected {expected}, got {velocity}"
    );
}

/// The area primary target gets `PrimaryTargetFactor` as its distance factor, and the throw
/// starts at the area centre: `force = lerp(min, max, clamp(0.25 + rand(0, 0.1)))` and the same
/// for the launch angle.
#[test]
fn the_area_primary_target_throw_uses_the_primary_target_factor() {
    let (mut world, gameplay, marine, attacker) = box_fixture();
    let mut attack = direct_attack(attacker, marine);
    attack.ground_zero = POSITION - Vec3::X;
    attack.damage = 100.0;
    attack.area_damage = Some(AreaDamageProfile {
        radius: 4.0,
        primary_target_factor: 0.25,
        distance_factor: 0.0,
        damage_factor: 0.0,
        linear_damage: false,
        ignores_y_axis: false,
        friendly_fire: false,
    });
    let mut rng = world.sim_rng;
    let force_factor = (0.25 + rng.range_float(0.0, 0.1)).clamp(0.0, 1.0);
    let launch_factor = (0.25 + rng.range_float(0.0, 0.1)).clamp(0.0, 1.0);
    let profile = profile();
    let force = (profile.force_max - profile.force_min) * force_factor + profile.force_min;
    let degrees = profile.launch_angle_min
        + (profile.launch_angle_max - profile.launch_angle_min) * launch_factor;

    let _dealt = world.apply_attack_damage(&attack, Some(&gameplay));

    let replacement = thrown_replacement(&world);
    let expected = launch(force, degrees) / MASS;
    let velocity = world.unit_body_linear_velocity(replacement).unwrap();
    assert!(
        velocity.abs_diff_eq(expected, TOLERANCE),
        "expected {expected}, got {velocity}"
    );
    assert_eq!(world.sim_rng, rng);
}

/// The dead replacement is a corpse: the dead-unit pass keeps it, and it goes once its
/// physics action waits (`CompleteOnInactivePhysics` after the body sleeps).
#[test]
fn a_thrown_replacement_stays_until_its_physics_action_waits() {
    // A heavy body lands near the throw point: 300 / 100 = 3 units per second.
    let (mut world, gameplay, marine, attacker) = fixture(&box_template(
        "marine_wreck",
        Vec3::splat(0.5),
        100.0,
        Vec3::Y * 0.5,
    ));
    let _dealt = world.apply_attack_damage(&direct_attack(attacker, marine), Some(&gameplay));
    let replacement = thrown_replacement(&world);

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_unit(replacement).is_some());
    assert_eq!(world.unit_body_is_active(replacement), Some(true));

    let mut asleep = false;
    let removed = (0..400).any(|_| {
        asleep = world.unit_body_is_active(replacement) == Some(false);
        world.update_entities_with_gameplay(0.05, &gameplay);
        world.get_unit(replacement).is_none()
    });
    assert!(removed, "the replacement never came to rest");
    assert!(asleep, "the replacement went before its body slept");
}

/// `applyPointImpulses(J, J, 1.5·J, offset)`: the pelvis gets 1.5 times the impulse of the
/// upper and lower bodies; the offset changes only its angular velocity.
#[test]
fn a_clamshell_throw_gives_the_pelvis_one_and_a_half_times_the_impulse() {
    let (mut world, gameplay, marine, _attacker) = clamshell_fixture();
    let replacement = world
        .create_and_throw_physics_replacement(
            marine,
            &profile(),
            ThrowSource {
                position: POSITION + ATTACKER_OFFSET,
                distance_factor: 1.0,
                force_scaler: 1.0,
            },
            false,
            &gameplay,
        )
        .expect("thrown clamshell replacement");

    let state = world
        .get_unit(replacement)
        .unwrap()
        .physics_object
        .as_ref()
        .unwrap();
    assert_eq!(
        state.death.clamshell_animation,
        Some(ClamshellAnimation::Clamshell)
    );
    let [upper, lower] = state.clamshell_bodies().unwrap();
    let pelvis = state.body;
    let impulse = launch(profile().force_max, profile().launch_angle_max);
    let physics = world.physics.world();
    for (body, scale) in [(pelvis, 1.5), (upper, 1.0), (lower, 1.0)] {
        let velocity = physics.linear_velocity(body).unwrap();
        let expected = impulse * scale / PART_MASS;
        assert!(
            velocity.abs_diff_eq(expected, TOLERANCE),
            "expected {expected}, got {velocity}"
        );
    }
}

/// `setupRocketOnDeath` draws a rotation and then the duration `750 + (rand >> 19 & 0xFFF)`
/// ms. The rocket ends at the first update at or after the timer: the unit goes and the
/// `MethaneExplosion` effect plays at its position.
#[test]
fn a_rocket_death_ends_at_its_timer_with_the_methane_explosion() {
    let (mut world, gameplay, marine, _attacker) = fixture(&box_template(
        "marine_wreck",
        Vec3::splat(0.5),
        200.0,
        Vec3::Y * 0.5,
    ));
    world.game_time_ms = 1_000;
    let mut rng = world.sim_rng;
    let _rotation = rng.range_float(0.0, TAU);
    let duration_ms = 750 + ((rng.next_state() >> 19) & 0xFFF);
    let replacement = world
        .create_and_throw_physics_replacement(
            marine,
            &profile(),
            ThrowSource {
                position: POSITION + ATTACKER_OFFSET,
                distance_factor: 1.0,
                force_scaler: 1.0,
            },
            true,
            &gameplay,
        )
        .expect("thrown rocket replacement");
    let rocket = world
        .get_unit(replacement)
        .unwrap()
        .physics_object
        .as_ref()
        .unwrap()
        .death
        .rocket
        .expect("rocket on death");
    assert_eq!(rocket.start_ms, 1_000);
    assert_eq!(rocket.duration_ms, duration_ms);

    let end_ms = 1_000 + duration_ms.div_ceil(50) * 50;
    while world.get_unit(replacement).is_some() {
        assert!(world.game_time_ms < end_ms, "the rocket outlived its timer");
        world.game_time_ms += 50;
        world.update_entities_with_gameplay(0.05, &gameplay);
    }
    assert_eq!(world.game_time_ms, end_ms);
    let explosion = world
        .impact_effect_requests_after(0)
        .find(|request| request.effect().name == METHANE_EXPLOSION)
        .expect("methane explosion");
    assert_eq!(explosion.effect().size, ImpactEffectSize::Large);
    assert_eq!(explosion.occurred_at_ms(), end_ms);
    assert!(explosion.surface().is_none());
    assert!(!explosion.emits_surface_effect());
    // The rocket lifted the unit before it exploded.
    assert!(explosion.position().y > POSITION.y + 1.0);
}

fn profile() -> DeathThrowProfile {
    DeathThrowProfile {
        throw_units: true,
        force_min: 100.0,
        force_max: 300.0,
        launch_angle_min: 10.0,
        launch_angle_max: 30.0,
        ..DeathThrowProfile::default()
    }
}

/// `force · (dir·cos a, sin a)` along +X.
fn launch(force: f32, degrees: f32) -> Vec3 {
    let angle = degrees.to_radians();
    Vec3::new(angle.cos(), angle.sin(), 0.0) * force
}

fn direct_attack(attacker: EntityId, target: EntityId) -> AttackDamage {
    AttackDamage {
        attacker_id: attacker,
        attacker_player_id: 1,
        primary_target_id: Some(target),
        ground_zero: POSITION,
        direction: Vec3::X,
        damage: 50.0,
        weapon_type: None,
        area_damage: None,
        death_throw: Some(AttackThrow {
            profile: profile(),
            force_origin: POSITION + ATTACKER_OFFSET,
            weapon_effects: None,
            projectile: None,
        }),
    }
}

fn thrown_replacement(world: &World) -> EntityId {
    world
        .units
        .iter()
        .find_map(|(unit_id, unit)| unit.is_physics_replacement().then_some(unit_id))
        .expect("thrown physics replacement")
}

fn box_fixture() -> (World, GameplayCatalog, EntityId, EntityId) {
    fixture(&box_template(
        "marine_wreck",
        Vec3::splat(0.5),
        MASS,
        Vec3::Y * 0.5,
    ))
}

fn clamshell_fixture() -> (World, GameplayCatalog, EntityId, EntityId) {
    fixture(&clamshell_template(
        "marine_wreck",
        Vec3::splat(0.3),
        Vec3::Y,
    ))
}

/// A Marine of player 2 at [`POSITION`] with the thrown replacement `template`, and an
/// attacker of player 1 at the attacker offset.
fn fixture(template: &PhysicsTemplate) -> (World, GameplayCatalog, EntityId, EntityId) {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: MARINE.to_owned(),
        physics_replacement_info: Some("marine_wreck".to_owned()),
        ..ProtoObject::default()
    });
    let mut gameplay = GameplayCatalog::from_tactics(&database, []);
    gameplay.insert_test_physics_replacement(MARINE, thrown(template));
    let mut world = world_with_flat_terrain();
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let marine = world.create_unit_at(2, POSITION);
    let unit = world.get_unit_mut(marine).unwrap();
    unit.proto_object_name = MARINE.to_owned();
    unit.obstruction_half_extents = Vec3::splat(0.5);
    unit.set_max_hitpoints(10.0);
    let attacker = world.create_unit_at(1, POSITION + ATTACKER_OFFSET);
    (world, gameplay, marine, attacker)
}

/// The template with `ThrownByProjectiles`.
fn thrown(template: &PhysicsTemplate) -> Arc<PhysicsTemplate> {
    let mut info = template.info().clone();
    info.flags
        .set(PhysicsInfoFlags::THROWN_BY_PROJECTILES, true);
    Arc::new(
        PhysicsTemplate::new(template.name(), info, template.bodies().to_vec())
            .expect("the bodies of the template"),
    )
}
