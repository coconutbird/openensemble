use super::*;
use crate::entities::projectiles::ProjectileLaunch;
use crate::entity_id::EntityClass;
use crate::gameplay::projectiles::ProjectileBehavior;
use crate::gameplay::{
    AreaDamageProfile, AttackAccuracyProfile, AttackAnimation, AttackProfile, ImpactEffectProfile,
    ImpactEffectSize, ProjectilePerturbanceProfile, ProjectileProfile,
};
use byteorder::{BigEndian, ByteOrder};
use half::f16;
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::{Database, ProtoObject};

fn collision_catalog(
    projectile_obstructable: bool,
    targets_foot: bool,
    external_shield: bool,
    area_damage: Option<AreaDamageProfile>,
) -> GameplayCatalog {
    let blocker_flags = [
        projectile_obstructable.then_some("ProjectileObstructable".to_owned()),
        external_shield.then_some("ExternalShield".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let target_flags = targets_foot
        .then_some(vec!["TargetsFootOfUnit".to_owned()])
        .unwrap_or_default();
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "attacker".to_owned(),
            tactics: Some("attacker.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "target".to_owned(),
            flags: target_flags,
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "blocker".to_owned(),
            flags: blocker_flags,
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "bullet".to_owned(),
            dbid: Some(70),
            object_class: Some("Projectile".to_owned()),
            velocity: Some(20.0),
            lifespan: Some(5.0),
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        actions: vec![Action {
            name: "Attack".to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some("Weapon".to_owned()),
            ..Action::default()
        }],
        weapons: vec![Weapon {
            name: "Weapon".to_owned(),
            max_range: Some(20.0),
            targets_foot_of_unit: targets_foot.then_some(true),
            ..Weapon::default()
        }],
        ..TacticData::default()
    };
    let profile = AttackProfile {
        action_name: "Attack".to_owned(),
        animation_type: "Attack".to_owned(),
        weapon_name: "Weapon".to_owned(),
        weapon_type: None,
        projectile: Some("bullet".to_owned()),
        impact_effect: None,
        area_damage,
        pull: None,
        hardpoint: None,
        orientation: crate::gameplay::AttackOrientationProfile::default(),
        charged_animation: None,
        friendly_fire: false,
        targets_foot_of_unit: targets_foot,
        projectile_reactions: crate::gameplay::ProjectileReactionFlags::default(),
        max_range: 20.0,
        max_velocity_lead: 0.0,
        accuracy: AttackAccuracyProfile::default(),
        damage_per_attack: 5.0,
        ammunition: crate::gameplay::AttackAmmunition::None,
        animations: vec![AttackAnimation {
            asset_path: "attack.uax".to_owned(),
            weight: 1,
            duration: 100.0,
            attack_positions: vec![0.0],
            events: Vec::new(),
            hardpoint_track: None,
        }],
        pre_attack_cooldown: [0.0, 0.0],
        post_attack_cooldown: [0.0, 0.0],
        reload_duration: 0.0,
        visual_ammo: 0,
        uses_height_bonus_damage: false,
    };
    GameplayCatalog::from_test_profiles(
        &database,
        [("attacker".to_owned(), tactics)],
        [("attacker".to_owned(), profile)],
    )
}

fn combat_world(blocker_player: PlayerId) -> (World, EntityId, EntityId, EntityId) {
    let mut world = World::with_seed(81);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let attacker_id = world.create_unit_at(1, Vec3::ZERO);
    let blocker_id = world.create_unit_at(blocker_player, Vec3::X * 5.0);
    let target_id = world.create_unit_at(2, Vec3::X * 10.0);
    configure_unit(&mut world, attacker_id, "attacker");
    configure_unit(&mut world, blocker_id, "blocker");
    configure_unit(&mut world, target_id, "target");
    assert!(world.issue_attack_order(1, attacker_id, target_id, 0.0));
    (world, attacker_id, blocker_id, target_id)
}

fn configure_unit(world: &mut World, unit_id: EntityId, proto_name: &str) {
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_name = proto_name.to_owned();
    unit.obstruction_half_extents = Vec3::splat(0.5);
}

fn resolve_first_shot(world: &mut World, gameplay: &GameplayCatalog) {
    world.update_entities_with_gameplay(0.05, gameplay);
    assert_eq!(world.projectiles.len(), 1);
    for _ in 0..20 {
        world.update_entities_with_gameplay(0.05, gameplay);
        if world.projectiles.is_empty() {
            return;
        }
    }
    panic!("projectile should collide before exhausting its lifespan");
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn projectile_obstruction_redirects_damage_to_the_first_interceptor() {
    let gameplay = collision_catalog(true, false, false, None);
    let (mut world, _, blocker_id, target_id) = combat_world(2);

    resolve_first_shot(&mut world, &gameplay);

    assert_close(world.get_unit(blocker_id).unwrap().hitpoints, 95.0);
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 100.0);
}

#[test]
fn source_visual_player_does_not_replace_projectile_damage_attribution() {
    let gameplay = collision_catalog(false, false, false, None);
    let (mut world, _, _, target_id) = combat_world(2);
    world.update_entities_with_gameplay(0.05, &gameplay);
    let projectile_id = world.projectiles.iter().next().unwrap().0;
    world
        .get_projectile_mut(projectile_id)
        .unwrap()
        .inherit_source_visual("captured_enemy", None, 2, Vec3::ZERO);
    assert_eq!(world.entity_owner(projectile_id), Some(2));
    assert_eq!(
        world
            .get_projectile(projectile_id)
            .unwrap()
            .created_by_player_id(),
        1
    );

    for _ in 0..20 {
        world.update_entities_with_gameplay(0.05, &gameplay);
        if world.get_projectile(projectile_id).is_none() {
            break;
        }
    }

    assert!(world.get_projectile(projectile_id).is_none());
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 95.0);
}

#[test]
fn external_shield_intercepts_without_projectile_obstructable() {
    let gameplay = collision_catalog(false, false, true, None);
    let (mut world, _, blocker_id, target_id) = combat_world(2);
    let blocker = world.get_unit_mut(blocker_id).unwrap();
    blocker.set_external_shield(true);
    blocker.obstruction_half_extents = Vec3::splat(2.0);

    resolve_first_shot(&mut world, &gameplay);

    assert_close(world.get_unit(blocker_id).unwrap().hitpoints, 95.0);
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 100.0);
}

#[test]
fn nonwall_external_shield_lets_projectiles_launched_inside_escape() {
    let gameplay = collision_catalog(false, false, true, None);
    let (mut world, attacker_id, blocker_id, target_id) = combat_world(2);
    world.get_unit_mut(attacker_id).unwrap().base.position = Vec3::X * 5.0;
    let blocker = world.get_unit_mut(blocker_id).unwrap();
    blocker.set_external_shield(true);
    blocker.obstruction_half_extents = Vec3::splat(2.0);

    resolve_first_shot(&mut world, &gameplay);

    assert_close(world.get_unit(blocker_id).unwrap().hitpoints, 100.0);
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 95.0);
}

#[test]
fn wall_external_shield_still_catches_projectiles_launched_inside() {
    let gameplay = collision_catalog(false, false, true, None);
    let (mut world, attacker_id, blocker_id, target_id) = combat_world(2);
    world.get_unit_mut(attacker_id).unwrap().base.position = Vec3::X * 5.0;
    let blocker = world.get_unit_mut(blocker_id).unwrap();
    blocker.set_external_shield(true);
    blocker.obstruction_half_extents = Vec3::splat(2.0);
    blocker.object_types.push("_WallShield".to_owned());

    resolve_first_shot(&mut world, &gameplay);

    assert_close(world.get_unit(blocker_id).unwrap().hitpoints, 95.0);
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 100.0);
}

#[test]
fn non_obstructable_friendly_unit_is_ignored_away_from_the_target() {
    let gameplay = collision_catalog(false, false, false, None);
    let (mut world, _, blocker_id, target_id) = combat_world(1);

    resolve_first_shot(&mut world, &gameplay);

    assert_close(world.get_unit(blocker_id).unwrap().hitpoints, 100.0);
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 95.0);
}

#[test]
fn targets_foot_launch_uses_ground_impact_as_the_full_splash_pool() {
    let area_damage = AreaDamageProfile {
        radius: 2.0,
        primary_target_factor: 0.0,
        distance_factor: 0.0,
        damage_factor: 0.0,
        linear_damage: false,
        ignores_y_axis: false,
        friendly_fire: false,
    };
    let gameplay = collision_catalog(false, true, false, Some(area_damage));
    let (mut world, _, blocker_id, target_id) = combat_world(2);
    world.get_unit_mut(blocker_id).unwrap().base.position = Vec3::new(10.75, 0.0, 0.0);
    world.get_unit_mut(blocker_id).unwrap().proto_object_name = "target".to_owned();

    resolve_first_shot(&mut world, &gameplay);

    assert!(world.get_unit(target_id).unwrap().hitpoints < 100.0);
    assert!(world.get_unit(blocker_id).unwrap().hitpoints < 100.0);
}

#[test]
fn impact_request_retains_proto_surface_and_intended_target_flying_rule() {
    let mut world = World::new();
    world.init_players(2);
    let target_id = world.create_unit_at(2, Vec3::X);
    world.get_unit_mut(target_id).unwrap().proto_object_name = "metal_target".to_owned();
    let database = Database {
        objects: vec![ProtoObject {
            name: "metal_target".to_owned(),
            surface_type: Some("Metal".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let effect = ImpactEffectProfile {
        name: "Impact".to_owned(),
        size: ImpactEffectSize::Large,
        do_shockwave_action: false,
    };
    let impact = ProjectileImpact {
        projectile_id: EntityId::new(EntityClass::Projectile, 9),
        owning_power_execution_id: None,
        source_id: EntityId::INVALID,
        source_player_id: 1,
        intended_target_id: target_id,
        primary_target_id: Some(target_id),
        position: Vec3::X,
        direction: Vec3::X,
        damage: 0.0,
        weapon_type: None,
        area_damage: None,
        impact_effect: Some(effect.clone()),
    };

    world.apply_projectile_impact(impact.clone(), Some(&database), None);
    let request = world.impact_effect_requests_after(0).next().unwrap();
    assert_eq!(request.effect(), &effect);
    assert_eq!(
        request.surface(),
        Some(&crate::ImpactSurface::Object("Metal".to_owned()))
    );
    assert_eq!(request.forward(), Vec3::Z);
    assert!(request.emits_surface_effect());

    world.get_unit_mut(target_id).unwrap().flying = true;
    world.apply_projectile_impact(impact, Some(&database), None);
    let request = world.impact_effect_requests_after(1).next().unwrap();
    assert!(!request.emits_surface_effect());
}

#[test]
fn shield_impact_forward_matches_retail_special_cases() {
    let mut world = World::new();
    let unit_id = world.create_unit_at(1, Vec3::ZERO);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.base.set_forward(Vec3::X);
    unit.object_types.push("_WallShield".to_owned());
    assert_eq!(impact_effect_forward(Some(unit), Vec3::Z), Vec3::X);

    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.object_types.clear();
    unit.object_types.push("_BaseShield".to_owned());
    assert_eq!(impact_effect_forward(Some(unit), Vec3::Z), Vec3::X);
}

#[test]
fn terrain_collision_offsets_ground_zero_above_the_triangle_surface() {
    let mut world = World::new();
    world.configure_terrain_simulation(&flat_xsd()).unwrap();
    let projectile_id = world.projectiles.allocate_id();
    let source_id = EntityId::new(EntityClass::Unit, 0);
    let target_id = EntityId::new(EntityClass::Unit, 1);
    let profile = ProjectileProfile {
        proto_object_id: 70,
        proto_object_name: "tumbling_round".to_owned(),
        speed: 4.0,
        starting_speed: 4.0,
        fuel: 0.0,
        acceleration: 0.0,
        max_projectile_height: 0.0,
        lifespan: 5.0,
        tracking_delay: 0.0,
        turn_rate_degrees: 0.0,
        perturbance: ProjectilePerturbanceProfile::default(),
        behavior: ProjectileBehavior::TUMBLING,
    };
    let projectile = Projectile::new(
        projectile_id,
        1,
        ProjectileLaunch {
            source_id,
            target_id,
            source_position: Vec3::new(1.0, 1.0, 1.0),
            target_position: Vec3::new(3.0, -1.0, 1.0),
            target_entity_position: Vec3::new(3.0, -1.0, 1.0),
            target_offset: Vec3::ZERO,
            target_radius: 0.1,
            max_range: 20.0,
            damage: 5.0,
            weapon_type: None,
            area_damage: None,
            impact_effect: None,
            friendly_fire: false,
            collides_with_all_units: true,
        },
        &profile,
    );
    world.projectiles.insert(projectile_id, projectile);
    assert_eq!(
        world
            .projectiles
            .get_mut(projectile_id)
            .unwrap()
            .advance(0.05, None, 0.0),
        ProjectileStep::Flying
    );
    let previous = world.projectiles.get(projectile_id).unwrap().base.position;
    let step = world
        .projectiles
        .get_mut(projectile_id)
        .unwrap()
        .advance(0.5, None, 0.0);

    let collision = world
        .resolve_projectile_collision(projectile_id, previous, step, None)
        .expect("tumbling projectile should strike flat terrain");
    assert!(collision.primary_target_id.is_none());
    assert!((collision.position.x - 2.0).abs() < 0.000_1);
    assert!((collision.position.y - 0.25).abs() < 0.000_1);
}

#[test]
fn world_routes_xsd_height_into_tracking_ground_avoidance() {
    let mut world = World::new();
    world.configure_terrain_simulation(&flat_xsd()).unwrap();
    let projectile_id = world.projectiles.allocate_id();
    let profile = ProjectileProfile {
        proto_object_id: 71,
        proto_object_name: "tracking_round".to_owned(),
        speed: 10.0,
        starting_speed: 10.0,
        fuel: 2.0,
        acceleration: 0.0,
        max_projectile_height: 0.0,
        lifespan: 5.0,
        tracking_delay: 0.0,
        turn_rate_degrees: 180.0,
        perturbance: ProjectilePerturbanceProfile::default(),
        behavior: ProjectileBehavior::TRACKING,
    };
    let projectile = Projectile::new(
        projectile_id,
        1,
        ProjectileLaunch {
            source_id: EntityId::INVALID,
            target_id: EntityId::INVALID,
            source_position: Vec3::new(1.0, 2.0, 1.0),
            target_position: Vec3::new(7.0, 0.0, 1.0),
            target_entity_position: Vec3::new(7.0, 0.0, 1.0),
            target_offset: Vec3::ZERO,
            target_radius: 0.1,
            max_range: 10.0,
            damage: 0.0,
            weapon_type: None,
            area_damage: None,
            impact_effect: None,
            friendly_fire: false,
            collides_with_all_units: true,
        },
        &profile,
    );
    world.projectiles.insert(projectile_id, projectile);
    world.initialize_projectile_follow_ground_height(projectile_id);
    let projectile = world.projectiles.get_mut(projectile_id).unwrap();
    projectile.tracking = true;
    projectile.base.position = Vec3::new(2.0, 1.0, 1.0);

    world.apply_projectile_tracking_ground_avoidance(projectile_id, false);
    assert!((world.get_projectile(projectile_id).unwrap().base.position.y - 2.0).abs() < 0.000_1);
}

#[test]
fn sticky_timer_follows_the_unit_and_defers_damage_until_detonation() {
    let behavior = ProjectileBehavior::STICKY.union(ProjectileBehavior::EXPLODE_ON_TIMER);
    let (mut world, projectile_id, target_id) = timed_sticky_world(behavior);
    let collision = ProjectileCollision {
        position: world.get_unit(target_id).unwrap().base.position,
        primary_target_id: Some(target_id),
    };

    let outcome =
        world.handle_projectile_collision(projectile_id, collision, ProjectileStep::Flying);
    assert!(matches!(
        outcome,
        ProjectileCollisionOutcome::Retained(None)
    ));
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 100.0);
    world.get_unit_mut(target_id).unwrap().base.position.x += 1.0;
    world.update_projectiles(0.05, None, None);
    assert_close(
        world.get_projectile(projectile_id).unwrap().base.position.x,
        world.get_unit(target_id).unwrap().base.position.x,
    );

    world.update_projectiles(0.1, None, None);
    assert!(world.get_projectile(projectile_id).is_none());
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 95.0);
}

#[test]
fn expire_timer_deals_unit_impact_once_then_disappears_without_final_damage() {
    let behavior = ProjectileBehavior::STICKY.union(ProjectileBehavior::EXPIRE_ON_TIMER);
    let (mut world, projectile_id, target_id) = timed_sticky_world(behavior);
    let collision = ProjectileCollision {
        position: world.get_unit(target_id).unwrap().base.position,
        primary_target_id: Some(target_id),
    };

    let ProjectileCollisionOutcome::Retained(Some(impact)) =
        world.handle_projectile_collision(projectile_id, collision, ProjectileStep::Flying)
    else {
        panic!("expire-on-timer unit impact should apply once and retain the projectile");
    };
    world.apply_projectile_impact(impact, None, None);
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 95.0);

    world.update_projectiles(0.15, None, None);
    assert!(world.get_projectile(projectile_id).is_none());
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 95.0);
}

fn timed_sticky_world(behavior: ProjectileBehavior) -> (World, EntityId, EntityId) {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let target_id = world.create_unit_at(2, Vec3::X * 3.0);
    let projectile_id = world.projectiles.allocate_id();
    let profile = ProjectileProfile {
        proto_object_id: 72,
        proto_object_name: "timed_sticky".to_owned(),
        speed: 10.0,
        starting_speed: 10.0,
        fuel: 0.0,
        acceleration: 0.0,
        max_projectile_height: 0.0,
        lifespan: 0.2,
        tracking_delay: 0.0,
        turn_rate_degrees: 0.0,
        perturbance: ProjectilePerturbanceProfile::default(),
        behavior,
    };
    let mut projectile = Projectile::new(
        projectile_id,
        1,
        ProjectileLaunch {
            source_id: EntityId::INVALID,
            target_id,
            source_position: Vec3::ZERO,
            target_position: Vec3::X * 3.0,
            target_entity_position: Vec3::X * 3.0,
            target_offset: Vec3::ZERO,
            target_radius: 0.5,
            max_range: 10.0,
            damage: 5.0,
            weapon_type: None,
            area_damage: None,
            impact_effect: None,
            friendly_fire: false,
            collides_with_all_units: true,
        },
        &profile,
    );
    assert_eq!(projectile.advance(0.05, None, 0.0), ProjectileStep::Flying);
    world.projectiles.insert(projectile_id, projectile);
    (world, projectile_id, target_id)
}

#[test]
fn segment_box_intersection_returns_the_entry_fraction() {
    let fraction =
        segment_aabb_entry_fraction(Vec3::ZERO, Vec3::X * 10.0, Vec3::X * 5.0, Vec3::ONE).unwrap();
    assert!((fraction - 0.4).abs() < 0.000_1);
    let position = penetrated_impact_position(Vec3::ZERO, Vec3::X * 10.0, fraction);
    assert!((position.x - 4.01).abs() < 0.000_1);
}

#[test]
fn parallel_segment_outside_box_does_not_intersect() {
    assert!(
        segment_aabb_entry_fraction(
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::new(10.0, 2.0, 0.0),
            Vec3::X * 5.0,
            Vec3::ONE,
        )
        .is_none()
    );
}

fn flat_xsd() -> Vec<u8> {
    let mut header = vec![0_u8; 32];
    BigEndian::write_i32(&mut header[0..4], 4);
    BigEndian::write_i32(&mut header[4..8], 8);
    BigEndian::write_f32(&mut header[8..12], 1.0);
    BigEndian::write_f32(&mut header[12..16], 1.0);
    BigEndian::write_i32(&mut header[16..20], 8);
    BigEndian::write_i32(&mut header[20..24], 8);
    BigEndian::write_f32(&mut header[24..28], 1.0);
    BigEndian::write_i32(&mut header[28..32], 1);
    let mut heights = vec![0_u8; 8 * 8 * 2];
    for sample in heights.as_chunks_mut::<2>().0 {
        BigEndian::write_u16(sample, f16::from_f32(0.0).to_bits());
    }
    let mut writer = ecf::Writer::new(0);
    writer.add_chunk(0x1111, header);
    writer.add_chunk(0x2222, heights);
    writer.finalize().unwrap()
}
