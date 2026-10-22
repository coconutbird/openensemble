use super::*;
use crate::trigger::{Condition, ConditionType, Effect, EffectType, TriggerVar, VarType};

#[test]
fn inputs_win_over_outputs_and_the_first_duplicate_wins() {
    let inputs = [VarBinding::new(2, 10), VarBinding::new(2, 11)];
    let outputs = [VarBinding::new(2, 20), VarBinding::new(3, 30)];
    let bindings = Bindings::new(&inputs, &outputs);

    assert_eq!(bindings.variable_id(2), Some(10));
    assert_eq!(bindings.variable_id(3), Some(30));
    assert_eq!(bindings.variable_id(1), None);
}

#[test]
fn automatic_slots_count_each_side_and_stop_at_the_maximum() {
    let condition = Condition::new(1, ConditionType::GameTime)
        .with_input(10)
        .with_output(20)
        .with_input(11);
    assert_eq!(condition.variable_id(1), Some(10));
    assert_eq!(condition.variable_id(2), Some(11));
    assert_eq!(condition.outputs, [VarBinding::new(1, 20)]);

    let effect = Effect::new(1, EffectType::TriggerActivate)
        .with_output(20)
        .with_input(10);
    assert_eq!(effect.inputs, [VarBinding::new(1, 10)]);
    assert_eq!(effect.outputs, [VarBinding::new(1, 20)]);

    let mut bindings = vec![VarBinding::new(1, 0); usize::from(u16::MAX)];
    push_next(&mut bindings, 7);
    push_next(&mut bindings, 8);
    assert_eq!(bindings[bindings.len() - 2], VarBinding::new(u16::MAX, 7));
    assert_eq!(bindings[bindings.len() - 1], VarBinding::new(u16::MAX, 8));
}

#[test]
fn values_skip_missing_and_null_variables() {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Integer).with_value(TriggerValue::Int(5)));
    let mut null = TriggerVar::new(2, VarType::Integer).with_value(TriggerValue::Int(6));
    null.is_null = true;
    script.add_variable(null);
    let inputs = [
        VarBinding::new(1, 1),
        VarBinding::new(2, 2),
        VarBinding::new(3, 3),
    ];
    let bindings = Bindings::new(&inputs, &[]);

    assert_eq!(bindings.value(&script, 1), Some(&TriggerValue::Int(5)));
    assert_eq!(bindings.used_variable_id(&script, 1), Some(1));
    assert_eq!(bindings.value(&script, 2), None);
    assert_eq!(bindings.used_variable_id(&script, 2), None);
    assert_eq!(bindings.value(&script, 3), None);
    assert_eq!(bindings.used_variable_id(&script, 3), None);
}
