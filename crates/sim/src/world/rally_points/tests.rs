use super::*;

#[test]
fn entity_targets_follow_live_positions_and_retain_their_fallback() {
    let mut world = World::new();
    world.init_players(1);
    let target = world.create_unit_at(1, Vec3::new(10.0, 0.0, 20.0));

    assert!(world.set_player_rally_point(1, Vec3::ONE, Some(target)));
    let rally_point = world.player_rally_point(1).unwrap();
    assert_eq!(rally_point.position(), Vec3::new(10.0, 0.0, 20.0));
    assert_eq!(rally_point.target_entity_id(), Some(target));

    world.get_unit_mut(target).unwrap().base.position = Vec3::new(30.0, 0.0, 40.0);
    assert_eq!(
        world.resolve_rally_point(rally_point),
        Vec3::new(30.0, 0.0, 40.0)
    );
    let _removed = world.remove_unit(target);
    assert_eq!(
        world.resolve_rally_point(rally_point),
        Vec3::new(10.0, 0.0, 20.0)
    );
}

#[test]
fn primary_and_coop_unit_rally_points_are_independent() {
    let mut world = World::new();
    world.init_players(2);
    let building = world.create_building(1);
    let primary = Vec3::new(10.0, 0.0, 0.0);
    let coop = Vec3::new(20.0, 0.0, 0.0);

    assert!(world.set_unit_rally_point(building, 1, primary, None));
    assert!(world.set_unit_rally_point(building, 2, coop, None));
    assert_eq!(
        world
            .unit_rally_point(building, 1)
            .map(RallyPoint::position),
        Some(primary)
    );
    assert_eq!(
        world
            .unit_rally_point(building, 2)
            .map(RallyPoint::position),
        Some(coop)
    );

    assert!(world.clear_unit_rally_point(building, 1));
    assert!(world.unit_rally_point(building, 1).is_none());
    assert!(world.unit_rally_point(building, 2).is_some());
}

#[test]
fn player_and_base_rally_points_apply_retail_mutual_exclusion() {
    let mut world = World::new();
    world.init_players(1);
    let first_base = world.create_base(1, Vec3::ZERO);
    let second_base = world.create_base(1, Vec3::new(20.0, 0.0, 0.0));
    let first_anchor = world.get_base(first_base).unwrap().anchor_building_id;
    let second_anchor = world.get_base(second_base).unwrap().anchor_building_id;

    assert!(world.set_player_rally_point(1, Vec3::new(50.0, 0.0, 0.0), None));
    assert!(world.set_unit_rally_point(first_anchor, 1, Vec3::new(5.0, 0.0, 0.0), None));
    assert!(world.player_rally_point(1).is_some());
    assert!(world.set_unit_rally_point(second_anchor, 1, Vec3::new(25.0, 0.0, 0.0), None));
    assert!(world.player_rally_point(1).is_none());

    assert!(world.set_player_rally_point(1, Vec3::new(60.0, 0.0, 0.0), None));
    assert!(world.unit_rally_point(first_anchor, 1).is_none());
    assert!(world.unit_rally_point(second_anchor, 1).is_none());
}

#[test]
fn rally_state_changes_the_deterministic_checksum() {
    let mut first = World::new();
    first.init_players(1);
    let mut second = World::new();
    second.init_players(1);
    assert_eq!(first.checksum(), second.checksum());

    assert!(second.set_player_rally_point(1, Vec3::new(1.0, 2.0, 3.0), None));
    assert_ne!(first.checksum(), second.checksum());
}
