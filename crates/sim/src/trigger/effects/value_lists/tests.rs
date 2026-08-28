use super::*;
use crate::trigger::{TriggerVar, VarType};

#[test]
fn location_lists_append_duplicates_remove_first_and_report_size() {
    let first = TriggerVec3::new(1.0, 2.0, 3.0);
    let second = TriggerVec3::new(4.0, 5.0, 6.0);
    let mut script = TriggerScript::new(1);
    add_var(
        &mut script,
        1,
        VarType::VectorList,
        TriggerValue::VectorList(vec![first]),
    );
    add_var(&mut script, 2, VarType::Vector, TriggerValue::Vector(first));
    add_var(
        &mut script,
        3,
        VarType::VectorList,
        TriggerValue::VectorList(vec![second]),
    );
    add_var(&mut script, 4, VarType::Bool, TriggerValue::Bool(false));
    add_var(&mut script, 5, VarType::Integer, TriggerValue::Int(0));

    let add = Effect::new(1, EffectType::LocationListAdd)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4);
    assert_eq!(execute(&add, &mut script), EffectOutcome::Applied);
    assert_eq!(vectors(&script, 1), &[first, first, second]);

    let remove = Effect::new(2, EffectType::LocationListRemove)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    assert_eq!(execute(&remove, &mut script), EffectOutcome::Applied);
    assert_eq!(vectors(&script, 1), &[first, second]);

    let size = Effect::new(3, EffectType::LocationListGetSize)
        .with_input_at(1, 1)
        .with_output_at(2, 5);
    assert_eq!(execute(&size, &mut script), EffectOutcome::Applied);
    assert_eq!(script.get_variable(5).unwrap().value, TriggerValue::Int(2));
}

#[test]
fn integer_remove_can_remove_one_or_every_duplicate() {
    let mut script = TriggerScript::new(1);
    add_var(
        &mut script,
        1,
        VarType::IntegerList,
        TriggerValue::IntegerList(vec![7, 7, 9]),
    );
    add_var(&mut script, 2, VarType::Integer, TriggerValue::Int(7));
    add_var(&mut script, 3, VarType::Bool, TriggerValue::Bool(false));
    add_var(&mut script, 4, VarType::Bool, TriggerValue::Bool(false));
    let remove = Effect::new(1, EffectType::IntegerListRemove)
        .with_input_at(1, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_output_at(5, 1);

    assert_eq!(execute(&remove, &mut script), EffectOutcome::Applied);
    assert_eq!(integers(&script, 1), &[7, 9]);

    script.get_variable_mut(4).unwrap().value = TriggerValue::Bool(true);
    assert_eq!(execute(&remove, &mut script), EffectOutcome::Applied);
    assert_eq!(integers(&script, 1), &[9]);
}

fn add_var(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn vectors(script: &TriggerScript, id: u32) -> &[TriggerVec3] {
    match &script.get_variable(id).unwrap().value {
        TriggerValue::VectorList(values) => values,
        _ => panic!("expected vector list"),
    }
}

fn integers(script: &TriggerScript, id: u32) -> &[i32] {
    match &script.get_variable(id).unwrap().value {
        TriggerValue::IntegerList(values) => values,
        _ => panic!("expected integer list"),
    }
}
