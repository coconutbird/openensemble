use super::*;
use crate::entities::SquadState;

#[test]
fn temporary_mobility_disable_pauses_and_resumes_the_existing_order() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .move_to(Vec3::new(20.0, 0.0, 0.0));

    assert!(world.set_squad_mobile(squad_id, false, true));
    world.update_entities(0.5);
    let paused = world.get_squad(squad_id).unwrap();
    assert_eq!(paused.state, SquadState::Moving);
    assert_eq!(paused.move_target, Some(Vec3::new(20.0, 0.0, 0.0)));
    assert_eq!(paused.base.position, Vec3::ZERO);

    assert!(world.set_squad_mobile(squad_id, true, true));
    world.update_entities(0.5);
    assert!(world.get_squad(squad_id).unwrap().base.position.x > 0.0);
}

#[test]
fn persistent_mobility_disable_removes_orders_and_stops_physical_members() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .move_to(Vec3::new(20.0, 0.0, 0.0));

    assert!(world.set_squad_mobile(squad_id, false, false));
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.state, SquadState::Idle);
    assert_eq!(squad.move_target, None);
    assert_eq!(squad.base.velocity, Vec3::ZERO);
}

#[test]
fn prototype_non_mobile_squads_reject_runtime_mobility_changes() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .base
        .configure_prototype_mobility(true);

    assert!(!world.set_squad_mobile(squad_id, true, false));
    assert_eq!(world.squad_is_mobile(squad_id), Some(false));
}

#[test]
fn selectable_and_auto_attackable_flags_are_authoritative_queries() {
    let mut world = World::new();
    let unit_id = world.create_unit(1);
    let squad_id = world.create_squad(1);

    assert!(world.set_entity_selectable(unit_id, false));
    assert!(world.set_entity_selectable(squad_id, false));
    assert_eq!(world.entity_is_selectable(unit_id), Some(false));
    assert_eq!(world.entity_is_selectable(squad_id), Some(false));

    assert!(world.set_unit_auto_attackable(unit_id, false));
    assert_eq!(world.unit_is_auto_attackable(unit_id), Some(false));
}

#[test]
fn runtime_render_override_applies_only_to_object_derived_entities() {
    let mut world = World::new();
    let unit_id = world.create_unit(1);
    let squad_id = world.create_squad(1);

    assert_eq!(world.entity_is_render_enabled(unit_id), Some(true));
    assert!(world.set_entity_render_enabled(unit_id, false));
    assert_eq!(world.entity_is_render_enabled(unit_id), Some(false));
    assert!(!world.set_entity_render_enabled(squad_id, false));
    assert_eq!(world.entity_is_render_enabled(squad_id), None);
}
