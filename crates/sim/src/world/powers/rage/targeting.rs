//! Retail Rage retarget scoring and leap setup.

use super::super::common::prototype_lifetime_ms;
use super::execution::{RagePowerExecution, RagePowerPhase, RageSpline};
use crate::EntityId;
use crate::entity::Entity;
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::Database;
use std::f32::consts::FRAC_PI_2;

const DIRECTION_EPSILON_SQUARED: f32 = 0.000_1;

#[derive(Debug, Clone, Copy)]
struct TargetCandidate {
    squad_id: EntityId,
    position: Vec3,
    obstruction_radius: f32,
    combat_value: f32,
}

pub(super) fn begin_jump(
    world: &mut World,
    database: &Database,
    execution: &mut RagePowerExecution,
    direction: Vec3,
) -> bool {
    let Some(target) = select_target(world, database, execution, direction) else {
        execution.target_squad_id = EntityId::INVALID;
        return false;
    };
    if execution.target_squad_id == target.squad_id {
        return false;
    }
    let Some((leader_id, start, owner_radius)) = owner_geometry(world, execution.owner_squad_id)
    else {
        return false;
    };
    let destination = landing_position(world, start, owner_radius, target);
    let middle = spline_middle(execution, start, destination);
    execution.target_squad_id = target.squad_id;
    execution.teleport_destination = destination;
    execution.teleport_remaining = execution.teleport_time;
    execution.retarget_remaining = execution.time_between_retarget;
    execution.spline = RageSpline::through(start, middle, destination);
    execution.phase = RagePowerPhase::Jumping;
    execution.move_input = Vec3::ZERO;
    execution.move_target = start;
    clear_owner_orders(world, execution.owner_squad_id);
    face_target(world, execution.owner_squad_id, leader_id, target.position);
    attach_teleport_effect(world, database, execution, leader_id);
    true
}

fn select_target(
    world: &World,
    database: &Database,
    execution: &RagePowerExecution,
    direction: Vec3,
) -> Option<TargetCandidate> {
    let owner_position = world.get_squad(execution.owner_squad_id)?.base.position;
    let candidates = candidates(
        world,
        database,
        execution.player_id,
        owner_position,
        execution.scan_radius,
    );
    let planar_direction = Vec3::new(direction.x, 0.0, direction.z);
    if planar_direction.length_squared() > DIRECTION_EPSILON_SQUARED {
        select_directional(
            &candidates,
            owner_position,
            planar_direction.normalize(),
            execution.scan_radius,
            execution.distance_vs_angle_weight,
        )
    } else {
        candidates.into_iter().max_by(|left, right| {
            left.combat_value
                .total_cmp(&right.combat_value)
                .then_with(|| right.squad_id.cmp(&left.squad_id))
        })
    }
}

fn candidates(
    world: &World,
    database: &Database,
    player_id: u8,
    owner_position: Vec3,
    scan_radius: f32,
) -> Vec<TargetCandidate> {
    let scan_radius_squared = scan_radius * scan_radius;
    world
        .squads
        .iter()
        .filter_map(|(squad_id, squad)| {
            if !squad.is_alive()
                || squad.base.player_id == player_id
                || world.players_are_allied(squad.base.player_id, player_id)
            {
                return None;
            }
            let leader = squad.unit_ids.iter().find_map(|id| world.get_unit(*id))?;
            let planar_delta = Vec3::new(
                leader.base.position.x - owner_position.x,
                0.0,
                leader.base.position.z - owner_position.z,
            );
            if planar_delta.length_squared() > scan_radius_squared {
                return None;
            }
            leader.is_attackable().then_some(TargetCandidate {
                squad_id,
                position: leader.base.position,
                obstruction_radius: leader.obstruction_radius(),
                combat_value: squad_combat_value(database, &squad.proto_squad_name),
            })
        })
        .collect()
}

fn select_directional(
    candidates: &[TargetCandidate],
    owner_position: Vec3,
    direction: Vec3,
    scan_radius: f32,
    distance_weight: f32,
) -> Option<TargetCandidate> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let delta = Vec3::new(
                candidate.position.x - owner_position.x,
                0.0,
                candidate.position.z - owner_position.z,
            );
            let distance = delta.length();
            if distance >= scan_radius || distance <= f32::EPSILON {
                return None;
            }
            let angle = direction.dot(delta / distance).clamp(-1.0, 1.0).acos();
            if angle >= FRAC_PI_2 {
                return None;
            }
            let score = (distance / scan_radius) * distance_weight
                + (angle / FRAC_PI_2) * (1.0 - distance_weight);
            (score < 1.0).then_some((score, *candidate))
        })
        .min_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.squad_id.cmp(&right.1.squad_id))
        })
        .map(|(_, candidate)| candidate)
}

fn owner_geometry(world: &World, squad_id: EntityId) -> Option<(EntityId, Vec3, f32)> {
    let squad = world.get_squad(squad_id)?;
    let leader_id = squad
        .unit_ids
        .iter()
        .find(|id| world.get_unit(**id).is_some())
        .copied()?;
    let leader = world.get_unit(leader_id)?;
    Some((leader_id, leader.base.position, leader.obstruction_radius()))
}

fn landing_position(
    world: &World,
    owner_position: Vec3,
    owner_radius: f32,
    target: TargetCandidate,
) -> Vec3 {
    let away = Vec3::new(
        owner_position.x - target.position.x,
        0.0,
        owner_position.z - target.position.z,
    )
    .normalize_or_zero();
    let away = if away == Vec3::ZERO { Vec3::Z } else { away };
    let mut destination =
        target.position + away * (target.obstruction_radius + owner_radius).max(0.0);
    if let Some(height) = world.terrain_height(destination, true) {
        destination.y = height;
    }
    destination
}

fn spline_middle(execution: &RagePowerExecution, start: Vec3, end: Vec3) -> Vec3 {
    let planar = Vec3::new(end.x - start.x, 0.0, end.z - start.z);
    let distance = planar.length();
    let offset = execution.teleport_lateral_distance.min(distance * 0.5);
    let mut middle = start + planar.normalize_or_zero() * offset;
    middle.y = end.y.max(middle.y) + execution.teleport_jump_distance;
    middle
}

fn clear_owner_orders(world: &mut World, squad_id: EntityId) {
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad.remove_all_orders();
        squad.mode = crate::entities::SquadMode::Power;
    }
}

fn face_target(world: &mut World, squad_id: EntityId, leader_id: EntityId, target: Vec3) {
    let Some(origin) = world.get_unit(leader_id).map(|unit| unit.base.position) else {
        return;
    };
    let direction = Vec3::new(target.x - origin.x, 0.0, target.z - origin.z).normalize_or_zero();
    if direction == Vec3::ZERO {
        return;
    }
    if let Some(unit) = world.get_unit_mut(leader_id) {
        unit.base.set_forward(direction);
    }
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad.base.set_forward(direction);
    }
}

fn attach_teleport_effect(
    world: &mut World,
    database: &Database,
    execution: &RagePowerExecution,
    leader_id: EntityId,
) {
    let Some(attachment_id) = world.add_prototype_attachment_to_unit(
        database,
        leader_id,
        execution.teleport_attachment_prototype_id,
    ) else {
        return;
    };
    let lifetime = database
        .objects
        .iter()
        .find(|prototype| {
            prototype
                .name
                .eq_ignore_ascii_case(&execution.teleport_attachment_prototype)
        })
        .and_then(prototype_lifetime_ms);
    if let Some(lifetime) = lifetime {
        world
            .power_manager
            .track_visual(attachment_id, world.game_time_ms.wrapping_add(lifetime));
    }
}

fn squad_combat_value(database: &Database, squad_name: &str) -> f32 {
    let Some(squad) = database
        .squads
        .iter()
        .find(|candidate| candidate.name.eq_ignore_ascii_case(squad_name))
    else {
        return 0.0;
    };
    squad
        .units
        .as_ref()
        .map(|units| {
            units
                .entries
                .iter()
                .map(|entry| {
                    let value = database
                        .objects
                        .iter()
                        .find(|object| object.name.eq_ignore_ascii_case(&entry.proto_object))
                        .and_then(|object| object.combat_value)
                        .filter(|value| value.is_finite() && *value > 0.0)
                        .unwrap_or_default();
                    value * nonnegative_count(entry.count)
                })
                .sum()
        })
        .unwrap_or_default()
}

fn nonnegative_count(value: i32) -> f32 {
    let value = u32::try_from(value).unwrap_or_default();
    let high = u16::try_from(value >> 16).unwrap_or_default();
    let low = u16::try_from(value & u32::from(u16::MAX)).unwrap_or_default();
    f32::from(high) * 65_536.0 + f32::from(low)
}
