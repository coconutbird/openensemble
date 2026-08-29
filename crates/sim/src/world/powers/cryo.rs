//! Retail `BPowerCryo` profile resolution and execution.

use super::common::{
    Bomber, BomberProfile, PowerPayment, consume_power, create_power_visual, random_direction,
    required_float, required_int, required_object_type, required_prototype,
    seconds_to_milliseconds, validate_level, validate_power_type, validate_requirements,
};
use super::{
    CryoPowerError, CryoPowerExecution, CryoPowerInvocation, PowerExecutionId, power_by_id,
};
use crate::EntityId;
use crate::world::{GeneralEvent, GeneralEventType, World};
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

#[derive(Debug)]
struct CryoProfile {
    cryo_object_prototype: String,
    bomber: BomberProfile,
    filter_type: String,
    radius: f32,
    minimum_falloff: f32,
    tick_duration_ms: u32,
    ticks: u32,
    cryo_amount_per_tick: f32,
    effect_start_ms: u32,
    killable_hitpoints: f32,
    freezing_thaw_time: f32,
    frozen_thaw_time: f32,
}

pub(super) fn invoke(
    world: &mut World,
    database: &Database,
    invocation: CryoPowerInvocation,
) -> Result<PowerExecutionId, CryoPowerError> {
    if !invocation.target_location.is_finite() {
        return Err(CryoPowerError::InvalidTarget);
    }
    let power = power_by_id(database, invocation.proto_power_id)
        .ok_or(CryoPowerError::PowerNotFound(invocation.proto_power_id))?;
    let attributes = validate_power_type(power, "Cryo")?;
    if let Some(disruption_id) =
        super::disruption::disrupting_power_at(world, attributes, invocation.target_location)
    {
        return Err(CryoPowerError::Disrupted(disruption_id));
    }
    if world.get_player(invocation.player_id).is_none() {
        return Err(CryoPowerError::PlayerNotFound(invocation.player_id));
    }
    let profile = CryoProfile::resolve(database, attributes, invocation.power_level)?;
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
    world.power_manager.cryo_executions.push(execution);
    let _fired = world.fire_general_event(&GeneralEvent::new(
        GeneralEventType::UsedPower,
        i32::from(invocation.player_id),
    ));
    Ok(id)
}

pub(super) fn update(world: &mut World, dt: f32, database: &Database) {
    let mut active = std::mem::take(&mut world.power_manager.cryo_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active.drain(..) {
        update_bomber(world, &mut execution, dt, database);
        if execution.bomb_released {
            update_ticks(world, &mut execution, database);
        }
        if execution.ticks_remaining > 0 {
            remaining.push(execution);
        } else {
            execution.bomber.kill(world);
        }
    }
    world.power_manager.cryo_executions = remaining;
}

fn create_execution(
    world: &mut World,
    database: &Database,
    invocation: CryoPowerInvocation,
    profile: CryoProfile,
    direction: Vec3,
    id: PowerExecutionId,
) -> CryoPowerExecution {
    let bomber = Bomber::spawn(
        world,
        database,
        invocation.player_id,
        invocation.target_location,
        direction,
        profile.bomber,
    );
    CryoPowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        bomber,
        cryo_object_id: EntityId::INVALID,
        target_location: invocation.target_location,
        direction,
        cryo_object_prototype: profile.cryo_object_prototype,
        filter_type: profile.filter_type,
        radius: profile.radius,
        minimum_falloff: profile.minimum_falloff,
        tick_duration_ms: profile.tick_duration_ms,
        ticks_remaining: profile.ticks,
        next_tick_time_ms: world
            .game_time_ms
            .wrapping_add(profile.effect_start_ms)
            .wrapping_add(profile.tick_duration_ms),
        cryo_amount_per_tick: profile.cryo_amount_per_tick,
        killable_hitpoints_left: profile.killable_hitpoints,
        freezing_thaw_time: profile.freezing_thaw_time,
        frozen_thaw_time: profile.frozen_thaw_time,
        elapsed_seconds: 0.0,
        bomb_released: false,
        ignored_squads: Vec::new(),
    }
}

fn update_bomber(
    world: &mut World,
    execution: &mut CryoPowerExecution,
    dt: f32,
    database: &Database,
) {
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    execution.elapsed_seconds += dt;
    execution.bomber.update(
        world,
        execution.target_location,
        execution.direction,
        execution.elapsed_seconds,
        dt,
    );
    if !execution.bomb_released && execution.elapsed_seconds >= execution.bomber.bomb_time() {
        execution.bomb_released = true;
        execution.cryo_object_id = create_power_visual(
            world,
            database,
            execution.player_id,
            execution.target_location,
            -execution.direction,
            &execution.cryo_object_prototype,
        );
    }
}

fn update_ticks(world: &mut World, execution: &mut CryoPowerExecution, database: &Database) {
    let current_time = world.game_time_ms;
    if current_time < execution.next_tick_time_ms {
        return;
    }
    let mut candidates = world.find_squads_by_leader_type_in_area(
        &execution.filter_type,
        execution.target_location,
        execution.radius,
    );
    candidates.sort_by(|left, right| {
        squad_distance_squared(world, *left, execution.target_location)
            .total_cmp(&squad_distance_squared(
                world,
                *right,
                execution.target_location,
            ))
            .then_with(|| left.as_u32().cmp(&right.as_u32()))
    });

    while execution.ticks_remaining > 0 && current_time >= execution.next_tick_time_ms {
        execution.ticks_remaining -= 1;
        execution.next_tick_time_ms = execution
            .next_tick_time_ms
            .wrapping_add(execution.tick_duration_ms);
        let Some(squad_id) = candidates.iter().copied().find(|candidate| {
            !execution.ignored_squads.contains(candidate) && world.get_squad(*candidate).is_some()
        }) else {
            continue;
        };
        execution.ignored_squads.push(squad_id);
        apply_to_squad(world, execution, database, squad_id);
        // Retail handles at most one newly selected squad per power update,
        // even when several scheduled ticks are overdue.
        break;
    }
}

fn apply_to_squad(
    world: &mut World,
    execution: &mut CryoPowerExecution,
    database: &Database,
    squad_id: EntityId,
) {
    if let Some(amount) = world.squad_cryo_maximum(squad_id, database) {
        let _accepted = world.add_squad_cryo_with_thaw_times(
            squad_id,
            amount,
            database,
            Some((execution.freezing_thaw_time, execution.frozen_thaw_time)),
        );
    }
    let dies_when_frozen = world
        .effective_squad_prototype(squad_id, database)
        .is_some_and(|prototype| {
            prototype
                .flags
                .iter()
                .any(|flag| flag.eq_ignore_ascii_case("DiesWhenFrozen"))
        });
    if !dies_when_frozen
        || !world
            .get_squad(squad_id)
            .is_some_and(crate::entities::Squad::is_cryo_frozen)
    {
        return;
    }
    let hitpoints = squad_hitpoints(world, squad_id);
    let transporter = squad_leader_is_transporter(world, squad_id);
    if transporter
        || (execution.killable_hitpoints_left > 0.0
            && hitpoints < execution.killable_hitpoints_left)
    {
        let _killed = world.force_cryo_frozen_kill(squad_id);
        execution.killable_hitpoints_left -= hitpoints;
    }
}

fn squad_distance_squared(world: &World, squad_id: EntityId, target: Vec3) -> f32 {
    world.get_squad(squad_id).map_or(f32::INFINITY, |squad| {
        squad.base.position.distance_squared(target)
    })
}

fn squad_hitpoints(world: &World, squad_id: EntityId) -> f32 {
    world.get_squad(squad_id).map_or(0.0, |squad| {
        squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| world.get_unit(*unit_id))
            .map(|unit| unit.hitpoints)
            .sum()
    })
}

fn squad_leader_is_transporter(world: &World, squad_id: EntityId) -> bool {
    world.get_squad(squad_id).is_some_and(|squad| {
        squad
            .unit_ids
            .iter()
            .find_map(|unit_id| world.get_unit(*unit_id))
            .is_some_and(|leader| {
                leader.is_object_type("_Transporter") || leader.is_object_type("Transporter")
            })
    })
}

impl CryoProfile {
    fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, CryoPowerError> {
        validate_level(attributes, level)?;
        let tick_duration = required_float(attributes, level, "TickDuration")?;
        let ticks = required_int(attributes, level, "NumTicks")?;
        let radius = required_float(attributes, level, "CryoRadius")?;
        let cryo_amount_per_tick = required_float(attributes, level, "CryoAmountPerTick")?;
        if radius <= 0.0 {
            return Err(CryoPowerError::InvalidData("CryoRadius"));
        }
        if tick_duration <= 0.0 {
            return Err(CryoPowerError::InvalidData("TickDuration"));
        }
        if ticks <= 0 {
            return Err(CryoPowerError::InvalidData("NumTicks"));
        }
        if cryo_amount_per_tick <= 0.0 {
            return Err(CryoPowerError::InvalidData("CryoAmountPerTick"));
        }
        Ok(Self {
            cryo_object_prototype: required_prototype(database, attributes, level, "CryoObject")?,
            bomber: BomberProfile::resolve(database, attributes, level)?,
            filter_type: required_object_type(database, attributes, level, "FilterType")?,
            radius,
            minimum_falloff: required_float(attributes, level, "MinCryoFalloff")?,
            tick_duration_ms: seconds_to_milliseconds(tick_duration),
            ticks: u32::try_from(ticks).map_err(|_| CryoPowerError::InvalidData("NumTicks"))?,
            cryo_amount_per_tick,
            effect_start_ms: seconds_to_milliseconds(required_float(
                attributes,
                level,
                "EffectStartTime",
            )?),
            killable_hitpoints: required_float(attributes, level, "MaxKillHp")?,
            freezing_thaw_time: required_float(attributes, level, "FreezingThawTime")?,
            frozen_thaw_time: required_float(attributes, level, "FrozenThawTime")?,
        })
    }
}

#[cfg(test)]
mod tests;
