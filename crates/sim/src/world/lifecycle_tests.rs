use super::World;

#[test]
fn removing_an_anchor_dissolves_base_membership() {
    let mut world = World::new();
    let anchor_id = world.create_building(1);
    let base_id = world.register_base(anchor_id).unwrap();
    let second_id = world.create_building(1);
    assert!(world.add_building_to_base(base_id, second_id));

    let removed = world.remove_unit(anchor_id);

    assert!(removed.is_some());
    assert!(world.get_base(base_id).is_none());
    assert_eq!(world.get_building(second_id).unwrap().base_id, None);
}

#[test]
fn dead_units_are_removed_and_stale_ids_fail() {
    let mut world = World::new();
    let old_id = world.create_unit(1);
    world.get_unit_mut(old_id).unwrap().kill();
    world.update_entities(0.05);
    let replacement_id = world.create_unit(1);

    assert!(world.get_unit(old_id).is_none());
    assert_eq!(old_id.pool_index(), replacement_id.pool_index());
    assert_ne!(old_id.generation(), replacement_id.generation());
}
