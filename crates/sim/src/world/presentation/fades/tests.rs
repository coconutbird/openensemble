use super::*;

#[test]
fn normal_transition_fades_down_holds_and_fades_up_in_world_time() {
    let mut world = World::new();
    let initial = world.checksum();
    let revision = world.start_screen_fade(
        [12, 34, 56],
        ScreenFadeSequence::Transition {
            fade_down_ms: 100,
            hold_ms: 50,
            fade_up_ms: 100,
            reverse: false,
        },
    );
    assert_eq!(revision, 1);
    assert_eq!(world.screen_fade_overlay().unwrap().rgba(), [12, 34, 56, 0]);
    assert!(!world.screen_fade_completed());
    assert_ne!(world.checksum(), initial);

    world.advance_time(50);
    assert_eq!(
        world.screen_fade_overlay().unwrap().rgba(),
        [12, 34, 56, 127]
    );
    world.advance_time(50);
    assert_eq!(
        world.screen_fade_overlay().unwrap().rgba(),
        [12, 34, 56, 255]
    );
    world.advance_time(75);
    assert_eq!(
        world.screen_fade_overlay().unwrap().rgba(),
        [12, 34, 56, 191]
    );
    world.advance_time(75);
    assert!(world.screen_fade_overlay().is_none());
    assert!(world.screen_fade_completed());
}

#[test]
fn reverse_transition_and_replacement_match_retail_completion_state() {
    let mut world = World::new();
    world.start_screen_fade(
        [0, 0, 0],
        ScreenFadeSequence::Transition {
            fade_down_ms: 100,
            hold_ms: 50,
            fade_up_ms: 200,
            reverse: true,
        },
    );
    world.advance_time(100);
    assert_eq!(world.screen_fade_overlay().unwrap().rgba()[3], 127);

    let revision = world.start_screen_fade(
        [255, 0, 0],
        ScreenFadeSequence::ToColor {
            duration_ms: 100,
            fade_in: false,
        },
    );
    assert_eq!(revision, 2);
    assert!(world.screen_fade_completed());
    assert_eq!(world.screen_fade_overlay().unwrap().rgba(), [255, 0, 0, 0]);

    world.advance_time(100);
    assert!(world.screen_fade_overlay().is_none());
    assert!(world.screen_fade_completed());
}

#[test]
fn reset_clears_active_fades_and_completion_subscribers() {
    let mut world = World::new();
    world.start_screen_fade(
        [0, 0, 0],
        ScreenFadeSequence::ToColor {
            duration_ms: 1,
            fade_in: false,
        },
    );
    world.advance_time(1);
    assert!(world.screen_fade_completed());

    world.reset();

    assert!(world.screen_fade_overlay().is_none());
    assert!(!world.screen_fade_completed());
}
