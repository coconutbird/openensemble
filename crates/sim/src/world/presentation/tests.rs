use super::*;

#[test]
fn retail_defaults_and_script_owned_locks_are_authoritative() {
    let mut world = World::new();
    world.init_players(2);

    assert!(world.hud_item_enabled(HudItem::Resources));
    assert!(!world.hud_item_enabled(HudItem::Time));
    assert!(world.minimap_skirt_mirroring());
    assert_eq!(
        world.player_presentation_state(1),
        PlayerPresentationState::default()
    );

    let initial = world.checksum();
    world.set_player_user_lock(1, 7, true);
    world.set_player_user_lock(1, 9, true);
    assert_eq!(
        world.player_presentation_state(1).user_lock_owner_script,
        Some(7)
    );
    world.set_player_user_lock(1, 9, false);
    assert!(world.player_presentation_state(1).user_locked());
    world.set_player_user_lock(1, 7, false);
    assert!(!world.player_presentation_state(1).user_locked());
    assert_eq!(world.checksum(), initial);
}

#[test]
fn camera_directives_are_revisioned_even_when_pose_repeats() {
    let mut world = World::new();
    world.init_players(1);
    let location = Vec3::new(1.0, 2.0, 3.0);
    let direction = Vec3::X;

    world.set_player_camera_v4(
        1,
        [false, true, false],
        Some(location),
        Some(direction),
        Some(4.0),
    );
    let first = world.player_presentation_state(1).camera_directive.unwrap();
    world.set_player_camera_v4(
        1,
        [false, true, false],
        Some(location),
        Some(direction),
        Some(4.0),
    );
    let second = world.player_presentation_state(1).camera_directive.unwrap();

    assert!(second.revision > first.revision);
    assert_eq!(second.location, Some(location));
    assert_eq!(second.direction, Some(direction));
}
