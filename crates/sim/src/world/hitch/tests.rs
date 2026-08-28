use super::*;

fn squad_with_member(world: &mut World, player_id: PlayerId) -> (EntityId, EntityId) {
    let squad_id = world.create_squad(player_id);
    let unit_id = world.create_unit(player_id);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

#[test]
fn hitch_resolves_member_target_and_unhitches_both_sides() {
    let mut world = World::new();
    world.init_players(1);
    let (towing_id, _) = squad_with_member(&mut world, 1);
    let (trailer_id, trailer_unit_id) = squad_with_member(&mut world, 1);

    assert_eq!(
        world.issue_hitch_order(1, towing_id, trailer_unit_id),
        Ok(trailer_id)
    );
    assert_eq!(
        world.get_squad(towing_id).unwrap().hitched_squad(),
        Some(trailer_id)
    );
    assert_eq!(
        world.get_squad(trailer_id).unwrap().hitched_to_squad(),
        Some(towing_id)
    );

    assert_eq!(
        world.issue_unhitch_order(1, towing_id, trailer_unit_id),
        Ok(trailer_id)
    );
    assert_eq!(world.get_squad(towing_id).unwrap().hitched_squad(), None);
    assert!(!world.get_squad(trailer_id).unwrap().is_hitched());
}

#[test]
fn hitch_rejects_wrong_owner_and_linked_squads() {
    let mut world = World::new();
    world.init_players(2);
    let (towing_id, _) = squad_with_member(&mut world, 1);
    let (trailer_id, _) = squad_with_member(&mut world, 2);
    let (other_trailer_id, _) = squad_with_member(&mut world, 1);

    assert_eq!(
        world.issue_hitch_order(2, towing_id, trailer_id),
        Err(HitchError::NotOwned)
    );
    assert!(world.issue_hitch_order(1, towing_id, trailer_id).is_ok());
    assert_eq!(
        world.issue_hitch_order(1, towing_id, other_trailer_id),
        Err(HitchError::AlreadyHitched)
    );
}

#[test]
fn removing_either_side_clears_the_inverse_relation() {
    let mut world = World::new();
    world.init_players(1);
    let (towing_id, _) = squad_with_member(&mut world, 1);
    let (trailer_id, _) = squad_with_member(&mut world, 1);
    world.issue_hitch_order(1, towing_id, trailer_id).unwrap();

    world.remove_squad(trailer_id).unwrap();

    assert_eq!(world.get_squad(towing_id).unwrap().hitched_squad(), None);
}

#[test]
fn hitch_relationship_contributes_to_world_checksum() {
    let mut first = World::new();
    first.init_players(1);
    let (towing_id, _) = squad_with_member(&mut first, 1);
    let (trailer_id, _) = squad_with_member(&mut first, 1);
    let mut second = World::new();
    second.init_players(1);
    let (second_towing, _) = squad_with_member(&mut second, 1);
    let (second_trailer, _) = squad_with_member(&mut second, 1);
    assert_eq!((towing_id, trailer_id), (second_towing, second_trailer));
    assert_eq!(first.checksum(), second.checksum());

    first.issue_hitch_order(1, towing_id, trailer_id).unwrap();

    assert_ne!(first.checksum(), second.checksum());
}
