//! Retail synchronized random-location generation.

use super::super::support::{bool_at, float_at, used_variable_id, variable_is_used, vector_at};
use super::super::{EffectOutcome, write_value};
use crate::World;
use crate::trigger::{Effect, TriggerScript, TriggerValue, TriggerVec3};
use glam::Vec3;

const RETAIL_TWO_PI: f32 = std::f32::consts::TAU;

pub(in crate::trigger::effects) fn random_location(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(input) = vector_at(effect, script, 1).filter(|value| value.is_finite()) else {
        return EffectOutcome::Skipped;
    };
    let Some(inner_radius) = optional_float(effect, script, 2, 0.0) else {
        return EffectOutcome::Skipped;
    };
    let Some(outer_radius) = float_at(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Some(test_obstructions) = optional_bool(effect, script, 5, false) else {
        return EffectOutcome::Skipped;
    };
    let Some(test_pathing) = optional_bool(effect, script, 6, false) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    if test_obstructions || test_pathing {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if !inner_radius.is_finite()
        || !outer_radius.is_finite()
        || inner_radius < 0.0
        || inner_radius > outer_radius
    {
        return EffectOutcome::Skipped;
    }

    let mut output = circular_distribution(world, input, outer_radius, inner_radius);
    if let Some(height) = world.terrain_height(output, true) {
        output.y = height;
    }
    write_value(
        script,
        output_id,
        TriggerValue::Vector(TriggerVec3::new(output.x, output.y, output.z)),
    )
}

fn circular_distribution(
    world: &mut World,
    center: Vec3,
    outer_radius: f32,
    inner_radius: f32,
) -> Vec3 {
    let distribution = world.trigger_random_float(0.0, 1.0);
    let radius = distribution.sqrt() * (outer_radius - inner_radius) + inner_radius;
    let theta = world.trigger_random_float(0.0, RETAIL_TWO_PI);
    let sine = theta.sin();
    let cosine = theta.cos();
    Vec3::new(
        center.x + cosine * radius,
        center.y,
        center.z - sine * radius,
    )
}

fn optional_float(effect: &Effect, script: &TriggerScript, slot: u16, default: f32) -> Option<f32> {
    if variable_is_used(effect, script, slot) {
        float_at(effect, script, slot)
    } else {
        Some(default)
    }
}

fn optional_bool(
    effect: &Effect,
    script: &TriggerScript,
    slot: u16,
    default: bool,
) -> Option<bool> {
    if variable_is_used(effect, script, slot) {
        bool_at(effect, script, slot)
    } else {
        Some(default)
    }
}

#[cfg(test)]
mod tests;
