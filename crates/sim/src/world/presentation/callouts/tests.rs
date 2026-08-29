use super::*;

#[test]
fn entity_replacement_parent_clearing_and_widget_capacity_match_retail() {
    let mut world = World::new();
    let squad = world.create_squad(1);
    let unit = world.create_unit(1);
    assert!(world.attach_unit_to_squad(unit, squad));

    assert_eq!(world.create_entity_hint_callout(squad, 10, false), 0);
    assert_eq!(world.create_entity_hint_callout(unit, 11, true), 1);
    assert!(world.hint_callout(0).is_none());
    assert_eq!(world.hint_callout(1).unwrap().widget_slot(), 0);
    assert_eq!(world.create_entity_hint_callout(unit, 12, true), 2);
    assert!(world.hint_callout(1).is_none());

    for expected_id in 3..7 {
        assert_eq!(
            world.create_location_hint_callout(Vec3::splat(1.0), expected_id),
            expected_id
        );
    }
    assert_eq!(world.hint_callouts().count(), MAX_HINT_CALLOUTS);
    assert_eq!(world.create_location_hint_callout(Vec3::ZERO, 99), -1);

    assert!(world.remove_hint_callout(4));
    assert_eq!(world.create_location_hint_callout(Vec3::Y, 100), 8);
    assert_eq!(world.hint_callout(8).unwrap().widget_slot(), 2);
}

#[test]
fn invalid_entities_consume_ids_and_live_entity_removal_cleans_up() {
    let mut world = World::new();
    let missing = EntityId::new(EntityClass::Unit, 40);
    assert_eq!(world.create_entity_hint_callout(missing, 1, true), -1);

    let unit = world.create_unit(1);
    assert_eq!(world.create_entity_hint_callout(unit, 2, true), 1);
    let checksum = world.checksum();
    assert!(world.remove_unit(unit).is_some());
    assert!(world.hint_callouts().next().is_none());
    assert_ne!(world.checksum(), checksum);
}
