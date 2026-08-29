use super::*;
use crate::trigger::{TriggerVar, VarType};
use crate::world::ObjectiveState;

#[test]
fn retail_objective_counter_ids_are_typed() {
    let expected = [
        (935, EffectType::ObjectiveIncrementCounter),
        (936, EffectType::ObjectiveDecrementCounter),
        (937, EffectType::ObjectiveGetCurrentCounter),
        (938, EffectType::ObjectiveGetFinalCounter),
    ];
    for (raw_type, effect_type) in expected {
        assert_eq!(EffectType::from_u16(raw_type), Some(effect_type));
    }
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
