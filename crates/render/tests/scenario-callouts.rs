//! Checks that a `PHXscn03` hint callout projects from the sim and the localized content.
//!
//! The test needs a local Halo Wars DE installation.

use glam::{Mat4, Vec3};
use render::terrain::project_hint_callouts;
use sim::HintCalloutAnchor;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-callouts -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn campaign_callout_projects_from_authoritative_sim_and_localized_content() {
    let mut loaded = test_support::installed_scenario("PHXscn03");
    let update = loaded
        .simulation
        .world
        .update_triggers_with_gameplay(&loaded.content.database, &loaded.simulation.gameplay);
    assert!(!update.unsupported_effect_types.contains(&809));

    let callout = loaded
        .simulation
        .world
        .hint_callouts()
        .next()
        .expect("PHXscn03 initial hint callout");
    assert_eq!(callout.string_id(), 22934);
    let HintCalloutAnchor::Location(location) = callout.anchor() else {
        panic!("PHXscn03 hint should use a fixed location");
    };
    let text = loaded
        .content
        .resolve_string(callout.string_id())
        .expect("localized callout text");
    assert_ne!(text.trim(), "");

    let projected = project_hint_callouts(
        &loaded.simulation.world,
        Mat4::from_translation(-location),
        [1280.0, 720.0],
    );
    assert_eq!(projected.len(), 1);
    let [screen_x, screen_y] = projected[0].screen_position;
    assert!((screen_x - 640.0).abs() <= f32::EPSILON);
    assert!((screen_y - 360.0).abs() <= f32::EPSILON);
    assert_eq!(projected[0].string_id, 22934);
    assert!(location.abs_diff_eq(Vec3::new(368.8735, -33.8938, 635.7485), 0.0001));
}
