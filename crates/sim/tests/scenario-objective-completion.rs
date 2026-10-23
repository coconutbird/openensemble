//! Runs a campaign scenario until a trigger completes an objective.

use sim::{EntityId, ObjectiveId, Simulation, TriggerScript, TriggerValue};

/// The `PHXscn02` trigger that starts active and completes an objective when the detonator dies.
const DETONATOR_TRIGGER: &str = "Detonator is destroyed";
/// `ObjectiveComplete` (133).
const OBJECTIVE_COMPLETE: u16 = 133;
/// Ticks that the test runs after the kill (20 seconds of game time).
const MAX_TICKS: u32 = 400;

/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-objective-completion -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn phx02_detonator_kill_completes_its_objective() {
    let mut loaded = test_support::installed_scenario("PHXscn02");
    let (detonator, objective_id) = detonator_trigger_inputs(&loaded);
    let world = &loaded.simulation.world;
    assert!(
        !world
            .objective(objective_id)
            .expect("objective")
            .completed(),
        "the objective starts open"
    );
    let score_before = world.player_objective_score(1);

    // The sim API stands in for the player input that destroys the detonator.
    assert!(loaded.simulation.world.kill_entity(detonator, true));
    let mut simulation = Simulation::new();
    let mut completed_tick = None;
    for tick in 0..MAX_TICKS {
        simulation.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
        if loaded
            .simulation
            .world
            .objective(objective_id)
            .is_some_and(|objective| objective.completed())
        {
            completed_tick = Some(tick);
            break;
        }
    }
    assert!(
        completed_tick.is_some(),
        "the detonator trigger completes objective {objective_id}"
    );

    let world = &loaded.simulation.world;
    let objective = world.objective(objective_id).expect("objective");
    assert!(objective.completed_at_ms() > 0);
    assert!(objective.completed_at_ms() <= world.game_time());
    assert_eq!(
        world.player_objective_score(1),
        score_before + i32::try_from(objective.score()).expect("score fits"),
        "a completed objective adds its score for the human player",
    );
}

/// Read the detonator entity of the `IsDead` condition and the objective of the
/// `ObjectiveComplete` effect of the detonator trigger.
fn detonator_trigger_inputs(loaded: &sim::LoadedGameScenario) -> (EntityId, ObjectiveId) {
    let (script, trigger) = loaded
        .simulation
        .world
        .trigger_engine()
        .scripts()
        .find_map(|(_, script)| {
            script
                .triggers
                .iter()
                .find(|trigger| trigger.name == DETONATOR_TRIGGER)
                .map(|trigger| (script, trigger))
        })
        .expect("PHXscn02 has the detonator trigger");
    assert!(trigger.start_active, "the detonator trigger starts active");
    let detonator = trigger
        .conditions
        .iter()
        .flat_map(|condition| (1..=4).filter_map(|slot| condition.variable_id(slot)))
        .find_map(|variable_id| value(script, variable_id)?.as_entity())
        .expect("the IsDead condition names the detonator");
    let objective_id = trigger
        .effects()
        .filter(|effect| effect.raw_type == OBJECTIVE_COMPLETE)
        .flat_map(|effect| (1..=3).filter_map(|slot| effect.variable_id(slot)))
        .find_map(|variable_id| match value(script, variable_id)? {
            TriggerValue::Objective(objective_id) => Some(*objective_id),
            _ => None,
        })
        .expect("the trigger completes an objective");
    (detonator, objective_id)
}

fn value(script: &TriggerScript, variable_id: sim::VarId) -> Option<&TriggerValue> {
    script
        .get_variable(variable_id)
        .map(|variable| &variable.value)
}
