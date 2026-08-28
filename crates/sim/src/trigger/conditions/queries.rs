//! Retail unit and squad query conditions.

use super::{Condition, TriggerScript, TriggerValue, as_f32, as_i32, as_object_type, value_at};
use crate::entities::Unit;
use crate::entity::Entity;
use crate::trigger::VarId;
use crate::{EntityId, World};
use glam::Vec3;

#[derive(Debug, Clone)]
struct CommonQuery {
    players: Vec<i32>,
    player_filter_used: bool,
    object_type: Option<String>,
    filter_list: Option<Vec<EntityId>>,
    area: SearchArea,
}

#[derive(Debug, Clone, Copy)]
enum SearchArea {
    Global,
    Sphere {
        center: Vec3,
        radius: f32,
    },
    Box {
        center: Vec3,
        forward: Vec3,
        right: Vec3,
        half_extents: Vec3,
    },
}

pub(super) fn can_get_units(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    if !matches!(condition.version, 5 | 6) {
        return false;
    }
    let query = read_common_query(condition, script, 3, 4, 9, 10, [11, 13, 14, 15]);
    let mut results = match (condition.version, query.area) {
        (5, SearchArea::Global) => collect_units_v5(world, &query),
        (6, SearchArea::Global) => collect_units_v6(world, &query),
        (_, _) => collect_units_in_area(world, &query),
    };
    if condition.version == 6 {
        remove_unordered(&mut results, |unit_id| {
            world.get_unit(*unit_id).is_some_and(Entity::is_alive)
                && query
                    .filter_list
                    .as_ref()
                    .is_none_or(|filter| filter.contains(unit_id))
        });
    } else if let Some(filter) = &query.filter_list {
        results.retain(|unit_id| filter.contains(unit_id));
    }
    write_query_outputs(condition, script, &results, 6, 8, TriggerValue::UnitList);
    !results.is_empty()
}

pub(super) fn can_get_squads(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    if !matches!(condition.version, 8 | 9) {
        return false;
    }
    let query = read_common_query(condition, script, 3, 11, 10, 12, [15, 16, 17, 18]);
    let prototype_id = value_at(condition, script, 6).and_then(as_i32);
    let ignore_air = condition.version == 9
        && value_at(condition, script, 19)
            .and_then(TriggerValue::as_bool)
            .unwrap_or(false);
    let mut results = match (condition.version, query.area) {
        (8, SearchArea::Global) => collect_squads_v8(world, &query, prototype_id),
        (9, SearchArea::Global) => collect_squads_v9(world, &query, prototype_id),
        (_, _) => collect_squads_in_area(world, &query),
    };
    let qualifies = |squad_id: &EntityId| {
        squad_qualifies(
            world,
            *squad_id,
            prototype_id,
            query.filter_list.as_deref(),
            query.object_type.as_deref(),
            ignore_air,
        )
    };
    if condition.version == 9 && matches!(query.area, SearchArea::Global) {
        remove_unordered(&mut results, qualifies);
    } else {
        results.retain(qualifies);
    }
    write_query_outputs(condition, script, &results, 7, 9, TriggerValue::SquadList);
    !results.is_empty()
}

fn read_common_query(
    condition: &Condition,
    script: &TriggerScript,
    player_slot: u16,
    object_type_slot: u16,
    filter_list_slot: u16,
    player_list_slot: u16,
    box_slots: [u16; 4],
) -> CommonQuery {
    let mut players = match value_at(condition, script, player_list_slot) {
        Some(TriggerValue::PlayerList(values)) => values.clone(),
        _ => Vec::new(),
    };
    let player_filter_used = value_at(condition, script, player_slot).is_some()
        || value_at(condition, script, player_list_slot).is_some();
    if let Some(player_id) = value_at(condition, script, player_slot).and_then(as_i32)
        && !players.contains(&player_id)
    {
        players.push(player_id);
    }
    let object_type = value_at(condition, script, object_type_slot)
        .and_then(as_object_type)
        .map(str::to_owned);
    let filter_list = match value_at(condition, script, filter_list_slot) {
        Some(TriggerValue::UnitList(values) | TriggerValue::SquadList(values)) => {
            Some(values.clone())
        }
        _ => None,
    };
    CommonQuery {
        players,
        player_filter_used,
        object_type,
        filter_list,
        area: read_search_area(condition, script, box_slots),
    }
}

fn read_search_area(
    condition: &Condition,
    script: &TriggerScript,
    box_slots: [u16; 4],
) -> SearchArea {
    let center = value_at(condition, script, 1).and_then(as_vec3);
    if let (Some(center), Some(radius)) = (center, value_at(condition, script, 2).and_then(as_f32))
    {
        return SearchArea::Sphere { center, radius };
    }
    let [facing_slot, width_slot, height_slot, depth_slot] = box_slots;
    let Some((center, (forward, width, height, depth))) = center.zip(
        value_at(condition, script, facing_slot)
            .and_then(as_vec3)
            .zip(value_at(condition, script, width_slot).and_then(as_f32))
            .zip(value_at(condition, script, height_slot).and_then(as_f32))
            .zip(value_at(condition, script, depth_slot).and_then(as_f32))
            .map(|(((forward, width), height), depth)| (forward, width, height, depth)),
    ) else {
        return SearchArea::Global;
    };
    let forward = forward.normalize_or_zero();
    let right = Vec3::Y.cross(forward).normalize_or_zero();
    SearchArea::Box {
        center: center + Vec3::Y * height,
        forward,
        right,
        half_extents: Vec3::new(width, height, depth),
    }
}

fn collect_units_v5(world: &World, query: &CommonQuery) -> Vec<EntityId> {
    world
        .units
        .iter()
        .filter_map(|(unit_id, unit)| {
            (unit.is_alive()
                && player_matches(unit.base.player_id, query, false)
                && object_type_matches(unit, query.object_type.as_deref()))
            .then_some(unit_id)
        })
        .collect()
}

fn collect_units_v6(world: &World, query: &CommonQuery) -> Vec<EntityId> {
    let mut results = Vec::new();
    for player in world.players() {
        if !player_matches(player.id, query, true) {
            continue;
        }
        results.extend(world.units.iter().filter_map(|(unit_id, unit)| {
            (unit.base.player_id == player.id
                && object_type_matches(unit, query.object_type.as_deref()))
            .then_some(unit_id)
        }));
    }
    results
}

fn collect_units_in_area(world: &World, query: &CommonQuery) -> Vec<EntityId> {
    world
        .units
        .iter()
        .filter_map(|(unit_id, unit)| {
            (unit.is_alive()
                && player_matches(unit.base.player_id, query, false)
                && object_type_matches(unit, query.object_type.as_deref())
                && unit_intersects_area(unit, query.area))
            .then_some(unit_id)
        })
        .collect()
}

fn collect_squads_v8(
    world: &World,
    query: &CommonQuery,
    prototype_id: Option<i32>,
) -> Vec<EntityId> {
    world
        .squads
        .iter()
        .filter_map(|(squad_id, squad)| {
            (squad.is_alive()
                && player_matches(squad.base.player_id, query, false)
                && prototype_id.is_none_or(|expected| squad.proto_squad_id == expected))
            .then_some(squad_id)
        })
        .collect()
}

fn collect_squads_v9(
    world: &World,
    query: &CommonQuery,
    prototype_id: Option<i32>,
) -> Vec<EntityId> {
    let mut results = Vec::new();
    for player in world.players() {
        if !player_matches(player.id, query, true) {
            continue;
        }
        results.extend(world.squads.iter().filter_map(|(squad_id, squad)| {
            (squad.base.player_id == player.id
                && prototype_id.is_none_or(|expected| squad.proto_squad_id == expected))
            .then_some(squad_id)
        }));
    }
    results
}

fn collect_squads_in_area(world: &World, query: &CommonQuery) -> Vec<EntityId> {
    world
        .squads
        .iter()
        .filter_map(|(squad_id, squad)| {
            let leader = squad
                .unit_ids
                .iter()
                .find_map(|unit_id| world.get_unit(*unit_id));
            (squad.is_alive()
                && player_matches(squad.base.player_id, query, false)
                && leader.is_some_and(|unit| {
                    object_type_matches(unit, query.object_type.as_deref())
                        && unit_intersects_area(unit, query.area)
                }))
            .then_some(squad_id)
        })
        .collect()
}

fn squad_qualifies(
    world: &World,
    squad_id: EntityId,
    prototype_id: Option<i32>,
    filter_list: Option<&[EntityId]>,
    object_type: Option<&str>,
    ignore_air: bool,
) -> bool {
    let Some(squad) = world.get_squad(squad_id) else {
        return false;
    };
    squad.is_alive()
        && prototype_id.is_none_or(|expected| squad.proto_squad_id == expected)
        && filter_list.is_none_or(|filter| filter.contains(&squad_id))
        && object_type.is_none_or(|expected| {
            squad.unit_ids.iter().all(|unit_id| {
                world
                    .get_unit(*unit_id)
                    .is_some_and(|unit| unit.is_object_type(expected))
            })
        })
        && (!ignore_air || !squad_is_airborne(world, squad_id))
}

fn squad_is_airborne(world: &World, squad_id: EntityId) -> bool {
    world
        .get_squad(squad_id)
        .and_then(|squad| {
            squad
                .unit_ids
                .iter()
                .find_map(|unit_id| world.get_unit(*unit_id))
        })
        .is_none_or(|leader| leader.flying)
}

fn player_matches(player_id: u8, query: &CommonQuery, strict_empty: bool) -> bool {
    if query.players.is_empty() {
        return !strict_empty || !query.player_filter_used;
    }
    query.players.contains(&i32::from(player_id))
}

fn object_type_matches(unit: &Unit, object_type: Option<&str>) -> bool {
    object_type.is_none_or(|expected| unit.is_object_type(expected))
}

fn unit_intersects_area(unit: &Unit, area: SearchArea) -> bool {
    match area {
        SearchArea::Global => true,
        SearchArea::Sphere { center, radius } => unit_intersects_sphere(unit, center, radius),
        SearchArea::Box {
            center,
            forward,
            right,
            half_extents,
        } => unit_intersects_box(unit, center, forward, right, half_extents),
    }
}

fn unit_intersects_sphere(unit: &Unit, center: Vec3, radius: f32) -> bool {
    if !center.is_finite() || !radius.is_finite() || radius < 0.0 {
        return false;
    }
    let delta = Vec3::new(
        (unit.base.position.x - center.x).abs(),
        0.0,
        (unit.base.position.z - center.z).abs(),
    );
    let distance = if unit.is_building() {
        let outside = (delta - unit.obstruction_half_extents.abs()).max(Vec3::ZERO);
        outside.x.hypot(outside.z)
    } else {
        let obstruction_radius = unit
            .obstruction_half_extents
            .x
            .abs()
            .max(unit.obstruction_half_extents.z.abs());
        (delta.x.hypot(delta.z) - obstruction_radius).max(0.0)
    };
    distance <= radius
}

fn unit_intersects_box(
    unit: &Unit,
    center: Vec3,
    forward: Vec3,
    right: Vec3,
    half_extents: Vec3,
) -> bool {
    if !center.is_finite()
        || !forward.is_finite()
        || !right.is_finite()
        || !half_extents.is_finite()
        || forward == Vec3::ZERO
        || right == Vec3::ZERO
        || half_extents.min_element() < 0.0
    {
        return false;
    }
    let delta = unit.base.position - center;
    let object_extents = unit.obstruction_half_extents.abs();
    [
        (right, half_extents.x),
        (Vec3::Y, half_extents.y),
        (forward, half_extents.z),
    ]
    .into_iter()
    .all(|(axis, query_extent)| {
        let object_extent = axis.abs().dot(object_extents);
        delta.dot(axis).abs() <= query_extent + object_extent
    })
}

fn write_query_outputs(
    condition: &Condition,
    script: &mut TriggerScript,
    results: &[EntityId],
    count_slot: u16,
    list_slot: u16,
    wrap_list: fn(Vec<EntityId>) -> TriggerValue,
) {
    if let Some(variable_id) = used_variable_id(condition, script, count_slot) {
        super::write_trigger_value(
            script,
            variable_id,
            TriggerValue::Int(i32::try_from(results.len()).unwrap_or(i32::MAX)),
        );
    }
    if let Some(variable_id) = used_variable_id(condition, script, list_slot) {
        super::write_trigger_value(script, variable_id, wrap_list(results.to_vec()));
    }
}

fn used_variable_id(
    condition: &Condition,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<VarId> {
    let variable_id = condition.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .is_some_and(|variable| !variable.is_null)
        .then_some(variable_id)
}

fn as_vec3(value: &TriggerValue) -> Option<Vec3> {
    value
        .as_location()
        .map(|value| Vec3::new(value.x, value.y, value.z))
}

fn remove_unordered<T>(values: &mut Vec<T>, keep: impl Fn(&T) -> bool) {
    let mut index = 0;
    while index < values.len() {
        if keep(&values[index]) {
            index += 1;
        } else {
            values.swap_remove(index);
        }
    }
}

#[cfg(test)]
mod tests;
