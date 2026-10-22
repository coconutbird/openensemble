//! Presentation, hint, and callout assertions for the campaign trigger loading test.

use sim::{LoadedGameScenario, TriggerValue};

pub(super) fn assert_presentation_state(scenario: &str, loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    match scenario {
        "PHXscn01" => assert!(!world.hud_item_enabled(sim::HudItem::Resources)),
        "PHXscn03" => {
            assert_eq!(
                world.minimap_rotation_degrees().to_bits(),
                180.0_f32.to_bits()
            );
            assert!(world.chats_enabled());
            let shake = world.camera_shake(1).expect("PHXscn03 camera shake");
            assert_eq!(shake.strength().to_bits(), 2.0_f32.to_bits());
            assert_eq!(shake.conservation_factor().to_bits(), 0.5_f32.to_bits());
            let rumbles = world.rumble_requests(2).collect::<Vec<_>>();
            let [rumble] = rumbles.as_slice() else {
                panic!("PHXscn03 should start one player-two rumble, got {rumbles:?}");
            };
            assert_eq!(rumble.id(), 0);
            assert_eq!(rumble.left().rumble_type(), Some("Fixed"));
            assert_eq!(rumble.right().rumble_type(), Some("Fixed"));
            assert_eq!(rumble.left().strength().to_bits(), 0.75_f32.to_bits());
            assert_eq!(rumble.duration_seconds().to_bits(), 1.0_f32.to_bits());
            assert!(!rumble.looped());
            assert!(rumble.pattern().is_none());
            for player_id in 1..world.player_count() {
                let player_id = u8::try_from(player_id).unwrap();
                assert!(world.objective_pointers(player_id).next().is_none());
            }
        }
        "PHXscn04" => assert!(!world.render_terrain_skirt_enabled()),
        "PHXscn05" => assert_eq!(
            world.minimap_rotation_degrees().to_bits(),
            (-160.0_f32).to_bits()
        ),
        "PHXscn06" => assert_phx06_presentation(loaded),
        "PHXscn07" => assert_phx07_fade(loaded),
        "PHXscn11" | "PHXscn12" => assert!(!world.minimap_skirt_mirroring()),
        "campaignTutorial" => assert_tutorial_presentation(loaded, false),
        "campaignTutorialAdvanced" => assert_tutorial_presentation(loaded, true),
        _ => {}
    }
}

fn assert_phx07_fade(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    let overlay = world
        .screen_fade_overlay()
        .expect("PHXscn07 should begin its authored black transition");
    assert_eq!(overlay.revision(), 1);
    assert_eq!(overlay.color(), [0, 0, 0]);
    assert_eq!(overlay.opacity().to_bits(), 0.0_f32.to_bits());
    assert!(!world.screen_fade_completed());
}

fn assert_phx06_presentation(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    assert!(!world.render_terrain_skirt_enabled());
    assert_eq!(
        world.minimap_rotation_degrees().to_bits(),
        (-85.0_f32).to_bits()
    );
    for player_id in [1, 4] {
        let state = world.player_presentation_state(player_id);
        assert!(state.camera_controls.scroll);
        assert!(state.camera_controls.yaw);
        assert!(state.camera_controls.zoom);
        let directive = state
            .camera_directive
            .unwrap_or_else(|| panic!("PHXscn06 player {player_id} camera directive"));
        super::assertions::assert_vec3_close(
            directive.location.expect("PHXscn06 camera location"),
            glam::Vec3::new(266.0444, 69.3125, 235.5218),
            0.001,
        );
        super::assertions::assert_vec3_close(
            directive.direction.expect("PHXscn06 camera direction"),
            glam::Vec3::new(0.6442, 0.0, 0.7648),
            0.0001,
        );
        assert!(directive.hover_height_offset.is_none());
    }
}

pub(super) fn assert_hint_callout_state(scenario: &str, loaded: &LoadedGameScenario) {
    let expected = match scenario {
        "PHXscn03" => Some((22934, glam::Vec3::new(368.8735, -33.8938, 635.7485))),
        "PHXscn11" => Some((25992, glam::Vec3::new(469.1019, 35.2007, 572.122))),
        "PHXscn15" => Some((24788, glam::Vec3::new(249.8237, -2.2667, 492.1157))),
        _ => None,
    };
    let Some((string_id, location)) = expected else {
        return;
    };
    let callouts = loaded.simulation.world.hint_callouts().collect::<Vec<_>>();
    let [callout] = callouts.as_slice() else {
        panic!("{scenario} should leave one active hint callout, got {callouts:?}");
    };
    assert_eq!(callout.id(), 0);
    assert_eq!(callout.widget_slot(), 0);
    assert_eq!(callout.string_id(), string_id);
    let sim::HintCalloutAnchor::Location(actual) = callout.anchor() else {
        panic!("{scenario} callout should use its authored location");
    };
    super::assertions::assert_vec3_close(actual, location, 0.0001);

    if scenario == "PHXscn03" {
        let output = loaded
            .simulation
            .world
            .trigger_engine()
            .get_script(0)
            .and_then(|script| script.get_variable(10301))
            .map(|variable| &variable.value);
        assert_eq!(output, Some(&TriggerValue::Int(0)));
    }
}

fn assert_tutorial_presentation(loaded: &LoadedGameScenario, advanced: bool) {
    let world = &loaded.simulation.world;
    assert!(world.screen_blur_enabled());
    assert_eq!(world.circle_menu_reset_revision(), u32::from(advanced));
    let state = world.player_presentation_state(1);
    assert!(!state.camera_controls.scroll);
    assert!(!state.camera_controls.yaw);
    assert!(!state.camera_controls.zoom);
    assert_eq!(state.ignore_dpad, !advanced);
    assert_eq!(state.user_lock_owner_script, Some(0));
}
