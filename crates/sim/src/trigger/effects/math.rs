//! Retail deterministic trigger arithmetic effects.

use super::support::{float_at, integer_at, used_variable_id, variable_is_used};
use super::{EffectOutcome, write_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::World;
use num_traits::ToPrimitive;

const RETAIL_RANDOM_LONG_MAX: u32 = 1 << 30;

pub(super) fn math_count(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(first) = integer_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(operator) = integer_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(second) = integer_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Some(result) = calculate(first, operator, second) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Int(result))
}

pub(super) fn random_count(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let minimum = integer_at(effect, script, 1).unwrap_or(0);
    let maximum = integer_at(effect, script, 2).unwrap_or(RETAIL_RANDOM_LONG_MAX.cast_signed());
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    if minimum >= maximum {
        return EffectOutcome::Skipped;
    }

    let range = maximum.wrapping_sub(minimum).cast_unsigned();
    let value = minimum.wrapping_add(world.trigger_random_index(range).cast_signed());
    write_value(script, output_id, TriggerValue::Int(value))
}

pub(super) fn lerp_count(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(first) = integer_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(second) = integer_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(percent) = float_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let difference = second
        .wrapping_sub(first)
        .to_f32()
        .expect("every retail count has an f32 representation");
    let delta = percent.clamp(0.0, 1.0) * difference;
    write_value(
        script,
        output_id,
        TriggerValue::Int(first.wrapping_add(float_to_i32(delta))),
    )
}

pub(super) fn lerp_percent(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(first) = float_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(second) = float_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(percent) = float_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let result = first + percent.clamp(0.0, 1.0) * (second - first);
    write_value(script, output_id, TriggerValue::Float(result))
}

pub(super) fn lerp_time(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(first) = time_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(second) = time_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(percent) = float_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let difference = second
        .wrapping_sub(first)
        .to_f32()
        .expect("every retail time has an f32 representation");
    let delta = percent.clamp(0.0, 1.0) * difference;
    write_value(
        script,
        output_id,
        TriggerValue::Time(first.wrapping_add(float_to_u32(delta))),
    )
}

pub(super) fn math_float(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(first) = float_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(operator) = integer_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(second) = float_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Some(result) = calculate_float(first, operator, second) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Float(result))
}

pub(super) fn as_float(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    if !matches!(effect.version, 2 | 3) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(output_id) = used_variable_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let result = if variable_is_used(effect, script, 1) {
        integer_at(effect, script, 1)
            .and_then(|value| value.to_f32())
            .unwrap_or(0.0)
    } else if effect.version == 2 && variable_is_used(effect, script, 2) {
        float_at(effect, script, 2).unwrap_or(0.0)
    } else if effect.version == 2 && variable_is_used(effect, script, 3) {
        float_at(effect, script, 3).unwrap_or(0.0)
    } else if effect.version == 2 && variable_is_used(effect, script, 5) {
        float_at(effect, script, 5).unwrap_or(0.0)
    } else {
        0.0
    };
    write_value(script, output_id, TriggerValue::Float(result))
}

pub(super) fn math_time(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(first) = time_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(operator) = integer_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(second) = time_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Some(result) = calculate_time(first, operator, second) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Time(result))
}

pub(super) fn random_time(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let minimum = time_at(effect, script, 1).unwrap_or(0);
    let maximum = time_at(effect, script, 2)
        .unwrap_or(RETAIL_RANDOM_LONG_MAX)
        .min(RETAIL_RANDOM_LONG_MAX);
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    if minimum >= maximum {
        return EffectOutcome::Skipped;
    }

    let value = minimum.wrapping_add(world.trigger_random_index(maximum - minimum));
    write_value(script, output_id, TriggerValue::Time(value))
}

fn calculate(first: i32, operator: i32, second: i32) -> Option<i32> {
    match operator {
        0 => Some(first.wrapping_add(second)),
        1 => Some(first.wrapping_sub(second)),
        2 => Some(first.wrapping_mul(second)),
        3 => Some(first.checked_div(second).unwrap_or(first)),
        4 => Some(first.checked_rem(second).unwrap_or(first)),
        _ => None,
    }
}

fn calculate_float(first: f32, operator: i32, second: f32) -> Option<f32> {
    match operator {
        0 => Some(first + second),
        1 => Some(first - second),
        2 => Some(first * second),
        3 => Some(if second == 0.0 { first } else { first / second }),
        4 => Some(0.0),
        _ => None,
    }
}

fn float_to_i32(value: f32) -> i32 {
    if value.is_nan() {
        return 0;
    }
    value.to_i32().unwrap_or_else(|| {
        if value.is_sign_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}

fn float_to_u32(value: f32) -> u32 {
    if value.is_nan() || value.is_sign_negative() {
        return 0;
    }
    value.to_u32().unwrap_or(u32::MAX)
}

fn time_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<u32> {
    match super::value_at(effect, script, slot)? {
        TriggerValue::Time(value) => Some(*value),
        _ => None,
    }
}

fn calculate_time(first: u32, operator: i32, second: u32) -> Option<u32> {
    match operator {
        0 => Some(first.wrapping_add(second)),
        1 => Some(first.saturating_sub(second)),
        2 => Some(first.wrapping_mul(second)),
        3 => Some(first.checked_div(second).unwrap_or(first)),
        4 => Some(first.checked_rem(second).unwrap_or(first)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
