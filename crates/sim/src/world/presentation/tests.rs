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
    let input = CameraDirectiveInput {
        controls: [false, true, false],
        location: Some(location),
        direction: Some(direction),
        hover_height_offset: Some(4.0),
        absolute_position: false,
        absolute_direction: true,
    };

    world.set_player_camera(1, input);
    let first = world.player_presentation_state(1).camera_directive.unwrap();
    world.set_player_camera(1, input);
    let second = world.player_presentation_state(1).camera_directive.unwrap();

    assert!(second.revision > first.revision);
    assert_eq!(second.location, Some(location));
    assert_eq!(second.direction, Some(direction));
    assert!(!second.absolute_position);
    assert!(second.absolute_direction);
}

#[test]
fn hint_message_same_id_resets_time_and_other_id_replaces_it() {
    let mut world = World::new();
    world.init_players(2);
    world.show_hint_message(10, &[1], 5.0);
    assert_eq!(world.hint_message(1).map(HintMessage::string_id), Some(10));
    assert!(world.hint_message(2).is_none());

    world.game_time_ms = 2_000;
    world.show_hint_message(10, &[], 7.0);
    let hint = world.hint_message(1).unwrap();
    assert_eq!(hint.shown_at_ms(), 2_000);
    assert!((hint.duration_s() - 7.0).abs() < f32::EPSILON);

    world.show_hint_message(11, &[2], 3.0);
    assert!(world.hint_message(1).is_none());
    assert_eq!(world.hint_message(2).map(HintMessage::string_id), Some(11));

    world.destroy_hint_message(Some(99));
    assert!(world.hint_message(2).is_some());
    world.destroy_hint_message(None);
    assert!(world.hint_message(2).is_none());
}

#[test]
fn flares_expire_after_thirty_seconds() {
    let mut world = World::new();
    world.init_players(2);
    world.add_minimap_flare(1, Vec3::ONE, 5);
    let flare = world.minimap_flares(1).next().unwrap();
    assert_eq!(flare.from_player(), 5);
    assert_eq!(flare.visible_until_ms(), 6_000);
    world.game_time_ms = 29_999;
    assert_eq!(world.minimap_flares(1).count(), 1);
    world.game_time_ms = 30_000;
    assert_eq!(world.minimap_flares(1).count(), 0);
    world.add_minimap_flare(9, Vec3::ONE, 1);
    assert_eq!(world.minimap_flares(9).count(), 0);
}

#[test]
fn user_message_negative_values_and_null_text_keep_the_old_slot() {
    let mut world = World::new();
    world.init_players(2);
    let update = |text: &str, value: f32, justify: i32| UserMessageUpdate {
        players: vec![2],
        text: text.to_owned(),
        position: [value, value],
        justify,
        point: value,
        alpha: value,
        color: None,
        enabled: true,
    };
    assert!(world.set_user_message(3, update("hello", 48.0, 2)));
    assert!(world.set_user_message(3, update("NULL", -1.0, 7)));
    let slot = world.user_message(3).unwrap();
    assert_eq!(slot.text(), "hello");
    assert!(slot.shown_to(2));
    assert!(!slot.shown_to(1));
    assert_eq!(slot.position(), [48.0, 48.0]);
    assert!((slot.scale() - 2.0).abs() < f32::EPSILON);
    assert_eq!(slot.justify(), UserMessageJustify::Right);
    assert!(!world.set_user_message(
        i32::try_from(USER_MESSAGE_COUNT).unwrap(),
        update("x", 0.0, 0)
    ));
}

#[test]
fn ui_flash_bits_power_cancel_and_global_flags_are_checksummed() {
    let mut world = World::new();
    world.init_players(1);
    assert!(world.music_manager_enabled());
    assert!(!world.chat_subtitles_forced());
    let initial = world.checksum();

    world.set_player_ui_item_flashing(1, 17, true);
    world.set_player_ui_item_flashing(1, 0, true);
    world.set_player_ui_item_flashing(1, 17, false);
    assert_eq!(world.player_presentation_state(1).flashing_ui_items, 1);
    world.cancel_player_power_targeting(1);
    assert_eq!(
        world
            .player_presentation_state(1)
            .power_targeting_cancel_revision,
        1
    );
    world.set_music_manager_enabled(false);
    world.set_chat_subtitles_forced(true);
    assert!(!world.music_manager_enabled());
    assert!(world.chat_subtitles_forced());
    assert_ne!(world.checksum(), initial);
}

#[test]
fn lightset_animation_needs_a_scenario_lightset_id() {
    let mut world = World::new();
    world.configure_lightsets(vec![0, 4]);
    assert!(!world.animate_lightset(3, 1_000));
    assert!(world.lightset_animation().is_none());
    world.game_time_ms = 500;
    assert!(world.animate_lightset(4, 1_000));
    let animation = world.lightset_animation().unwrap();
    assert_eq!((animation.start_ms, animation.end_ms), (500, 1_500));
}
