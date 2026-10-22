//! Retail location arithmetic effects of the skirmish AI script.
//!
//! The effects compute in the SSE lanes of `BVector`. The helpers keep the retail order of the
//! float additions so that the results are bit-identical.

use super::{EffectOutcome, write_value};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue, TriggerVec3};
use crate::world::{World, WorldBounds};

/// The distance that `LocationAdjustDir` keeps inside the playable bounds.
const ADJUST_DIR_MARGIN: f32 = 0.01;

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::LerpLocation => lerp_location(effect, script),
        EffectType::MathLocation => math_location(effect, script),
        EffectType::LocationAdjustDir => location_adjust_dir(effect, script, world),
        _ => return None,
    };
    Some(outcome)
}

/// `LerpLocation` (294), inline case `0x126` of `BTriggerEffect_fire`.
///
/// Signature 4 gets `(b - a) * t + a` for the locations `a` and `b` in signatures 1 and 2. The
/// effect clamps `t` in signature 3 to 0 to 1. Retail reads and writes without a null check.
fn lerp_location(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let (Some(first), Some(second), Some(fraction)) = (
        raw_location(effect, script, 1),
        raw_location(effect, script, 2),
        raw_value(effect, script, 3).and_then(TriggerValue::as_float),
    ) else {
        return EffectOutcome::Skipped;
    };
    // `f32::clamp` keeps NaN, like the retail compares.
    let fraction = fraction.clamp(0.0, 1.0);
    let lerp = |low: f32, high: f32| (high - low) * fraction + low;
    let output = TriggerVec3::new(
        lerp(first.x, second.x),
        lerp(first.y, second.y),
        lerp(first.z, second.z),
    );
    write_raw_location(effect, script, 4, output)
}

/// `MathLocation` (295), inline case `0x127` of `BTriggerEffect_fire`.
///
/// Signature 4 gets the sum (operator 0) or the difference (operator 1) of the locations in
/// signatures 1 and 3. The operator is in signature 2. For another operator, retail writes an
/// uninitialized stack value. The sim does not write a value for it.
fn math_location(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let (Some(first), Some(operator), Some(second)) = (
        raw_location(effect, script, 1),
        raw_value(effect, script, 2).and_then(TriggerValue::as_int),
        raw_location(effect, script, 3),
    ) else {
        return EffectOutcome::Skipped;
    };
    let output = match operator {
        0 => TriggerVec3::new(first.x + second.x, first.y + second.y, first.z + second.z),
        1 => TriggerVec3::new(first.x - second.x, first.y - second.y, first.z - second.z),
        _ => return EffectOutcome::Skipped,
    };
    write_raw_location(effect, script, 4, output)
}

/// `LocationAdjustDir` (554, `BTriggerEffect_teLocationAdjustDir` @ `0x14046A9E0`).
///
/// Signature 4 gets the location in signature 1 moved by the distance in signature 3 along the
/// normalized direction in signature 2. If the result is outside the simulation bounds
/// (`gWorld + 1192`), the effect shortens the distance so that the result is just inside them.
/// Retail reads and writes without a null check.
fn location_adjust_dir(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let (Some(origin), Some(direction), Some(distance)) = (
        raw_location(effect, script, 1),
        raw_location(effect, script, 2),
        raw_value(effect, script, 3).and_then(TriggerValue::as_float),
    ) else {
        return EffectOutcome::Skipped;
    };
    let output = adjust_dir(
        origin,
        direction,
        distance,
        world.effective_playable_bounds(),
    );
    write_raw_location(effect, script, 4, output)
}

fn adjust_dir(
    origin: TriggerVec3,
    direction: TriggerVec3,
    distance: f32,
    bounds: Option<WorldBounds>,
) -> TriggerVec3 {
    let length_squared =
        (direction.x * direction.x + direction.y * direction.y) + direction.z * direction.z;
    let direction = normalize(direction, length_squared);
    let target = add_scaled(origin, direction, distance);
    let Some(bounds) = bounds else {
        return target;
    };
    let (min_x, min_z, max_x, max_z) = (
        bounds.min_x(),
        bounds.min_z(),
        bounds.max_x(),
        bounds.max_z(),
    );
    if min_x <= target.x && target.x <= max_x && min_z <= target.z && target.z <= max_z {
        return target;
    }
    let mut edge = target;
    if min_x > target.x && min_z > target.z {
        if target.z <= target.x {
            edge.x = min_x;
        } else {
            edge.z = min_z;
        }
    } else if target.x > max_x && target.z > max_z {
        if target.x <= target.z {
            edge.x = max_x;
        } else {
            edge.z = max_z;
        }
    } else {
        edge.x = clamp_low_first(target.x, min_x, max_x);
        edge.z = clamp_low_first(target.z, min_z, max_z);
    }
    let delta = TriggerVec3::new(edge.x - target.x, edge.y - target.y, edge.z - target.z);
    let overshoot = ((delta.x * delta.x + delta.z * delta.z) + delta.y * delta.y).sqrt();
    let delta_length_squared = (delta.x * delta.x + delta.y * delta.y) + delta.z * delta.z;
    let toward_edge = normalize(delta, delta_length_squared);
    let cosine = (toward_edge.x * (0.0 - direction.x) + toward_edge.y * (0.0 - direction.y))
        + toward_edge.z * (0.0 - direction.z);
    let distance = distance - (overshoot / cosine + ADJUST_DIR_MARGIN);
    add_scaled(origin, direction, distance)
}

/// Clamp like the retail branch order: below the minimum first, then above the maximum.
fn clamp_low_first(value: f32, minimum: f32, maximum: f32) -> f32 {
    if minimum <= value {
        if value <= maximum { value } else { maximum }
    } else {
        minimum
    }
}

/// `BVector` normalize of the effect: an infinite squared length gives NaN, a zero length gives
/// the zero vector.
fn normalize(vector: TriggerVec3, length_squared: f32) -> TriggerVec3 {
    if length_squared == f32::INFINITY {
        return TriggerVec3::new(f32::NAN, f32::NAN, f32::NAN);
    }
    let length = length_squared.sqrt();
    if length == 0.0 {
        return TriggerVec3::new(0.0, 0.0, 0.0);
    }
    TriggerVec3::new(vector.x / length, vector.y / length, vector.z / length)
}

fn add_scaled(origin: TriggerVec3, direction: TriggerVec3, scale: f32) -> TriggerVec3 {
    TriggerVec3::new(
        scale * direction.x + origin.x,
        scale * direction.y + origin.y,
        scale * direction.z + origin.z,
    )
}

fn raw_value<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
) -> Option<&'a TriggerValue> {
    let variable_id = effect.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .map(|variable| &variable.value)
}

fn raw_location(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<TriggerVec3> {
    raw_value(effect, script, signature_id).and_then(TriggerValue::as_location)
}

fn write_raw_location(
    effect: &Effect,
    script: &mut TriggerScript,
    signature_id: u16,
    value: TriggerVec3,
) -> EffectOutcome {
    let Some(output_id) = effect.variable_id(signature_id) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Vector(value))
}

#[cfg(test)]
mod tests;
