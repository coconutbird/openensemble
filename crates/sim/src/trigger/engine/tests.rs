use super::*;
use crate::player::PlayerState;
use crate::trigger::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerValue, TriggerVar, VarType,
};

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn time_reached(id: i32, time_variable: u32) -> Condition {
    Condition::new(id, ConditionType::GameTimeReached).with_input_at(1, time_variable)
}

fn activate(id: i32, trigger_variable: u32) -> Effect {
    Effect::new(id, EffectType::TriggerActivate).with_input_at(1, trigger_variable)
}

fn increment(id: i32, source: u32, destination: u32) -> Effect {
    Effect::new(id, EffectType::CountIncrement)
        .with_input_at(1, source)
        .with_output_at(2, destination)
}

fn set_player_state(id: i32, player_variable: u32, state_variable: u32) -> Effect {
    let mut effect = Effect::new(id, EffectType::SetPlayerState)
        .with_input_at(1, player_variable)
        .with_input_at(3, state_variable);
    effect.version = 2;
    effect
}

fn world_with_players(count: u8) -> World {
    let mut world = World::new();
    world.init_players(count);
    world
}

/// `BTriggerScript_activateTrigger` appends to the active list, and `BTriggerScript_update`
/// copies that list into the queue. T0 activates T2 and then T1, so the next update runs T2
/// before T1 although T1 comes first in the trigger array.
#[test]
fn activation_order_decides_which_effect_runs_first() {
    let mut script = TriggerScript::default();
    add(&mut script, 0, VarType::Trigger, TriggerValue::Trigger(2));
    add(&mut script, 1, VarType::Trigger, TriggerValue::Trigger(1));
    add(&mut script, 2, VarType::Time, TriggerValue::Time(100));
    add(&mut script, 3, VarType::Integer, TriggerValue::Int(0));
    add(&mut script, 4, VarType::Integer, TriggerValue::Int(0));
    add(&mut script, 5, VarType::Time, TriggerValue::Time(1_000));
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(activate(0, 0))
            .with_effect_on_true(activate(1, 1)),
    );
    script.add_trigger(
        Trigger::new(1)
            .with_condition(time_reached(0, 2))
            .with_effect_on_true(increment(0, 3, 4)),
    );
    script.add_trigger(
        Trigger::new(2)
            .with_condition(time_reached(0, 2))
            .with_effect_on_true(increment(0, 3, 3)),
    );
    script.add_trigger(
        Trigger::new(3)
            .starts_active()
            .with_condition(time_reached(0, 5)),
    );
    let mut world = world_with_players(1);
    let mut engine = TriggerEngine::new();
    let script_id = engine.add_script(script);
    engine.activate_script(script_id, 0);

    let _first = engine.update(&mut world);
    assert_eq!(
        engine.get_script(script_id).unwrap().active_triggers(),
        &[3, 2, 1]
    );
    world.game_time_ms = 100;
    let _second = engine.update(&mut world);
    let script = engine.get_script(script_id).unwrap();
    assert_eq!(script.get_variable(3).unwrap().value, TriggerValue::Int(1));
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Int(2),
        "T2 (activated first) runs before T1"
    );
}

/// `Shutdown` (case `0x38`) clears the active list and the queue, but the rest of the effect
/// list runs. The manager deletes the script after its update.
#[test]
fn shutdown_runs_the_remaining_effects_and_deletes_the_script() {
    let mut script = TriggerScript::default();
    add(&mut script, 0, VarType::Player, TriggerValue::Player(1));
    add(&mut script, 1, VarType::Player, TriggerValue::Player(2));
    add(&mut script, 2, VarType::PlayerState, TriggerValue::Int(2));
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(Effect::new(0, EffectType::Shutdown))
            .with_effect_on_true(set_player_state(1, 0, 2)),
    );
    script.add_trigger(
        Trigger::new(1)
            .starts_active()
            .with_effect_on_true(set_player_state(0, 1, 2)),
    );
    let mut world = world_with_players(2);
    let mut engine = TriggerEngine::new();
    let script_id = engine.add_script(script);
    engine.activate_script(script_id, 0);

    let update = engine.update(&mut world);
    assert_eq!(update.triggers_fired, 1);
    assert!(engine.get_script(script_id).is_none());
    assert_eq!(world.get_player(1).unwrap().state, PlayerState::Defeated);
    assert_eq!(world.get_player(2).unwrap().state, PlayerState::Playing);
}

/// Retail starts script IDs at 0 and gives one ID to each created script in creation order.
#[test]
fn script_ids_start_at_zero_in_creation_order() {
    let mut engine = TriggerEngine::new();
    assert_eq!(engine.add_script(TriggerScript::default()), 0);
    assert_eq!(engine.add_script(TriggerScript::default()), 1);
}

fn child_script() -> TriggerScript {
    let mut script = TriggerScript::default();
    script.add_variable(TriggerVar::new(0, VarType::Player).with_value(TriggerValue::Player(-1)));
    script.add_variable(TriggerVar::new(1, VarType::Integer).null());
    add(&mut script, 2, VarType::PlayerState, TriggerValue::Int(4));
    add(&mut script, 3, VarType::Float, TriggerValue::Float(0.0));
    let mut retrieve = Condition::new(0, ConditionType::CanRetrieveExternals)
        .with_output_at(1, 0)
        .with_output_at(2, 1);
    retrieve.version = 2;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_condition(retrieve)
            .with_condition(
                Condition::new(1, ConditionType::CanRetrieveExternalFloat).with_output_at(1, 3),
            )
            .with_effect_on_true(set_player_state(0, 0, 2)),
    );
    script
}

fn launcher_script(file_name: &str) -> TriggerScript {
    let mut script = TriggerScript::default();
    add(
        &mut script,
        0,
        VarType::String,
        TriggerValue::String(file_name.to_owned()),
    );
    add(&mut script, 1, VarType::Player, TriggerValue::Player(2));
    add(&mut script, 2, VarType::Float, TriggerValue::Float(2.5));
    script.add_variable(TriggerVar::new(3, VarType::Vector).null());
    let mut launch = Effect::new(0, EffectType::LaunchScript)
        .with_input_at(15, 0)
        .with_input_at(6, 1)
        .with_input_at(13, 2)
        .with_input_at(11, 3);
    launch.version = 4;
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(launch));
    script
}

/// `LaunchScript` v4 creates the script with the next ID and the non-null externals and
/// activates it. The manager loop count is fixed, so the new script first runs in the next
/// update (`BTriggerManager_update`).
#[test]
fn launch_script_runs_the_new_script_in_the_next_update() {
    let mut world = world_with_players(2);
    let mut engine = TriggerEngine::new();
    engine
        .script_library_mut()
        .insert("Child.triggerscript", child_script());
    let parent = engine.add_script(launcher_script("child.TRIGGERSCRIPT"));
    engine.activate_script(parent, 0);

    let first = engine.update(&mut world);
    assert_eq!(first.launched_scripts, vec![1]);
    let child = engine.get_script(1).expect("the launched script exists");
    assert_eq!(child.name, "child.TRIGGERSCRIPT");
    assert_eq!(child.externals.player, Some(2));
    assert_eq!(child.externals.float, Some(2.5));
    assert_eq!(child.externals.location, None);
    assert_eq!(child.active_triggers(), &[0]);
    assert_eq!(world.get_player(2).unwrap().state, PlayerState::Playing);

    let _second = engine.update(&mut world);
    assert_eq!(world.get_player(2).unwrap().state, PlayerState::Won);
}

/// A missing file uses no script ID. A file that did not parse uses one ID, like a retail
/// script whose `BTriggerScript_load` fails.
#[test]
fn failed_launches_follow_the_retail_script_id_rules() {
    let mut world = world_with_players(1);
    let mut engine = TriggerEngine::new();
    engine
        .script_library_mut()
        .insert_invalid("Broken.triggerscript");
    let missing = engine.add_script(launcher_script("Missing.triggerscript"));
    let broken = engine.add_script(launcher_script("Broken.triggerscript"));
    engine.activate_script(missing, 0);
    engine.activate_script(broken, 0);

    let update = engine.update(&mut world);
    assert_eq!(
        update.missing_script_files,
        vec![
            "Missing.triggerscript".to_owned(),
            "Broken.triggerscript".to_owned()
        ]
    );
    assert_eq!(engine.next_script_id, 3);
    assert_eq!(engine.script_count(), 0);
}

fn unit_list_script(units: Vec<crate::EntityId>) -> TriggerScript {
    let mut script = TriggerScript::default();
    add(
        &mut script,
        0,
        VarType::UnitList,
        TriggerValue::UnitList(units.clone()),
    );
    add(&mut script, 1, VarType::Integer, TriggerValue::Int(-1));
    script.add_variable(
        TriggerVar::new(2, VarType::UnitList)
            .with_value(TriggerValue::UnitList(units))
            .null(),
    );
    add(&mut script, 3, VarType::Time, TriggerValue::Time(100));
    add(&mut script, 4, VarType::Time, TriggerValue::Time(1_000));
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_condition(time_reached(0, 3))
            .with_effect_on_true(
                Effect::new(0, EffectType::UnitListGetSize)
                    .with_input_at(1, 0)
                    .with_output_at(2, 1),
            ),
    );
    script.add_trigger(
        Trigger::new(1)
            .starts_active()
            .with_condition(time_reached(0, 4)),
    );
    script
}

/// `BTriggerScript_invalidateEntityID`: a destroyed unit leaves each non-null unit list at the
/// start of the next update of an active script. Only the first copy goes. A script that
/// starts after the destruction keeps the ID.
#[test]
fn destroyed_units_leave_unit_lists_before_the_next_update() {
    let mut world = world_with_players(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    let script_id = world
        .trigger_engine_mut()
        .add_script(unit_list_script(vec![first, second, first]));
    world.trigger_engine_mut().activate_script(script_id, 0);
    let _idle = world.update_triggers();

    assert!(world.remove_unit(first).is_some());
    let late_id = world
        .trigger_engine_mut()
        .add_script(unit_list_script(vec![first]));
    world.trigger_engine_mut().activate_script(late_id, 0);
    assert_eq!(
        world
            .trigger_engine()
            .get_script(script_id)
            .unwrap()
            .pending_invalid_entities(),
        &[first]
    );
    world.game_time_ms = 100;
    let _update = world.update_triggers();
    let script = world.trigger_engine().get_script(script_id).unwrap();
    assert_eq!(script.get_variable(1).unwrap().value, TriggerValue::Int(2));
    assert_eq!(
        script.get_variable(0).unwrap().value,
        TriggerValue::UnitList(vec![second, first])
    );
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::UnitList(vec![first, second, first]),
        "retail skips null list variables"
    );
    let late = world.trigger_engine().get_script(late_id).unwrap();
    assert_eq!(late.get_variable(1).unwrap().value, TriggerValue::Int(1));
}

/// A unit that an effect destroys leaves the lists of the running script at once, and the
/// lists of the other scripts at their next update.
#[test]
fn units_destroyed_by_an_effect_leave_the_running_script_at_once() {
    let mut world = world_with_players(1);
    let doomed = world.create_unit(1);
    let other = world.create_unit(1);
    let mut script = TriggerScript::default();
    add(
        &mut script,
        0,
        VarType::UnitList,
        TriggerValue::UnitList(vec![doomed, other]),
    );
    add(&mut script, 1, VarType::Integer, TriggerValue::Int(-1));
    add(&mut script, 2, VarType::Unit, TriggerValue::Unit(doomed));
    add(&mut script, 3, VarType::Time, TriggerValue::Time(1_000));
    let mut destroy = Effect::new(0, EffectType::Destroy).with_input_at(3, 2);
    destroy.version = 3;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(destroy)
            .with_effect_on_true(
                Effect::new(1, EffectType::UnitListGetSize)
                    .with_input_at(1, 0)
                    .with_output_at(2, 1),
            ),
    );
    script.add_trigger(
        Trigger::new(1)
            .starts_active()
            .with_condition(time_reached(0, 3)),
    );

    // The watcher runs first, so the destroyed unit waits for its next update.
    let engine = world.trigger_engine_mut();
    let watcher_id = engine.add_script(unit_list_script(vec![doomed]));
    let script_id = engine.add_script(script);
    engine.activate_script(watcher_id, 0);
    engine.activate_script(script_id, 0);
    let _update = world.update_triggers();

    assert!(world.get_unit(doomed).is_none());
    let script = world.trigger_engine().get_script(script_id).unwrap();
    assert_eq!(script.get_variable(1).unwrap().value, TriggerValue::Int(1));
    let watcher = world.trigger_engine().get_script(watcher_id).unwrap();
    assert_eq!(watcher.pending_invalid_entities(), &[doomed]);
    assert_eq!(
        watcher.get_variable(0).unwrap().value,
        TriggerValue::UnitList(vec![doomed])
    );
}
