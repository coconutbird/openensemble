use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};
use crate::world::World;

#[test]
fn math_count_uses_retail_integer_operations_and_zero_divisor_fallback() {
    let mut script = TriggerScript::new(1);
    for (id, value) in [(1, 17), (2, 3), (3, 5), (4, 0)] {
        script.add_variable(
            TriggerVar::new(id, VarType::Integer).with_value(TriggerValue::Int(value)),
        );
    }
    let effect = Effect::new(1, EffectType::MathCount)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4);

    assert_eq!(math_count(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(script.get_variable(4).unwrap().value, TriggerValue::Int(3));

    script.get_variable_mut(3).unwrap().value = TriggerValue::Int(0);
    assert_eq!(math_count(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(script.get_variable(4).unwrap().value, TriggerValue::Int(17));
}

#[test]
fn math_time_clamps_subtraction_and_wraps_unsigned_addition() {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Time).with_value(TriggerValue::Time(u32::MAX)));
    script.add_variable(TriggerVar::new(2, VarType::MathOperator).with_value(TriggerValue::Int(0)));
    script.add_variable(TriggerVar::new(3, VarType::Time).with_value(TriggerValue::Time(2)));
    script.add_variable(TriggerVar::new(4, VarType::Time).with_value(TriggerValue::Time(0)));
    let effect = Effect::new(1, EffectType::MathTime)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4);

    assert_eq!(math_time(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(script.get_variable(4).unwrap().value, TriggerValue::Time(1));

    script.get_variable_mut(1).unwrap().value = TriggerValue::Time(5);
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(1);
    script.get_variable_mut(3).unwrap().value = TriggerValue::Time(5);
    assert_eq!(math_time(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(script.get_variable(4).unwrap().value, TriggerValue::Time(0));
}

#[test]
fn random_time_uses_retail_inclusive_range_and_synchronized_rng() {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Time).with_value(TriggerValue::Time(0)));
    script.add_variable(TriggerVar::new(2, VarType::Time).with_value(TriggerValue::Time(1_000)));
    script.add_variable(TriggerVar::new(3, VarType::Time).with_value(TriggerValue::Time(0)));
    let effect = Effect::new(1, EffectType::RandomTime)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3);
    let mut world = World::new();
    let mut oracle = World::new();

    let expected = oracle.trigger_random_index(1_000);
    assert_eq!(
        random_time(&effect, &mut script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::Time(expected)
    );
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );
}

#[test]
fn random_time_clamps_the_retail_maximum_and_rejects_an_empty_range() {
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::Time).with_value(TriggerValue::Time((1 << 30) - 1)),
    );
    script.add_variable(TriggerVar::new(2, VarType::Time).with_value(TriggerValue::Time(u32::MAX)));
    script.add_variable(TriggerVar::new(3, VarType::Time).with_value(TriggerValue::Time(0)));
    let effect = Effect::new(1, EffectType::RandomTime)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3);
    let mut world = World::new();

    assert_eq!(
        random_time(&effect, &mut script, &mut world),
        EffectOutcome::Applied
    );
    let TriggerValue::Time(value) = script.get_variable(3).unwrap().value else {
        panic!("random-time output was not a time");
    };
    assert!(((1 << 30) - 1..=1 << 30).contains(&value));

    script.get_variable_mut(1).unwrap().value = TriggerValue::Time(1 << 30);
    assert_eq!(
        random_time(&effect, &mut script, &mut world),
        EffectOutcome::Skipped
    );
}

#[test]
fn random_count_uses_retail_inclusive_signed_range() {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Integer).with_value(TriggerValue::Int(-5)));
    script.add_variable(TriggerVar::new(2, VarType::Integer).with_value(TriggerValue::Int(5)));
    script.add_variable(TriggerVar::new(3, VarType::Integer).with_value(TriggerValue::Int(0)));
    let effect = Effect::new(1, EffectType::RandomCount)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3);
    let mut world = World::new();
    let mut oracle = World::new();
    let expected = -5 + oracle.trigger_random_index(10).cast_signed();

    assert_eq!(
        random_count(&effect, &mut script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::Int(expected)
    );
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );
}

#[test]
fn lerp_effects_clamp_control_and_preserve_retail_numeric_types() {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Integer).with_value(TriggerValue::Int(10)));
    script.add_variable(TriggerVar::new(2, VarType::Integer).with_value(TriggerValue::Int(21)));
    script.add_variable(TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(0.5)));
    script.add_variable(TriggerVar::new(4, VarType::Integer).with_value(TriggerValue::Int(0)));
    let count = Effect::new(1, EffectType::LerpCount)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4);
    assert_eq!(lerp_count(&count, &mut script), EffectOutcome::Applied);
    assert_eq!(script.get_variable(4).unwrap().value, TriggerValue::Int(15));

    script.get_variable_mut(1).unwrap().value = TriggerValue::Float(4.0);
    script.get_variable_mut(2).unwrap().value = TriggerValue::Float(8.0);
    script.get_variable_mut(3).unwrap().value = TriggerValue::Float(2.0);
    script.get_variable_mut(4).unwrap().value = TriggerValue::Float(0.0);
    let percent = Effect::new(2, EffectType::LerpPercent)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4);
    assert_eq!(lerp_percent(&percent, &mut script), EffectOutcome::Applied);
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Float(8.0)
    );
}

#[test]
fn lerp_time_uses_retail_unsigned_wrapping_delta() {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Time).with_value(TriggerValue::Time(10)));
    script.add_variable(TriggerVar::new(2, VarType::Time).with_value(TriggerValue::Time(5)));
    script.add_variable(TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(0.5)));
    script.add_variable(TriggerVar::new(4, VarType::Time).with_value(TriggerValue::Time(0)));
    let effect = Effect::new(1, EffectType::LerpTime)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4);

    assert_eq!(lerp_time(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Time(2_147_483_658)
    );
}

#[test]
fn math_float_matches_retail_divide_and_modulus_fallbacks() {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Float).with_value(TriggerValue::Float(7.5)));
    script.add_variable(TriggerVar::new(2, VarType::MathOperator).with_value(TriggerValue::Int(3)));
    script.add_variable(TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(0.0)));
    script.add_variable(TriggerVar::new(4, VarType::Float).with_value(TriggerValue::Float(-1.0)));
    let effect = Effect::new(1, EffectType::MathFloat)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4);

    assert_eq!(math_float(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Float(7.5)
    );
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(4);
    assert_eq!(math_float(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Float(0.0)
    );
}

#[test]
fn as_float_uses_retail_versioned_priority_and_zero_default() {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Integer).with_value(TriggerValue::Int(7)));
    script.add_variable(TriggerVar::new(2, VarType::Float).with_value(TriggerValue::Float(12.5)));
    script.add_variable(TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(0.25)));
    script.add_variable(TriggerVar::new(4, VarType::Float).with_value(TriggerValue::Float(-1.0)));
    script.add_variable(TriggerVar::new(5, VarType::Float).with_value(TriggerValue::Float(99.0)));
    let mut effect = Effect::new(1, EffectType::AsFloat)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4)
        .with_input_at(5, 5);
    effect.version = 2;

    assert_eq!(as_float(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Float(7.0)
    );

    script.get_variable_mut(1).unwrap().is_null = true;
    assert_eq!(as_float(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Float(12.5)
    );

    effect.version = 3;
    assert_eq!(as_float(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Float(0.0)
    );
}
