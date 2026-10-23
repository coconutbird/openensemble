use super::*;
use crate::trigger::{TriggerVar, VarType};
use crate::world::ObjectiveState;

#[test]
fn retail_objective_counter_ids_are_typed() {
    let expected = [
        (133, EffectType::ObjectiveComplete),
        (183, EffectType::ObjectiveDisplay),
        (935, EffectType::ObjectiveIncrementCounter),
        (936, EffectType::ObjectiveDecrementCounter),
        (937, EffectType::ObjectiveGetCurrentCounter),
        (938, EffectType::ObjectiveGetFinalCounter),
    ];
    for (raw_type, effect_type) in expected {
        assert_eq!(EffectType::from_u16(raw_type), Some(effect_type));
    }
}

/// A world with one human player, one computer AI player and objective 4 with score 250.
fn scored_world() -> World {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(2).unwrap().player_type = crate::player::PlayerType::ComputerAi;
    let mut objective = ObjectiveState::new(4);
    objective.set_score(250);
    world.configure_objectives(vec![objective]);
    world
}

fn objective_script(completed: bool) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Objective,
        TriggerValue::Objective(4),
    );
    add_value(&mut script, 2, VarType::Bool, TriggerValue::Bool(completed));
    add_value(&mut script, 3, VarType::Time, TriggerValue::Time(2_500));
    script
}

fn state_effect(effect_type: EffectType, version: u8) -> Effect {
    let mut effect = Effect::new(1, effect_type)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);
    effect.version = version;
    effect
}

#[test]
fn objective_complete_v2_sets_time_score_and_reopen_notification() {
    let mut world = scored_world();
    world.advance_time(1_200);
    let effect = state_effect(EffectType::ObjectiveComplete, 2);
    let mut script = objective_script(true);
    let before = world.checksum();
    assert_eq!(
        execute(&effect, &mut script, &mut world),
        Some(EffectOutcome::Applied)
    );
    let objective = *world.objective(4).unwrap();
    assert!(objective.completed());
    assert_eq!(objective.completed_at_ms(), 1_200);
    assert_eq!(world.player_objective_score(1), 250);
    assert_eq!(world.player_objective_score(2), 0);
    // A completion adds no notification.
    assert_eq!(world.latest_objective_notification_sequence(), 0);
    assert_ne!(world.checksum(), before);

    // The same state again changes nothing, so the score is not added twice.
    let _outcome = execute(&effect, &mut script, &mut world);
    assert_eq!(world.player_objective_score(1), 250);

    let mut reopen = objective_script(false);
    let _outcome = execute(&effect, &mut reopen, &mut world);
    let objective = *world.objective(4).unwrap();
    assert!(!objective.completed());
    assert_eq!(objective.completed_at_ms(), 0);
    assert_eq!(world.player_objective_score(1), 0);
    let notifications: Vec<_> = world.objective_notifications_after(0).copied().collect();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].objective_id(), 4);
    assert!((notifications[0].duration_s() - 10.0).abs() < f32::EPSILON);
}

#[test]
fn objective_complete_other_versions_are_unsupported() {
    let mut world = scored_world();
    let effect = state_effect(EffectType::ObjectiveComplete, 1);
    let mut script = objective_script(true);
    assert_eq!(
        execute(&effect, &mut script, &mut world),
        Some(EffectOutcome::Unsupported(133))
    );
    assert!(!world.objective(4).unwrap().completed());
}

#[test]
fn objective_display_uses_version_two_duration_or_ten_seconds() {
    let mut world = scored_world();
    let mut script = objective_script(true);
    let v1 = state_effect(EffectType::ObjectiveDisplay, 1);
    let v2 = state_effect(EffectType::ObjectiveDisplay, 2);
    assert_eq!(
        execute(&v1, &mut script, &mut world),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(
        execute(&v2, &mut script, &mut world),
        Some(EffectOutcome::Applied)
    );
    assert!(world.objective(4).unwrap().displayed());
    let durations: Vec<_> = world
        .objective_notifications_after(0)
        .map(|notification| notification.duration_s())
        .collect();
    assert_eq!(durations.len(), 2);
    assert!((durations[0] - 10.0).abs() < f32::EPSILON);
    assert!((durations[1] - 2.5).abs() < 1e-6);

    // Hiding clears the flag and adds no notification.
    let mut hide = objective_script(false);
    let _outcome = execute(&v1, &mut hide, &mut world);
    assert!(!world.objective(4).unwrap().displayed());
    assert_eq!(world.latest_objective_notification_sequence(), 2);
    assert_eq!(world.objective_notifications_after(1).count(), 1);
}

#[test]
fn counter_family_clamps_and_writes_retail_outputs() {
    let mut objective = ObjectiveState::new(8);
    objective.set_final_count(3);
    let mut world = World::new();
    world.configure_objectives(vec![objective]);
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Objective,
        TriggerValue::Objective(8),
    );
    add_value(&mut script, 2, VarType::Integer, TriggerValue::Int(5));
    add_value(&mut script, 3, VarType::Integer, TriggerValue::Int(0));
    add_value(&mut script, 4, VarType::Integer, TriggerValue::Int(0));

    let increment = Effect::new(1, EffectType::ObjectiveIncrementCounter)
        .with_input_at(1, 1)
        .with_output_at(3, 3);
    assert_eq!(
        execute(&increment, &mut script, &mut world),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(world.objective_current_count(8), 1);
    assert_eq!(int_value(&script, 3), 1);

    let increment_five = Effect::new(2, EffectType::ObjectiveIncrementCounter)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3);
    let _outcome = execute(&increment_five, &mut script, &mut world);
    assert_eq!(world.objective_current_count(8), 3);
    assert_eq!(int_value(&script, 3), 3);

    let decrement = Effect::new(3, EffectType::ObjectiveDecrementCounter)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3);
    let _outcome = execute(&decrement, &mut script, &mut world);
    assert_eq!(world.objective_current_count(8), 0);
    assert_eq!(int_value(&script, 3), 0);

    let get_final = Effect::new(4, EffectType::ObjectiveGetFinalCounter)
        .with_input_at(1, 1)
        .with_output_at(2, 4);
    let _outcome = execute(&get_final, &mut script, &mut world);
    assert_eq!(int_value(&script, 4), 3);

    let get_current = Effect::new(5, EffectType::ObjectiveGetCurrentCounter)
        .with_input_at(1, 1)
        .with_output_at(2, 4);
    let _outcome = execute(&get_current, &mut script, &mut world);
    assert_eq!(int_value(&script, 4), 0);
}

#[test]
fn invalid_objective_queries_return_zero_and_mutation_remains_a_no_op() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Objective,
        TriggerValue::Objective(999),
    );
    add_value(&mut script, 2, VarType::Integer, TriggerValue::Int(-1));

    let query = Effect::new(1, EffectType::ObjectiveGetFinalCounter)
        .with_input_at(1, 1)
        .with_output_at(2, 2);
    assert_eq!(
        execute(&query, &mut script, &mut world),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(int_value(&script, 2), 0);

    let increment = Effect::new(2, EffectType::ObjectiveIncrementCounter)
        .with_input_at(1, 1)
        .with_output_at(3, 2);
    let _outcome = execute(&increment, &mut script, &mut world);
    assert_eq!(int_value(&script, 2), 0);
    assert!(world.objective(999).is_none());
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn int_value(script: &TriggerScript, id: u32) -> i32 {
    script
        .get_variable(id)
        .and_then(|variable| variable.value.as_int())
        .expect("integer variable")
}
