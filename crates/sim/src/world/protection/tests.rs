use super::*;
use crate::entities::SquadState;

fn protected_world() -> (World, EntityId, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    let protected_squad = world.create_squad(1);
    let protected_unit = world.create_unit(1);
    assert!(world.attach_unit_to_squad(protected_unit, protected_squad));
    let proxy_squad = world.create_squad(1);
    let proxy_unit = world.create_unit(1);
    assert!(world.attach_unit_to_squad(proxy_unit, proxy_squad));
    world
        .get_squad_mut(protected_squad)
        .unwrap()
        .set_damage_proxy(proxy_squad);
    (
        world,
        protected_squad,
        protected_unit,
        proxy_squad,
        proxy_unit,
    )
}

#[test]
fn raw_damage_redirects_to_the_proxy_squads_first_child() {
    let (mut world, _, protected_unit, _, proxy_unit) = protected_world();

    assert!(world.damage_unit(protected_unit, 25.0));

    assert!((world.get_unit(protected_unit).unwrap().hitpoints - 100.0).abs() < f32::EPSILON);
    assert!((world.get_unit(proxy_unit).unwrap().hitpoints - 75.0).abs() < f32::EPSILON);
}

#[test]
fn dead_missing_and_empty_proxies_fall_back_to_the_requested_unit() {
    let (mut world, protected_squad, protected_unit, proxy_squad, proxy_unit) = protected_world();
    world.get_squad_mut(proxy_squad).unwrap().state = SquadState::Dead;
    assert_eq!(world.resolve_damage_target(protected_unit), protected_unit);

    world.get_squad_mut(proxy_squad).unwrap().state = SquadState::Idle;
    assert!(world.detach_unit_from_squad(proxy_unit));
    assert_eq!(world.resolve_damage_target(protected_unit), protected_unit);

    let missing = world.create_squad(1);
    assert!(world.remove_squad(missing).is_some());
    world
        .get_squad_mut(protected_squad)
        .unwrap()
        .set_damage_proxy(missing);
    assert_eq!(world.resolve_damage_target(protected_unit), protected_unit);
}

#[test]
fn removing_a_proxy_squad_clears_every_reference_to_it() {
    let (mut world, protected_squad, _, proxy_squad, _) = protected_world();

    assert!(world.remove_squad(proxy_squad).is_some());

    assert_eq!(
        world.get_squad(protected_squad).unwrap().damage_proxy(),
        None
    );
}

#[test]
fn malformed_proxy_cycles_terminate_deterministically() {
    let (mut world, protected_squad, protected_unit, proxy_squad, proxy_unit) = protected_world();
    world
        .get_squad_mut(proxy_squad)
        .unwrap()
        .set_damage_proxy(protected_squad);

    assert_eq!(world.resolve_damage_target(protected_unit), protected_unit);
    assert_eq!(world.resolve_damage_target(proxy_unit), proxy_unit);
}
