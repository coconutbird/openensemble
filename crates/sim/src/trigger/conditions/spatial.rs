//! Retail spatial and vector comparison conditions.

use super::{compare_partial, operator_at, value_at, write_trigger_value};
use crate::World;
use crate::trigger::{Condition, TriggerScript, TriggerValue, TriggerVec3};

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

/// `CanGetDesignSpheres` (974, `BTriggerCondition_tcCanGetDesignSpheres` @ `0x140430C50`).
///
/// Walk the scenario design spheres in array order. A sphere matches if the
/// slot 1 type is null, or if its type has the same length and text without
/// case (`BString_compareN` with `_strnicmp`). Overwrite slot 2 with the
/// centers and slot 3 with the radii, also when nothing matches. Return
/// whether a sphere matches.
pub(super) fn can_get_design_spheres(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    let type_name = match value_at(condition, script, 1) {
        Some(TriggerValue::String(value)) => Some(value.clone()),
        Some(_) => return false,
        None => None,
    };
    let mut centers = Vec::new();
    let mut radii = Vec::new();
    for sphere in world.design_spheres() {
        if type_name
            .as_deref()
            .is_none_or(|expected| sphere.type_name.eq_ignore_ascii_case(expected))
        {
            centers.push(TriggerVec3::new(
                sphere.center.x,
                sphere.center.y,
                sphere.center.z,
            ));
            radii.push(sphere.radius);
        }
    }
    let matched = !centers.is_empty();
    if let Some(variable_id) = condition.variable_id(2) {
        write_trigger_value(script, variable_id, TriggerValue::VectorList(centers));
    }
    if let Some(variable_id) = condition.variable_id(3) {
        write_trigger_value(script, variable_id, TriggerValue::FloatList(radii));
    }
    matched
}

#[cfg(test)]
mod tests;
