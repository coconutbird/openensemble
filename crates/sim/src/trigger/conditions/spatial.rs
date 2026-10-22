//! Retail spatial and vector comparison conditions.

use super::queries::unit_xz_surface_distance;
use super::{compare_partial, operator_at, used_variable_id, value_at, write_trigger_value};
use crate::trigger::{Condition, TriggerScript, TriggerValue, TriggerVec3};
use crate::{EntityId, World};
use glam::Vec3;

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

/// `MarkerSquadsInArea` (980, `BTriggerCondition_tcMarkerSquadsInArea` @ `0x140431110`).
///
/// The condition is true if a live squad of the list in signature 1 is no farther than the
/// radius in signature 3 from the location in signature 2. Retail reads these signatures
/// without a null check. A non-null signature 4 gets each squad in range, in list order.
/// With a null signature 4 the search stops at the first squad in range.
pub(super) fn marker_squads_in_area(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    let raw = |signature_id| {
        condition
            .variable_id(signature_id)
            .and_then(|variable_id| script.get_variable(variable_id))
            .map(|variable| &variable.value)
    };
    let Some(TriggerValue::SquadList(squads)) = raw(1) else {
        return false;
    };
    let (Some(location), Some(radius)) = (
        raw(2).and_then(TriggerValue::as_location),
        raw(3).and_then(TriggerValue::as_float),
    ) else {
        return false;
    };
    let point = Vec3::new(location.x, location.y, location.z);
    let output_id = used_variable_id(condition, script, 4);
    let mut in_range = Vec::new();
    for squad_id in squads {
        if world.get_squad(*squad_id).is_none()
            || radius < squad_xz_surface_distance(world, *squad_id, point)
        {
            continue;
        }
        if output_id.is_none() {
            return true;
        }
        in_range.push(*squad_id);
    }
    let found = !in_range.is_empty();
    if let Some(output_id) = output_id {
        write_trigger_value(script, output_id, TriggerValue::SquadList(in_range));
    }
    found
}

/// The XZ distance from a squad to `point` (`BEntity_calculateXZDistanceToPoint`
/// @ `0x1402049A0`).
///
/// A squad with one unit uses the obstruction of that unit. Another squad uses the distance from
/// its center less its obstruction radius. The result is not less than 0.
fn squad_xz_surface_distance(world: &World, squad_id: EntityId, point: Vec3) -> f32 {
    let Some(squad) = world.get_squad(squad_id) else {
        return f32::INFINITY;
    };
    if let [unit_id] = squad.unit_ids.as_slice()
        && let Some(unit) = world.get_unit(*unit_id)
    {
        return unit_xz_surface_distance(unit, point);
    }
    let dx = squad.base.position.x - point.x;
    let dz = squad.base.position.z - point.z;
    (dx.hypot(dz) - world.squad_obstruction_radius(squad_id)).max(0.0)
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
