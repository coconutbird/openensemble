//! Gravity-ball scan, pickup queue, and the capture of objects by the gravity-ball pull action.

use super::super::common::create_power_visual;
use super::{
    QueuedWaveObject, WaveCapturedObject, WaveGravityBallState, WavePowerExecution, ball_position,
    combat, debris, owner_leader_id,
};
use crate::EntityId;
use crate::entities::squads::formation_offset_to_local;
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::physics::GravityBallSpring;
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
) {
    update_queued_objects(world, database, execution);
    validate_captured_objects(world, execution);
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
    process_ticks(world, database, gameplay, execution);
}

fn process_ticks(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
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
            let result = attempt_pull_unit(world, gameplay, execution, *unit_id, leader_id, ball);
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
        let _part = debris::rip_part_off_unit(world, gameplay, execution, unit_id);
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
        queue_add_object(world, execution, unit_id);
        return true;
    }
    if alive && hitpoints > execution.profile.health_to_capture {
        return false;
    }
    if let Some(replacement_id) = create_physics_replacement(world, gameplay, unit_id) {
        queue_add_object(world, execution, replacement_id);
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
    let proto_object_name = world.get_unit(unit_id)?.proto_object_name.clone();
    let template = std::sync::Arc::clone(gameplay?.physics_replacement(&proto_object_name)?);
    world.spawn_physics_replacement(unit_id, &template)
}

/// `BPowerUserWave` pickup test: infantry, or a body whose box half extents sum to more than 1
/// and less than 8.
fn physics_valid_for_pulling(world: &World, unit: &crate::Unit) -> bool {
    if unit.is_object_type("Infantry") {
        return true;
    }
    world
        .unit_body_half_extents(unit.base.id)
        .is_some_and(|extents| {
            let sum = extents.x + extents.y + extents.z;
            sum > 1.0 && sum < 8.0
        })
}

fn queue_add_object(world: &mut World, execution: &mut WavePowerExecution, unit_id: EntityId) {
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
    if world.unit_body(unit_id).is_none() || !physics_valid_for_pulling(world, unit) {
        return;
    }
    let is_clamshell = world
        .get_unit(unit_id)
        .and_then(|unit| unit.physics_object.as_ref())
        .is_some_and(|state| state.clamshell_bodies().is_some());
    if is_clamshell {
        let _damping = world.set_unit_body_linear_damping(unit_id, 0.1);
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

/// `BPowerWave_addObjectToBall` (`0x140331D40`, 2008 `powerwave.cpp:1996-2086`): the pickup
/// attachment at the centre of mass, the physics action back to its update state without
/// complete-on-inactive, the debris angular damping, the gravity-ball pull action with the rest
/// length `CapturedSpringRestLength + captured · CapturedRadialSpacing` and the random lateral
/// point impulse.
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
    let Some((position, forward, color_player_id, was_auto_attackable)) =
        world.get_unit(unit_id).map(|unit| {
            (
                unit.base.position,
                unit.base.forward,
                unit.base.player_id,
                unit.auto_attackable_setting(),
            )
        })
    else {
        return;
    };
    let (Some(center), Some(mass)) = (
        world.unit_body_center_of_mass(unit_id),
        world.unit_body_mass(unit_id),
    ) else {
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
        unit.stop();
    }
    let _restarted = world.restart_unit_physics_action(unit_id, false);
    let _damping =
        world.set_unit_body_angular_damping(unit_id, execution.profile.debris_angular_damping);
    let _pull =
        world.add_gravity_ball_pull(unit_id, execution.ball_object_id, capture_spring(execution));
    let _applied = world.apply_unit_point_impulse(unit_id, lateral, position + ball_direction);
    let _changed_owner = world.change_unit_owner(unit_id, execution.player_id);
    let _reset_dopples = world.reset_entity_dopples(unit_id, false, false);
    execution.captured_objects.push(WaveCapturedObject {
        unit_id,
        pickup_attachment_id: attachment_id,
        color_player_id,
        was_auto_attackable,
    });
}

/// The spring of a new gravity-ball action. The rest length grows with the number of captured
/// objects when the action is made.
pub(super) fn capture_spring(execution: &WavePowerExecution) -> GravityBallSpring {
    GravityBallSpring {
        rest_length: execution.profile.captured_spring_rest_length
            + execution
                .captured_objects
                .len()
                .to_f32()
                .unwrap_or(f32::MAX)
                * execution.profile.captured_radial_spacing,
        strength: execution.profile.captured_spring_strength,
        damping: execution.profile.captured_spring_dampening,
        minimum_lateral_speed: execution.profile.captured_minimum_lateral_speed,
    }
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
