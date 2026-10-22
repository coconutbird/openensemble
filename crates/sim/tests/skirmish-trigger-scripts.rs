//! Runs the retail skirmish world and player scripts on `blood_gulch`.
//!
//! `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test skirmish-trigger-scripts -- --ignored`

use sim::trigger::{LibraryScript, TriggerExternals};

/// Effect DBIDs that the trigger runtime slice added for the skirmish scripts.
const NEW_EFFECTS: [u16; 8] = [56, 130, 173, 392, 781, 904, 943, 970];

/// Condition DBIDs that the trigger runtime slice added for the skirmish scripts.
const NEW_CONDITIONS: [u16; 5] = [192, 440, 566, 975, 977];

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn skirmish_scripts_run_the_new_handlers_without_unsupported_ids() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let database = &loaded.content.database;
    let scenario = &mut loaded.simulation;

    // The scenario loader preloads `preloadxmlfiles.xml`, so `LaunchScript` finds the crate
    // and creep scripts without file reads in the tick.
    for file_name in ["Creeps.triggerscript", "CrateMaker3.triggerscript"] {
        assert!(
            matches!(
                scenario
                    .world
                    .trigger_engine()
                    .script_library()
                    .get(file_name),
                LibraryScript::Parsed(_)
            ),
            "{file_name} is preloaded"
        );
    }

    let world_script = scenario
        .launch_trigger_script(
            &mut loaded.source,
            database,
            "SkirmishWorld.triggerscript",
            TriggerExternals::default(),
        )
        .expect("SkirmishWorld parses")
        .expect("SkirmishWorld exists");
    let player_script = scenario
        .launch_trigger_script(
            &mut loaded.source,
            database,
            "skirmishplayer.TRIGGERSCRIPT",
            TriggerExternals::with_player(1),
        )
        .expect("SkirmishPlayer parses")
        .expect("SkirmishPlayer exists");
    assert!(player_script > world_script);

    let mut unsupported_effects = Vec::new();
    let mut unsupported_conditions = Vec::new();
    let mut missing_files = Vec::new();
    for _ in 0..80 {
        let update = scenario
            .world
            .update_triggers_with_gameplay(database, &scenario.gameplay);
        unsupported_effects.extend(update.unsupported_effect_types);
        unsupported_conditions.extend(update.unsupported_condition_types);
        missing_files.extend(update.missing_script_files);
        scenario.world.game_time_ms += 50;
    }

    for raw_type in NEW_EFFECTS {
        assert!(
            !unsupported_effects.contains(&raw_type),
            "effect {raw_type} is unsupported: {unsupported_effects:?}"
        );
    }
    for raw_type in NEW_CONDITIONS {
        assert!(
            !unsupported_conditions.contains(&raw_type),
            "condition {raw_type} is unsupported: {unsupported_conditions:?}"
        );
    }
    assert!(
        missing_files.is_empty(),
        "missing scripts: {missing_files:?}"
    );
    // `CanRetrieveExternals` (192) read the player external, so the player script left its
    // first trigger. Before, the script waited there for ever.
    let left_first_trigger = scenario
        .world
        .trigger_engine()
        .get_script(player_script)
        .is_none_or(|script| !script.is_trigger_active(0));
    assert!(left_first_trigger);
}
