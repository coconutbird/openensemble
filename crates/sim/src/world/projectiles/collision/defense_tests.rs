use super::*;
use crate::entities::projectiles::ProjectileLaunch;
use crate::entity_id::EntityClass;
use crate::gameplay::ProjectileReactionFlags;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn deflect_runs_before_dodge_and_redirects_the_live_projectile() {
    let actions = vec![dodge_action(), deflect_action(false)];
    let reactions = ProjectileReactionFlags::new(true, true, false);
    let (gameplay, mut world, projectile_id, target_id) = defense_world(actions, reactions);
    let original_position = world.get_unit(target_id).unwrap().base.position;

    advance_until_reaction(&mut world, &gameplay, projectile_id, target_id);

    let projectile = world
        .get_projectile(projectile_id)
        .expect("deflected projectile should remain live");
    assert!(projectile.base.velocity.x < 0.0);
    assert_eq!(
        world.get_unit(target_id).unwrap().base.position,
        original_position
    );
    assert_close(world.get_unit(target_id).unwrap().hitpoints, 100.0);
}

#[test]
fn dodge_moves_the_member_and_lets_the_projectile_continue_past() {
    let reactions = ProjectileReactionFlags::new(true, false, false);
    let (gameplay, mut world, projectile_id, target_id) =
        defense_world(vec![dodge_action()], reactions);
    let original_position = world.get_unit(target_id).unwrap().base.position;

    advance_until_reaction(&mut world, &gameplay, projectile_id, target_id);

    let target = world.get_unit(target_id).unwrap();
    assert_ne!(target.base.position, original_position);
    assert_close(target.hitpoints, 100.0);
    let projectile = world
        .get_projectile(projectile_id)
        .expect("dodged projectile should continue its flight");
    assert!(projectile.base.alive);
    assert!(projectile.base.velocity.x > 0.0);
}

#[test]
fn small_arms_deflect_requires_the_matching_weapon_permission() {
    let action = deflect_action(true);
    let normal = ProjectileReactionFlags::new(false, true, false);
    let (normal_gameplay, mut normal_world, normal_projectile, normal_target) =
        defense_world(vec![action.clone()], normal);
    advance_until_finished(&mut normal_world, &normal_gameplay, normal_projectile);
    assert_close(
        normal_world.get_unit(normal_target).unwrap().hitpoints,
        95.0,
    );

    let small_arms = ProjectileReactionFlags::new(false, false, true);
    let (small_gameplay, mut small_world, small_projectile, small_target) =
        defense_world(vec![action], small_arms);
    advance_until_reaction(
        &mut small_world,
        &small_gameplay,
        small_projectile,
        small_target,
    );
    assert!(
        small_world
            .get_projectile(small_projectile)
            .unwrap()
            .base
            .velocity
            .x
            < 0.0
    );
    assert_close(small_world.get_unit(small_target).unwrap().hitpoints, 100.0);
}

fn defense_world(
    actions: Vec<Action>,
    reactions: ProjectileReactionFlags,
) -> (GameplayCatalog, World, EntityId, EntityId) {
    let persistent_actions = actions.iter().map(|action| action.name.clone()).collect();
    let tactics = TacticData {
        actions,
        tactic: Some(TacticRules {
            persistent_actions,
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "target".to_owned(),
            tactics: Some("target.tactics".to_owned()),
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
    let gameplay = GameplayCatalog::from_tactics(&database, [("target".to_owned(), tactics)]);
    let mut world = World::with_seed(81);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let target_position = Vec3::X * 10.0;
    let squad_id = world.create_squad_at(2, target_position);
    let target_id = world.create_unit_at(2, target_position);
    let target = world.get_unit_mut(target_id).unwrap();
    target.proto_object_name = "target".to_owned();
    target.obstruction_half_extents = Vec3::splat(0.5);
    target.base.set_forward(-Vec3::X);
    assert!(world.attach_unit_to_squad(target_id, squad_id));

    let projectile_id = world.projectiles.allocate_id();
    let profile = gameplay.projectile("bullet").unwrap();
    let mut projectile = Projectile::new(
        projectile_id,
        1,
        ProjectileLaunch {
            source_id: EntityId::new(EntityClass::Unit, 99),
            target_id,
            source_position: Vec3::ZERO,
            target_position,
            target_entity_position: target_position,
            target_offset: Vec3::ZERO,
            target_radius: 0.5,
            max_range: 20.0,
            damage: 5.0,
            weapon_type: None,
            area_damage: None,
            impact_effect: None,
            friendly_fire: false,
            collides_with_all_units: true,
        },
        profile,
    );
    projectile.configure_reactions(reactions);
    world.projectiles.insert(projectile_id, projectile);
    (gameplay, world, projectile_id, target_id)
}

fn dodge_action() -> Action {
    Action {
        name: "Dodge".to_owned(),
        action_type: Some("Dodge".to_owned()),
        dodge_chance_max: Some(1.0),
        dodge_chance_min: Some(1.0),
        dodge_max_angle: Some(180.0),
        dodge_cooldown: Some(1.0),
        ..Action::default()
    }
}

fn deflect_action(small_arms: bool) -> Action {
    Action {
        name: "Deflect".to_owned(),
        action_type: Some("Deflect".to_owned()),
        deflect_chance_max: Some(1.0),
        deflect_chance_min: Some(1.0),
        deflect_max_angle: Some(180.0),
        deflect_cooldown: Some(1.0),
        small_arms: small_arms.then_some(true),
        ..Action::default()
    }
}

fn advance_until_reaction(
    world: &mut World,
    gameplay: &GameplayCatalog,
    projectile_id: EntityId,
    target_id: EntityId,
) {
    let original_position = world.get_unit(target_id).unwrap().base.position;
    for _ in 0..20 {
        world.update_projectiles(0.05, None, Some(gameplay));
        let redirected = world
            .get_projectile(projectile_id)
            .is_some_and(|projectile| projectile.base.velocity.x < 0.0);
        let moved = world
            .get_unit(target_id)
            .is_some_and(|unit| unit.base.position != original_position);
        if redirected || moved {
            return;
        }
    }
    panic!("projectile should trigger a persistent defense action");
}

fn advance_until_finished(world: &mut World, gameplay: &GameplayCatalog, projectile_id: EntityId) {
    for _ in 0..20 {
        world.update_projectiles(0.05, None, Some(gameplay));
        if world.get_projectile(projectile_id).is_none() {
            return;
        }
    }
    panic!("undefended projectile should finish at its target");
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.000_1);
}
