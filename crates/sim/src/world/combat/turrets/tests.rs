use super::super::test_catalog::{secondary_turret_catalog, secondary_turret_catalog_with_minimum};
use super::*;

#[test]
fn secondary_scans_rotate_one_owned_squad_per_player_update() {
    let (gameplay, mut world) = turret_world();
    let first_squad = world.create_squad_at(1, Vec3::ZERO);
    let second_squad = world.create_squad_at(1, Vec3::new(0.0, 0.0, 1.0));
    let target_squad = world.create_squad_at(2, Vec3::new(5.0, 0.0, 0.0));
    let first = turret_member(&mut world, first_squad, Vec3::ZERO);
    let second = turret_member(&mut world, second_squad, Vec3::new(0.0, 0.0, 1.0));
    let target = world.create_unit_at(2, Vec3::new(5.0, 0.0, 0.0));
    world.get_unit_mut(target).unwrap().proto_object_name = "test_target".to_owned();
    assert!(world.attach_unit_to_squad(target, target_squad));
    world.configure_test_visibility_grid();

    world.update_entities_with_gameplay(0.05, &gameplay);

    assert_eq!(turret_target(&world, first), Some(target));
    assert_eq!(turret_target(&world, second), None);

    world.update_entities_with_gameplay(0.05, &gameplay);

    assert_eq!(turret_target(&world, second), Some(target));
}

#[test]
fn blocked_state_consumes_its_first_token_and_resumes_through_none() {
    let (gameplay, mut world) = turret_world();
    let squad = world.create_squad_at(1, Vec3::ZERO);
    let target_squad = world.create_squad_at(2, Vec3::new(5.0, 0.0, 0.0));
    let attacker = turret_member(&mut world, squad, Vec3::ZERO);
    let target = target_member(&mut world, target_squad);
    world
        .get_unit_mut(attacker)
        .unwrap()
        .garrison
        .set_container(Some(target));
    world.configure_test_visibility_grid();

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_phase(&world, attacker), TurretAttackPhase::Blocked);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_phase(&world, attacker), TurretAttackPhase::Blocked);

    world
        .get_unit_mut(attacker)
        .unwrap()
        .garrison
        .set_container(None);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_phase(&world, attacker), TurretAttackPhase::None);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_target(&world, attacker), Some(target));
}

#[test]
fn gather_order_consumes_a_none_state_scan_token() {
    let (gameplay, mut world) = turret_world();
    let squad = world.create_squad_at(1, Vec3::ZERO);
    let target_squad = world.create_squad_at(2, Vec3::new(5.0, 0.0, 0.0));
    let attacker = turret_member(&mut world, squad, Vec3::ZERO);
    let target = target_member(&mut world, target_squad);
    world.configure_test_visibility_grid();
    world
        .get_unit_mut(attacker)
        .unwrap()
        .turret_combat
        .grant_secondary_scan_token("test");
    world.get_squad_mut(squad).unwrap().begin_gather(target);

    assert!(
        world
            .collect_turret_engagements(attacker, &gameplay)
            .is_empty()
    );
    world.get_squad_mut(squad).unwrap().remove_all_orders();
    assert!(
        world
            .collect_turret_engagements(attacker, &gameplay)
            .is_empty()
    );

    world
        .get_unit_mut(attacker)
        .unwrap()
        .turret_combat
        .grant_secondary_scan_token("test");
    assert!(
        world
            .collect_turret_engagements(attacker, &gameplay)
            .is_empty()
    );
    assert_eq!(turret_target(&world, attacker), Some(target));
    assert_eq!(
        world.collect_turret_engagements(attacker, &gameplay).len(),
        1
    );
}

#[test]
fn secondary_scan_acquires_before_starting_the_attachment_animation() {
    let (gameplay, mut world) = turret_world();
    let squad = world.create_squad_at(1, Vec3::ZERO);
    let target_squad = world.create_squad_at(2, Vec3::new(5.0, 0.0, 0.0));
    let attacker = turret_member(&mut world, squad, Vec3::ZERO);
    let target = target_member(&mut world, target_squad);
    world.configure_test_visibility_grid();

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_target(&world, attacker), Some(target));
    assert_eq!(turret_animation(&world, attacker), None);

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_animation(&world, attacker), Some(0));
    assert!(turret_state(&world, attacker).attack_wait_on());
}

#[test]
fn secondary_scan_uses_the_squad_visible_enemy_list() {
    let (gameplay, mut world) = turret_world();
    let squad = world.create_squad_at(1, Vec3::ZERO);
    let target_squad = world.create_squad_at(2, Vec3::new(5.0, 0.0, 0.0));
    let attacker = turret_member(&mut world, squad, Vec3::ZERO);
    world
        .get_unit_mut(attacker)
        .unwrap()
        .configure_authored_line_of_sight(4.0);
    let target = target_member(&mut world, target_squad);
    world.configure_test_visibility_grid();

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_target(&world, attacker), None);
    world
        .get_unit_mut(attacker)
        .unwrap()
        .configure_authored_line_of_sight(6.0);
    world.update_line_of_sight();
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_target(&world, attacker), Some(target));
}

#[test]
fn secondary_scan_rejects_targets_inside_authored_minimum_range() {
    let gameplay = secondary_turret_catalog_with_minimum(6.0);
    let (_, mut world) = turret_world();
    let squad = world.create_squad_at(1, Vec3::ZERO);
    let target_squad = world.create_squad_at(2, Vec3::new(5.9, 0.0, 0.0));
    let attacker = turret_member(&mut world, squad, Vec3::ZERO);
    let target = target_member(&mut world, target_squad);
    world.get_unit_mut(target).unwrap().base.position = Vec3::X * 5.9;
    world.configure_test_visibility_grid();

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_target(&world, attacker), None);

    world.get_unit_mut(target).unwrap().base.position = Vec3::X * 6.0;
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(turret_target(&world, attacker), Some(target));
}

#[test]
fn active_attack_wait_defers_the_change_mode_hit_check() {
    let (gameplay, mut world) = turret_world();
    let squad = world.create_squad_at(1, Vec3::ZERO);
    let target_squad = world.create_squad_at(2, Vec3::new(5.0, 0.0, 0.0));
    let attacker = turret_member(&mut world, squad, Vec3::ZERO);
    let target = target_member(&mut world, target_squad);
    world.configure_test_visibility_grid();
    world.update_entities_with_gameplay(0.05, &gameplay);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(
        world
            .get_squad_mut(squad)
            .unwrap()
            .begin_change_mode(crate::entities::SquadMode::Lockdown, None)
    );

    world.advance_turret_attacks(0.02, &gameplay);
    assert_eq!(turret_target(&world, attacker), Some(target));
    world.advance_turret_attacks(0.04, &gameplay);
    assert_eq!(turret_target(&world, attacker), Some(target));
    world.advance_turret_attacks(0.01, &gameplay);
    assert_eq!(turret_target(&world, attacker), None);
}

fn turret_world() -> (GameplayCatalog, World) {
    let gameplay = secondary_turret_catalog();
    let mut world = World::with_seed(7);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    (gameplay, world)
}

fn turret_member(world: &mut World, squad_id: EntityId, position: Vec3) -> EntityId {
    let unit_id = world.create_unit_at(1, position);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_name = "test_turret".to_owned();
    unit.configure_authored_line_of_sight(20.0);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    unit_id
}

fn target_member(world: &mut World, squad_id: EntityId) -> EntityId {
    let target = world.create_unit_at(2, Vec3::new(5.0, 0.0, 0.0));
    world.get_unit_mut(target).unwrap().proto_object_name = "test_target".to_owned();
    assert!(world.attach_unit_to_squad(target, squad_id));
    target
}

fn turret_target(world: &World, unit_id: EntityId) -> Option<EntityId> {
    turret_state(world, unit_id).target()
}

fn turret_phase(world: &World, unit_id: EntityId) -> TurretAttackPhase {
    turret_state(world, unit_id).phase()
}

fn turret_animation(world: &World, unit_id: EntityId) -> Option<usize> {
    turret_state(world, unit_id).animation_index()
}

fn turret_state(world: &World, unit_id: EntityId) -> &crate::entities::TurretAttackState {
    world
        .get_unit(unit_id)
        .unwrap()
        .turret_combat
        .get("TurretAttack")
        .unwrap()
}
