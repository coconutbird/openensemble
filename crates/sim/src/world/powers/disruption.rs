//! Retail `BPowerDisruption` profile resolution and execution.

use super::common::{
    Bomber, BomberProfile, PowerPayment, attach_power_visual, consume_power, create_power_visual,
    finish_power_visual, random_direction, required_float, required_prototype, required_string,
    validate_level, validate_power_type, validate_requirements,
};
use super::{
    DisruptionPowerError, DisruptionPowerExecution, DisruptionPowerInvocation, PowerExecutionId,
    power_by_id,
};
use crate::EntityId;
use crate::world::{GeneralEvent, GeneralEventType, World};
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

const FIELD_DEATH_LIFETIME_MS: u32 = 2_500;

#[derive(Debug)]
struct DisruptionProfile {
    disruption_object_prototype: String,
    pulse_object_prototype: String,
    strike_object_prototype: String,
    pulse_sound: String,
    radius: f32,
    duration_seconds: f32,
    start_time_seconds: f32,
    pulse_spacing_seconds: f32,
    bomber: BomberProfile,
}

pub(super) fn invoke(
    world: &mut World,
    database: &Database,
    invocation: DisruptionPowerInvocation,
) -> Result<PowerExecutionId, DisruptionPowerError> {
    if !invocation.target_location.is_finite() {
        return Err(DisruptionPowerError::InvalidTarget);
    }
    let power = power_by_id(database, invocation.proto_power_id).ok_or(
        DisruptionPowerError::PowerNotFound(invocation.proto_power_id),
    )?;
    let attributes = validate_power_type(power, "Disruption")?;
    if world.get_player(invocation.player_id).is_none() {
        return Err(DisruptionPowerError::PlayerNotFound(invocation.player_id));
    }
    let profile = DisruptionProfile::resolve(database, attributes, invocation.power_level)?;
    let payment = if invocation.ignore_requirements {
        PowerPayment::default()
    } else {
        validate_requirements(
            world,
            database,
            power,
            invocation.player_id,
            invocation.proto_power_id,
        )?
    };
    let direction = random_direction(world);
    if !invocation.ignore_requirements {
        consume_power(
            world,
            power,
            invocation.player_id,
            invocation.proto_power_id,
            invocation.squad_id,
            &payment,
        )?;
    }
    let id = world.power_manager.allocate_id();
    let execution = create_execution(world, database, invocation, profile, direction, id);
    world.power_manager.disruption_executions.push(execution);
    let _fired = world.fire_general_event(&GeneralEvent::new(
        GeneralEventType::UsedPower,
        i32::from(invocation.player_id),
    ));
    Ok(id)
}

pub(super) fn update(world: &mut World, dt: f32, database: &Database) {
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let mut active = std::mem::take(&mut world.power_manager.disruption_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active.drain(..) {
        if update_execution(world, &mut execution, dt, database) {
            remaining.push(execution);
        }
    }
    world.power_manager.disruption_executions = remaining;
}

pub(super) fn disrupting_power_at(
    world: &World,
    source_attributes: &PowerAttributes,
    location: Vec3,
) -> Option<PowerExecutionId> {
    if source_attributes.not_disruptable.unwrap_or(false) {
        return None;
    }
    world
        .power_manager
        .disruption_executions
        .iter()
        .find(|execution| {
            execution.is_active()
                && planar_distance_squared(location, execution.target_location)
                    < execution.radius * execution.radius
        })
        .map(DisruptionPowerExecution::id)
}

pub(super) fn disrupting_power_circle(
    world: &World,
    source_attributes: &PowerAttributes,
    location: Vec3,
    radius: f32,
) -> Option<PowerExecutionId> {
    if source_attributes.not_disruptable.unwrap_or(false) {
        return None;
    }
    world
        .power_manager
        .disruption_executions
        .iter()
        .find(|execution| {
            let combined_radius = execution.radius + radius.max(0.0);
            execution.is_active()
                && planar_distance_squared(location, execution.target_location)
                    < combined_radius * combined_radius
        })
        .map(DisruptionPowerExecution::id)
}

fn create_execution(
    world: &mut World,
    database: &Database,
    invocation: DisruptionPowerInvocation,
    profile: DisruptionProfile,
    direction: Vec3,
    id: PowerExecutionId,
) -> DisruptionPowerExecution {
    let right = Vec3::Y.cross(direction).normalize();
    let bomber = Bomber::spawn(
        world,
        database,
        invocation.player_id,
        invocation.target_location,
        direction,
        profile.bomber,
    );
    DisruptionPowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        bomber,
        disruption_object_id: EntityId::INVALID,
        target_location: invocation.target_location,
        direction,
        right,
        disruption_object_prototype: profile.disruption_object_prototype,
        pulse_object_prototype: profile.pulse_object_prototype,
        strike_object_prototype: profile.strike_object_prototype,
        pulse_sound: profile.pulse_sound,
        radius: profile.radius,
        time_remaining_seconds: profile.duration_seconds,
        start_time_seconds: profile.start_time_seconds,
        next_pulse_time_seconds: 0.0,
        pulse_spacing_seconds: profile.pulse_spacing_seconds,
        pulse_count: 0,
        elapsed_seconds: 0.0,
    }
}

fn update_execution(
    world: &mut World,
    execution: &mut DisruptionPowerExecution,
    dt: f32,
    database: &Database,
) -> bool {
    execution.elapsed_seconds += dt;
    execution.bomber.update(
        world,
        execution.target_location,
        execution.direction,
        execution.elapsed_seconds,
        dt,
    );
    if execution.elapsed_seconds < execution.bomber.bomb_time() {
        return true;
    }
    if execution.disruption_object_id.is_invalid() {
        execution.disruption_object_id = create_power_visual(
            world,
            database,
            execution.player_id,
            execution.target_location,
            -execution.direction,
            &execution.disruption_object_prototype,
        );
    }
    if !execution.is_active() {
        return true;
    }
    if execution.elapsed_seconds > execution.next_pulse_time_seconds {
        let _pulse_id = attach_power_visual(
            world,
            database,
            execution.disruption_object_id,
            &execution.pulse_object_prototype,
        );
        execution.pulse_count = execution.pulse_count.saturating_add(1);
        execution.next_pulse_time_seconds = execution.elapsed_seconds
            + execution.pulse_count.to_f32().unwrap_or(f32::MAX) * execution.pulse_spacing_seconds;
    }
    execution.time_remaining_seconds -= dt;
    if execution.time_remaining_seconds > 0.0 {
        return true;
    }
    finish_power_visual(
        world,
        execution.disruption_object_id,
        FIELD_DEATH_LIFETIME_MS,
    );
    execution.disruption_object_id = EntityId::INVALID;
    execution.bomber.kill(world);
    false
}

impl DisruptionProfile {
    fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, DisruptionPowerError> {
        validate_level(attributes, level)?;
        let radius = required_float(attributes, level, "DisruptionRadius")?;
        if radius <= 0.0 {
            return Err(DisruptionPowerError::InvalidData("DisruptionRadius"));
        }
        let duration_seconds = required_float(attributes, level, "DisruptionTimeSec")?;
        if duration_seconds <= 0.0 {
            return Err(DisruptionPowerError::InvalidData("DisruptionTimeSec"));
        }
        Ok(Self {
            disruption_object_prototype: required_prototype(
                database,
                attributes,
                level,
                "DisruptionObject",
            )?,
            pulse_object_prototype: required_prototype(database, attributes, level, "PulseObject")?,
            strike_object_prototype: required_prototype(
                database,
                attributes,
                level,
                "StrikeObject",
            )?,
            pulse_sound: required_string(attributes, level, "PulseSound", "sound")?,
            radius,
            duration_seconds,
            start_time_seconds: required_float(attributes, level, "DisruptionStartTime")?,
            pulse_spacing_seconds: required_float(attributes, level, "PulseSpacing")?,
            bomber: BomberProfile::resolve(database, attributes, level)?,
        })
    }
}

fn planar_distance_squared(left: Vec3, right: Vec3) -> f32 {
    let x = left.x - right.x;
    let z = left.z - right.z;
    x.mul_add(x, z * z)
}

#[cfg(test)]
mod tests;
