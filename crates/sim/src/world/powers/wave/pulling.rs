//! Gravity-ball scan, pickup queue, and deterministic spring action.

use super::super::common::create_power_visual;
use super::{
    QueuedWaveObject, WaveCapturedObject, WaveGravityBallState, WavePowerExecution, ball_position,
    combat, debris, owner_leader_id,
};
use crate::EntityId;
use crate::entities::squads::formation_offset_to_local;
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::physics::PHYSICS_GRAVITY;
use crate::world::World;
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

const PULLING_RANGE_BUFFER_MULTIPLIER: f32 = 1.5;

enum PullResult {
    Done,
    Pending,
}

pub(super) fn update(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    dt: f32,
) {
    update_queued_objects(world, database, execution);
    validate_captured_objects(world, execution);
    pull_captured_objects(world, execution, dt);
    execution.state = if execution.captured_objects.len()
        >= execution.profile.maximum_captured_objects
        && execution.current_explosion_damage_bank
            >= execution.maximum_possible_explosion_damage_bank
    {
        WaveGravityBallState::PullingFull
    } else {
        WaveGravityBallState::Pulling
    };
    if execution.elapsed_seconds <= execution.next_tick_time {
        return;
    }
    process_ticks(world, database, gameplay, execution, dt);
}

fn process_ticks(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    dt: f32,
) {
    let ball = ball_position(world, execution);
    let leader_id = owner_leader_id(world, execution.owner_squad_id);
    let scan_range = execution.profile.pulling_range * PULLING_RANGE_BUFFER_MULTIPLIER;
    let units_in_area = world
        .units
        .iter()
        .filter_map(|(unit_id, unit)| {
            (unit.base.position.distance(ball) - unit.obstruction_radius() <= scan_range)
                .then_some(unit_id)
        })
        .collect::<Vec<_>>();
    let mut units_to_pull_next = Vec::new();
    let mut first_tick = true;
    while execution.elapsed_seconds > execution.next_tick_time {
        for unit_id in &units_in_area {
            let result =
                attempt_pull_unit(world, gameplay, execution, *unit_id, leader_id, ball, dt);
            if first_tick && matches!(result, PullResult::Pending) {
                units_to_pull_next.push(*unit_id);
            }
        }
        execution.next_tick_time += execution.profile.tick_length;
        first_tick = false;
        let did_damage =
            combat::update_lightning(world, database, gameplay, execution, &units_to_pull_next);
        if did_damage {
            increase_damage_bank(execution);
            if !execution.ignore_requirements && !pay_upkeep(world, execution) {
                execution.explosion_requested = true;
                break;
            }
        }
    }
    units_to_pull_next.sort_unstable();
    units_to_pull_next.dedup();
    super::replace_units_to_pull(world, execution, units_to_pull_next);
}

fn attempt_pull_unit(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    unit_id: EntityId,
    leader_id: EntityId,
    ball: Vec3,
    dt: f32,
) -> PullResult {
    if unit_id == leader_id
        || unit_id == execution.ball_object_id
        || execution
            .captured_objects
            .iter()
            .any(|captured| captured.unit_id == unit_id)
    {
        return PullResult::Done;
    }
    let Some(unit) = world.get_unit(unit_id) else {
        return PullResult::Done;
    };
    if unit.is_invulnerable() {
        return PullResult::Done;
    }
    let distance = unit.base.position.distance(ball) - unit.obstruction_radius();
    if distance > execution.profile.pulling_range * PULLING_RANGE_BUFFER_MULTIPLIER {
        return PullResult::Done;
    }
    if distance > execution.profile.pulling_range {
        return PullResult::Pending;
    }
    if capture_unit(world, gameplay, execution, unit_id) {
        return PullResult::Done;
    }
    if world.get_unit(unit_id).is_none_or(|unit| !unit.is_alive()) {
        return PullResult::Pending;
    }
    let random = world.trigger_random_index(32_767);
    if random % 255 < u32::from(execution.profile.throw_part_chance_pulling) {
        let _part = debris::rip_part_off_unit(world, gameplay, execution, unit_id, dt);
    }
    PullResult::Pending
}

fn capture_unit(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    unit_id: EntityId,
) -> bool {
    let Some((alive, hitpoints, physics_replacement)) = world.get_unit(unit_id).map(|unit| {
        (
            unit.is_alive(),
            unit.hitpoints,
            unit.is_physics_replacement(),
        )
    }) else {
        return true;
    };
    if physics_replacement {
        queue_add_object(world, gameplay, execution, unit_id);
        return true;
    }
    if alive && hitpoints > execution.profile.health_to_capture {
        return false;
    }
    if let Some(replacement_id) = create_physics_replacement(world, gameplay, unit_id) {
        queue_add_object(world, gameplay, execution, replacement_id);
    } else if alive {
        let _killed = world.kill_unit(unit_id, false);
    }
    true
}

fn create_physics_replacement(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    unit_id: EntityId,
) -> Option<EntityId> {
    let source = world.get_unit(unit_id)?.clone();
    let profile = gameplay?
        .physics_replacement(&source.proto_object_name)?
        .clone();
    let ground_height = world
        .terrain_height(source.base.position, true)
        .unwrap_or(source.base.position.y.min(0.0));
    let replacement_id = world.units.allocate_id();
    let replacement = source.create_physics_replacement(replacement_id, &profile, ground_height);
    let _removed = world.remove_unit(unit_id)?;
    world.units.insert(replacement_id, replacement);
    Some(replacement_id)
}

fn physics_valid_for_pulling(unit: &crate::Unit) -> bool {
    if unit.is_object_type("Infantry") {
        return true;
    }
    unit.physics.as_ref().is_some_and(|physics| {
        let extents = physics.collider().half_extents;
        let sum = extents.x + extents.y + extents.z;
        sum > 1.0 && sum < 8.0
    })
}

fn queue_add_object(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    unit_id: EntityId,
) {
    if execution.captured_objects.len() >= execution.profile.maximum_captured_objects
        || execution
            .queued_pickup_objects
            .iter()
            .any(|queued| queued.unit_id == unit_id)
    {
        return;
    }
    let Some(unit) = world.get_unit(unit_id) else {
        return;
    };
    if unit.physics.is_none() || !physics_valid_for_pulling(unit) {
        return;
    }
    let is_clamshell = gameplay
        .and_then(|catalog| catalog.physics_replacement(&unit.proto_object_name))
        .is_some_and(crate::gameplay::PhysicsReplacementProfile::is_clamshell);
    if is_clamshell
        && let Some(body) = world
            .get_unit_mut(unit_id)
            .and_then(|unit| unit.physics.as_mut())
    {
        body.set_linear_damping(0.1);
    }
    let last_add_time = execution
        .queued_pickup_objects
        .last()
        .map_or(execution.elapsed_seconds, |queued| queued.add_time);
    let delay = world.trigger_random_float(0.0, execution.profile.pickup_object_rate);
    execution.queued_pickup_objects.push(QueuedWaveObject {
        unit_id,
        add_time: last_add_time + delay,
    });
}

fn update_queued_objects(
    world: &mut World,
    database: &Database,
    execution: &mut WavePowerExecution,
) {
    let ready = execution
        .queued_pickup_objects
        .iter()
        .take_while(|queued| queued.add_time <= execution.elapsed_seconds)
        .count();
    let queued = execution
        .queued_pickup_objects
        .drain(..ready)
        .collect::<Vec<_>>();
    for queued in queued {
        add_object_to_ball(world, database, execution, queued.unit_id);
    }
}

fn add_object_to_ball(
    world: &mut World,
    database: &Database,
    execution: &mut WavePowerExecution,
    unit_id: EntityId,
) {
    if execution
        .captured_objects
        .iter()
        .any(|captured| captured.unit_id == unit_id)
    {
        return;
    }
    let Some((position, center, forward, mass, color_player_id, was_auto_attackable)) =
        world.get_unit(unit_id).and_then(|unit| {
            unit.physics.as_ref().map(|physics| {
                (
                    unit.base.position,
                    unit.base.position + physics.collider().center_offset,
                    unit.base.forward,
                    physics.material().mass,
                    unit.base.player_id,
                    unit.auto_attackable_setting(),
                )
            })
        })
    else {
        return;
    };
    let attachment_id = create_power_visual(
        world,
        database,
        execution.player_id,
        center,
        forward,
        &execution.profile.pickup_attachment,
    );
    attach_visual_to_unit(world, attachment_id, unit_id, center - position);
    let ball_direction = Vec3::new(
        ball_position(world, execution).x - position.x,
        0.0,
        ball_position(world, execution).z - position.z,
    )
    .normalize_or(Vec3::X);
    let mut lateral = ball_direction.cross(Vec3::Y);
    lateral.x *= world.trigger_random_float(
        -execution.profile.initial_lateral_pull_strength,
        execution.profile.initial_lateral_pull_strength,
    ) * mass;
    lateral.y =
        world.trigger_random_float(0.0, execution.profile.initial_lateral_pull_strength) * mass;
    lateral.z *= world.trigger_random_float(
        -execution.profile.initial_lateral_pull_strength,
        execution.profile.initial_lateral_pull_strength,
    ) * mass;
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.set_auto_attackable(false);
        unit.obstruction_half_extents = Vec3::ZERO;
        if let Some(physics) = &mut unit.physics {
            physics.set_angular_damping(execution.profile.debris_angular_damping);
        }
        unit.stop();
        let _applied = unit.apply_impulse_at_point(lateral, position + ball_direction);
    }
    let _changed_owner = world.change_unit_owner(unit_id, execution.player_id);
    let _reset_dopples = world.reset_entity_dopples(unit_id, false, false);
    execution.captured_objects.push(WaveCapturedObject {
        unit_id,
        pickup_attachment_id: attachment_id,
        color_player_id,
        was_auto_attackable,
    });
}

fn attach_visual_to_unit(
    world: &mut World,
    attachment_id: EntityId,
    unit_id: EntityId,
    world_offset: Vec3,
) {
    if attachment_id.is_invalid() {
        return;
    }
    let forward = world
        .get_unit(unit_id)
        .map_or(Vec3::Z, |unit| unit.base.forward);
    let local_offset = formation_offset_to_local(forward, world_offset);
    if let Some(child) = world.entity_object_state_mut(attachment_id) {
        child.set_attached_to(Some(unit_id));
        child.set_attachment_local_offset(local_offset);
    }
    if let Some(parent) = world.entity_object_state_mut(unit_id) {
        parent.add_attachment(attachment_id);
    }
}

fn validate_captured_objects(world: &mut World, execution: &mut WavePowerExecution) {
    let captured = std::mem::take(&mut execution.captured_objects);
    for captured in captured {
        if world.get_unit(captured.unit_id).is_some() {
            execution.captured_objects.push(captured);
        } else {
            let _attachment = world.remove_object(captured.pickup_attachment_id);
        }
    }
}

fn pull_captured_objects(world: &mut World, execution: &WavePowerExecution, dt: f32) {
    let ball = ball_position(world, execution);
    for (index, captured) in execution.captured_objects.iter().enumerate() {
        let Some((center, velocity, mass)) = world.get_unit(captured.unit_id).and_then(|unit| {
            unit.physics.as_ref().map(|body| {
                (
                    unit.base.position + body.collider().center_offset,
                    unit.base.velocity,
                    body.material().mass,
                )
            })
        }) else {
            continue;
        };
        let rest_length = execution.profile.captured_spring_rest_length
            + index.to_f32().unwrap_or(f32::MAX) * execution.profile.captured_radial_spacing;
        let impulse = gravity_ball_impulse(
            PullBody {
                center,
                velocity,
                mass,
            },
            ball,
            PullSpring {
                rest_length,
                strength: execution.profile.captured_spring_strength,
                damping: execution.profile.captured_spring_dampening,
                minimum_lateral_speed: execution.profile.captured_minimum_lateral_speed,
                dt,
            },
        );
        if let Some(unit) = world.get_unit_mut(captured.unit_id) {
            let _applied = unit.apply_impulse(impulse);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PullBody {
    pub center: Vec3,
    pub velocity: Vec3,
    pub mass: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PullSpring {
    pub rest_length: f32,
    pub strength: f32,
    pub damping: f32,
    pub minimum_lateral_speed: f32,
    pub dt: f32,
}

pub(super) fn gravity_ball_impulse(body: PullBody, ball: Vec3, spring: PullSpring) -> Vec3 {
    let anti_gravity = Vec3::Y * body.mass * PHYSICS_GRAVITY * spring.dt;
    let outward_offset = body.center - ball;
    let distance = outward_offset.length();
    if distance < 0.001 || distance < spring.rest_length {
        return anti_gravity;
    }
    let outward = outward_offset / distance;
    let relative_radial_velocity = body.velocity.dot(outward);
    let force = relative_radial_velocity.mul_add(
        spring.damping,
        (distance - spring.rest_length) * spring.strength,
    );
    let spring_impulse = -outward * force * spring.dt;
    let lateral_velocity = body.velocity - outward * relative_radial_velocity;
    let lateral = if lateral_velocity.length() < spring.minimum_lateral_speed {
        lateral_velocity * body.mass
    } else {
        Vec3::ZERO
    };
    anti_gravity + spring_impulse + lateral
}

fn increase_damage_bank(execution: &mut WavePowerExecution) {
    execution.current_explosion_damage_bank = (execution.current_explosion_damage_bank
        + execution.profile.explosion_damage_bank_per_tick)
        .clamp(0.0, execution.maximum_possible_explosion_damage_bank);
}

fn pay_upkeep(world: &mut World, execution: &WavePowerExecution) -> bool {
    let Some(player) = world.get_player_mut(execution.player_id) else {
        return false;
    };
    if player.resources.get(execution.profile.supplies_resource_id)
        < execution.profile.supplies_per_tick
    {
        return false;
    }
    player.resources.subtract(
        execution.profile.supplies_resource_id,
        execution.profile.supplies_per_tick,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_impulse_matches_retail_spring_antigravity_and_lateral_terms() {
        let spring = PullSpring {
            rest_length: 1.0,
            strength: 100.0,
            damping: 0.5,
            minimum_lateral_speed: 2.0,
            dt: 0.1,
        };
        let impulse = gravity_ball_impulse(
            PullBody {
                center: Vec3::X * 3.0,
                velocity: Vec3::new(2.0, 0.0, 1.0),
                mass: 10.0,
            },
            Vec3::ZERO,
            spring,
        );
        assert!(impulse.abs_diff_eq(Vec3::new(-20.1, 9.81, 10.0), 0.000_1));

        let compressed = gravity_ball_impulse(
            PullBody {
                center: Vec3::X * 0.5,
                velocity: Vec3::Z,
                mass: 10.0,
            },
            Vec3::ZERO,
            spring,
        );
        assert!(compressed.abs_diff_eq(Vec3::Y * 9.81, 0.000_1));
    }
}
