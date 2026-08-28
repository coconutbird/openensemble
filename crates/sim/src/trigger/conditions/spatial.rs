//! Retail spatial and vector comparison conditions.

use super::{compare_partial, operator_at, value_at};
use crate::World;
use crate::trigger::{Condition, TriggerScript, TriggerValue};

const DEFAULT_VECTOR_EPSILON: f32 = 0.000_001;

pub(super) fn squad_location_distance(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
) -> bool {
    let Some(squad_id) = value_at(condition, script, 1).and_then(TriggerValue::as_entity) else {
        return false;
    };
    let Some(location) = value_at(condition, script, 2).and_then(TriggerValue::as_location) else {
        return false;
    };
    let Some(operator) = operator_at(condition, script, 3) else {
        return false;
    };
    let Some(distance) = value_at(condition, script, 4).and_then(TriggerValue::as_float) else {
        return false;
    };
    let Some(squad) = world.get_squad(squad_id) else {
        return false;
    };
    let dx = squad.base.position.x - location.x;
    let dz = squad.base.position.z - location.z;
    compare_partial(&dx.hypot(dz), operator, &distance)
}

pub(super) fn compare_vector(condition: &Condition, script: &TriggerScript) -> bool {
    let Some(first) = value_at(condition, script, 1).and_then(TriggerValue::as_location) else {
        return false;
    };
    let Some(second) = value_at(condition, script, 2).and_then(TriggerValue::as_location) else {
        return false;
    };
    let epsilon = value_at(condition, script, 3)
        .and_then(TriggerValue::as_float)
        .unwrap_or(DEFAULT_VECTOR_EPSILON);
    (first.x - second.x).abs() <= epsilon
        && (first.y - second.y).abs() <= epsilon
        && (first.z - second.z).abs() <= epsilon
}

#[cfg(test)]
mod tests;
