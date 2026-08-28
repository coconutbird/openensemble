use sim::{EffectType, Simulation, TriggerValue, load_scenario_from_game_dir};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-trigger-loading -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_triggers_load_remap_and_execute_from_the_authoritative_tick() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let scenario =
        std::env::var("OPENENSEMBLE_TEST_SCENARIO").unwrap_or_else(|_| "blood_gulch".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, &scenario)
        .expect("scenario, layered database, simulation, and triggers should load");

    let engine = loaded.simulation.world.trigger_engine();
    assert_eq!(engine.script_count(), 2);
    assert_eq!(engine.active_trigger_count(), 3);
    let blood_gulch = engine
        .scripts()
        .map(|(_, script)| script)
        .find(|script| script.name == "Blood_Gulch")
        .expect("Blood Gulch's gameplay trigger system should be loaded");
    assert_eq!(
        blood_gulch.variables.len(),
        6,
        "sparse variables stay sparse"
    );
    assert_eq!(blood_gulch.triggers.len(), 2);
    assert!(blood_gulch.triggers.iter().all(|trigger| {
        matches!(
            trigger.effects_on_true.as_slice(),
            [teleporter, debug]
                if teleporter.effect_type == EffectType::SetTeleporterDestination
                    && debug.effect_type == EffectType::DebugVarString
        )
    }));

    let entrance_a = loaded
        .simulation
        .get_entity_id(87)
        .expect("scenario squad 87 should be live");
    let exit_a = loaded
        .simulation
        .get_entity_id(5)
        .expect("scenario squad 5 should be live");
    let entrance_b = loaded
        .simulation
        .get_entity_id(86)
        .expect("scenario squad 86 should be live");
    let exit_b = loaded
        .simulation
        .get_entity_id(6)
        .expect("scenario squad 6 should be live");
    assert_eq!(squad_variable(blood_gulch, 20), entrance_a);
    assert_eq!(squad_variable(blood_gulch, 21), exit_a);
    assert_eq!(squad_variable(blood_gulch, 23), entrance_b);
    assert_eq!(squad_variable(blood_gulch, 24), exit_b);
    assert_eq!(
        loaded
            .simulation
            .world
            .get_squad(entrance_a)
            .expect("entrance A should be a squad")
            .teleporter_destination,
        None
    );

    let mut simulation = Simulation::new();
    simulation.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);

    assert_eq!(loaded.simulation.world.game_time(), 50);
    assert_eq!(loaded.simulation.world.trigger_engine().script_count(), 0);
    assert_eq!(
        loaded
            .simulation
            .world
            .get_squad(entrance_a)
            .expect("entrance A should remain live")
            .teleporter_destination,
        Some(exit_a)
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_squad(entrance_b)
            .expect("entrance B should remain live")
            .teleporter_destination,
        Some(exit_b)
    );
}

fn squad_variable(script: &sim::TriggerScript, variable_id: u32) -> sim::EntityId {
    match &script
        .get_variable(variable_id)
        .unwrap_or_else(|| panic!("trigger variable {variable_id} should exist"))
        .value
    {
        TriggerValue::Squad(entity_id) => *entity_id,
        value => panic!("trigger variable {variable_id} should be a squad, got {value:?}"),
    }
}
