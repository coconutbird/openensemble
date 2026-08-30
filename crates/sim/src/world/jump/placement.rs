//! Retail-style unobstructed landing search used by squad Jump actions.

use super::super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use glam::Vec3;
use std::collections::BTreeSet;

const MINIMUM_SQUAD_RADIUS: f32 = 0.2;
const SEARCH_SPACING_SCALE: f32 = 1.1;
const SEARCH_GROW_ATTEMPTS: usize = 4;

pub(super) fn find_landing_position(
    world: &World,
    center: Vec3,
    target_radius: f32,
    squad_radius: f32,
    excluded: &BTreeSet<EntityId>,
) -> Option<Vec3> {
    let search_radius = squad_radius.max(MINIMUM_SQUAD_RADIUS);
    if position_is_clear(world, center, search_radius, excluded, false) {
        return Some(center);
    }

    let mut attempts = 1usize;
    let mut obstruction_radius = finite_nonnegative(target_radius);
    while attempts <= SEARCH_GROW_ATTEMPTS {
        let candidates = perimeter_candidates(obstruction_radius, search_radius, center);
        if let Some(candidate) = candidates
            .into_iter()
            .filter(|candidate| position_is_clear(world, *candidate, search_radius, excluded, true))
            .min_by(|left, right| {
                planar_distance_squared(*left, center)
                    .total_cmp(&planar_distance_squared(*right, center))
            })
        {
            return Some(candidate);
        }
        attempts += 1;
        obstruction_radius += finite_nonnegative(squad_radius);
    }
    None
}

pub(super) fn clamp_inside_playable_bounds(world: &World, mut position: Vec3) -> Vec3 {
    let Some(bounds) = world.effective_playable_bounds() else {
        return position;
    };
    let minimum_x = (bounds.min_x() + 1.0).min(bounds.max_x());
    let minimum_z = (bounds.min_z() + 1.0).min(bounds.max_z());
    let maximum_x = (bounds.max_x() - 1.0).max(bounds.min_x());
    let maximum_z = (bounds.max_z() - 1.0).max(bounds.min_z());
    position.x = position.x.clamp(minimum_x, maximum_x);
    position.z = position.z.clamp(minimum_z, maximum_z);
    position
}

fn perimeter_candidates(obstruction_radius: f32, squad_radius: f32, center: Vec3) -> Vec<Vec3> {
    let forward = Vec3::X;
    let right = Vec3::Z;
    let obstruction_size = obstruction_radius * 2.0;
    let delta = squad_radius * 2.0 * SEARCH_SPACING_SCALE;
    let reset = (obstruction_radius + squad_radius) * SEARCH_SPACING_SCALE;
    let mut candidates = Vec::new();

    let mut position = center + forward * reset - right * reset;
    candidates.push(position);
    let final_corner = position;
    add_source_edge(
        &mut candidates,
        &mut position,
        right * delta,
        obstruction_size,
        delta,
        center + forward * reset + right * reset,
    );
    add_source_edge(
        &mut candidates,
        &mut position,
        -forward * delta,
        obstruction_size,
        delta,
        center - forward * reset + right * reset,
    );
    add_source_edge(
        &mut candidates,
        &mut position,
        -right * delta,
        obstruction_size,
        delta,
        center - forward * reset - right * reset,
    );
    add_source_edge(
        &mut candidates,
        &mut position,
        forward * delta,
        obstruction_size,
        delta,
        final_corner,
    );
    candidates
}

fn add_source_edge(
    candidates: &mut Vec<Vec3>,
    position: &mut Vec3,
    step: Vec3,
    edge_distance: f32,
    step_distance: f32,
    next_corner: Vec3,
) {
    let mut traveled = 0.0;
    loop {
        *position += step;
        traveled += step_distance;
        if traveled >= edge_distance {
            *position = next_corner;
            candidates.push(*position);
            break;
        }
        candidates.push(*position);
    }
}

fn position_is_clear(
    world: &World,
    position: Vec3,
    radius: f32,
    excluded: &BTreeSet<EntityId>,
    check_bounds: bool,
) -> bool {
    if !position.is_finite() || (check_bounds && world.is_outside_playable_bounds(position, true)) {
        return false;
    }
    !world.units.iter().any(|(unit_id, unit)| {
        !excluded.contains(&unit_id)
            && unit.is_alive()
            && !unit.is_garrisoned()
            && circle_overlaps_box(
                position,
                radius,
                unit.base.position,
                unit.obstruction_half_extents,
            )
    })
}

fn circle_overlaps_box(center: Vec3, radius: f32, box_center: Vec3, half_extents: Vec3) -> bool {
    let extents = half_extents.abs();
    let closest_x = center
        .x
        .clamp(box_center.x - extents.x, box_center.x + extents.x);
    let closest_z = center
        .z
        .clamp(box_center.z - extents.z, box_center.z + extents.z);
    let delta_x = center.x - closest_x;
    let delta_z = center.z - closest_z;
    delta_x * delta_x + delta_z * delta_z <= radius * radius
}

fn planar_distance_squared(left: Vec3, right: Vec3) -> f32 {
    let delta = left - right;
    delta.x * delta.x + delta.z * delta.z
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}
