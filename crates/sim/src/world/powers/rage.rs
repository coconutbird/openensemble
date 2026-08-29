//! Retail `BPowerRage` lifecycle, upkeep, movement, and live input.

mod aura;
mod combat;
mod execution;
mod profile;
mod targeting;

pub(in crate::world::powers) use execution::PendingRageKill;
pub use execution::{RagePowerExecution, RagePowerPhase};

use super::common::{PowerPayment, consume_power, validate_power_type, validate_requirements};
use super::{
    NativePowerError, NativePowerInput, NativePowerInvocation, PowerExecutionId, power_by_id,
};
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::entities::{SquadMode, SquadState, UnitDataScalar};
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::world::{GeneralEvent, GeneralEventType, World};
use execution::{RageFlags, RagePowerExecution as Execution, RagePowerPhase as Phase, RageSpline};
use glam::Vec3;
use pipeline::database::hw1::Database;
use profile::RageProfile;

const RAGE_POWER_TYPE: u32 = 5;

pub(super) fn invoke(
    world: &mut World,
    database: &Database,
    invocation: NativePowerInvocation,
) -> Result<PowerExecutionId, NativePowerError> {
    if !invocation.target_location.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    validate_power_user(invocation)?;
    let power = power_by_id(database, invocation.proto_power_id)
        .ok_or(NativePowerError::PowerNotFound(invocation.proto_power_id))?;
    let attributes = validate_power_type(power, "Rage")?;
    if world.get_player(invocation.player_id).is_none() {
        return Err(NativePowerError::PlayerNotFound(invocation.player_id));
    }
    let (_, owner_position) = owner_snapshot(world, invocation.player_id, invocation.squad_id)
        .ok_or(NativePowerError::InvalidTarget)?;
    if world
        .get_squad(invocation.squad_id)
        .is_some_and(crate::entities::Squad::is_raging)
    {
        return Err(NativePowerError::PowerUnavailable);
    }
    let profile = RageProfile::resolve(database, attributes, invocation.power_level)?;
    validate_location(world, attributes, owner_position)?;
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
    let mut execution = create_execution(invocation, profile, owner_position, id);
    activate_owner(world, database, &mut execution);
    world.power_manager.rage_executions.push(execution);
    let _fired = world.fire_general_event(&GeneralEvent::new(
        GeneralEventType::UsedPower,
        i32::from(invocation.player_id),
    ));
    Ok(id)
}

fn validate_power_user(invocation: NativePowerInvocation) -> Result<(), NativePowerError> {
    let id = invocation.power_user_id;
    if id.is_valid()
        && (id.player_id() != i32::from(invocation.player_id) || id.power_type() != RAGE_POWER_TYPE)
    {
        return Err(NativePowerError::InvalidData("PowerUserID"));
    }
    Ok(())
}

fn validate_location(
    world: &World,
    attributes: &pipeline::database::hw1::powers::PowerAttributes,
    position: Vec3,
) -> Result<(), NativePowerError> {
    if world.is_outside_playable_bounds(position, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(id) = super::disruption::disrupting_power_at(world, attributes, position) {
        return Err(NativePowerError::Disrupted(id));
    }
    Ok(())
}

fn create_execution(
    invocation: NativePowerInvocation,
    profile: RageProfile,
    owner_position: Vec3,
    id: PowerExecutionId,
) -> Execution {
    Execution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        power_user_id: invocation.power_user_id,
        target_location: invocation.target_location,
        target_squad_id: EntityId::INVALID,
        phase: Phase::Active,
        move_input: Vec3::ZERO,
        move_target: owner_position,
        teleport_destination: owner_position,
        spline: RageSpline::default(),
        teleport_remaining: -1.0,
        retarget_remaining: -1.0,
        elapsed_seconds: 0.0,
        next_tick_time: 0.0,
        tick_length: profile.tick_length,
        supplies_per_tick: profile.supplies_per_tick,
        supplies_per_tick_attacking: profile.supplies_per_tick_attacking,
        supplies_per_jump: profile.supplies_per_jump,
        supplies_resource_id: profile.supplies_resource_id,
        damage_multiplier: profile.damage_multiplier,
        damage_taken_multiplier: profile.damage_taken_multiplier,
        speed_multiplier: profile.speed_multiplier,
        nudge_multiplier: profile.nudge_multiplier,
        scan_radius: profile.scan_radius,
        teleport_time: profile.teleport_time,
        teleport_lateral_distance: profile.teleport_lateral_distance,
        teleport_jump_distance: profile.teleport_jump_distance,
        time_between_retarget: profile.time_between_retarget,
        distance_vs_angle_weight: profile.distance_vs_angle_weight,
        projectile_prototype: profile.projectile_prototype,
        hand_attachment_prototype: profile.hand_attachment_prototype,
        hand_attachment_prototype_id: profile.hand_attachment_prototype_id,
        teleport_attachment_prototype: profile.teleport_attachment_prototype,
        teleport_attachment_prototype_id: profile.teleport_attachment_prototype_id,
        aura_attachment_prototypes: profile.aura_attachment_prototypes,
        aura_attachment_prototype_ids: profile.aura_attachment_prototype_ids,
        heal_attachment_prototype: profile.heal_attachment_prototype,
        heal_attachment_prototype_id: profile.heal_attachment_prototype_id,
        aura_filter_type: profile.aura_filter_type,
        heal_per_kill_combat_value: profile.heal_per_kill_combat_value,
        aura_radius: profile.aura_radius,
        aura_damage_bonus: profile.aura_damage_bonus,
        flags: RageFlags::new(
            invocation.ignore_requirements,
            !invocation.power_user_id.is_valid(),
        ),
        hand_attachment_ids: Vec::new(),
        aura_squad_ids: Vec::new(),
        aura_attachments: Vec::new(),
    }
}

fn activate_owner(world: &mut World, database: &Database, execution: &mut Execution) {
    let unit_ids = world
        .get_squad(execution.owner_squad_id)
        .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
    adjust_owner_units(world, execution, &unit_ids, false);
    let leader_id = unit_ids
        .iter()
        .find(|id| world.get_unit(**id).is_some())
        .copied();
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.remove_all_orders();
        let _first = squad.rage.begin();
        squad.mode = SquadMode::Power;
        squad.state = SquadState::Working;
    }
    let Some(leader_id) = leader_id else {
        return;
    };
    for _ in 0..2 {
        if let Some(id) = world.add_prototype_attachment_to_unit(
            database,
            leader_id,
            execution.hand_attachment_prototype_id,
        ) {
            execution.hand_attachment_ids.push(id);
        }
    }
}

pub(super) fn update(
    world: &mut World,
    dt: f32,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
) {
    let active = std::mem::take(&mut world.power_manager.rage_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active {
        if update_execution(world, dt, database, gameplay, &mut execution) {
            remaining.push(execution);
        } else {
            shutdown(world, &mut execution);
        }
    }
    world.power_manager.rage_executions = remaining;
}

fn update_execution(
    world: &mut World,
    dt: f32,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut Execution,
) -> bool {
    let Some((_, owner_position)) =
        owner_snapshot(world, execution.player_id, execution.owner_squad_id)
    else {
        return false;
    };
    let Some(attributes) =
        power_by_id(database, execution.proto_power_id).and_then(|power| power.attributes.as_ref())
    else {
        return false;
    };
    if validate_location(world, attributes, owner_position).is_err() {
        return false;
    }
    reconcile_target(world, execution);
    aura::update(world, database, execution);
    execution.retarget_remaining -= dt;
    if execution.phase == Phase::Jumping {
        update_jump(world, dt, gameplay, execution);
    } else {
        update_motion(world, dt, execution);
    }
    charge_upkeep(world, dt, execution)
}

fn reconcile_target(world: &mut World, execution: &mut Execution) {
    if execution.target_squad_id.is_invalid() {
        return;
    }
    let valid = world
        .get_squad(execution.target_squad_id)
        .filter(|squad| squad.is_alive())
        .and_then(|squad| squad.unit_ids.iter().find_map(|id| world.get_unit(*id)))
        .is_some_and(crate::entities::Unit::is_attackable);
    if valid {
        return;
    }
    execution.target_squad_id = EntityId::INVALID;
    execution.phase = Phase::Active;
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.remove_all_orders();
        squad.mode = SquadMode::Power;
        squad.state = SquadState::Working;
    }
}

fn update_jump(
    world: &mut World,
    dt: f32,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut Execution,
) {
    execution.teleport_remaining -= dt;
    if execution.teleport_remaining <= 0.0 {
        land_jump(world, gameplay, execution);
        return;
    }
    let progress = ((execution.teleport_time - execution.teleport_remaining)
        / execution.teleport_time)
        .clamp(0.0, 1.0)
        * 0.5;
    let position = execution.spline.evaluate(progress);
    let Some(leader_id) = owner_leader_id(world, execution.owner_squad_id) else {
        return;
    };
    if let Some(leader) = world.get_unit_mut(leader_id) {
        leader.base.position = position;
        let direction = execution.teleport_destination - position;
        if direction.length_squared() > f32::EPSILON {
            leader.base.set_forward(direction);
        }
    }
}

fn land_jump(world: &mut World, gameplay: Option<&GameplayCatalog>, execution: &mut Execution) {
    combat::apply_landing_impact(world, execution, gameplay);
    let _teleported =
        world.teleport_squad(execution.owner_squad_id, execution.teleport_destination);
    execution.teleport_remaining = -1.0;
    execution.move_input = Vec3::ZERO;
    execution.move_target = execution.teleport_destination;
    if target_is_alive(world, execution.target_squad_id) {
        set_attack_order(world, execution.owner_squad_id, execution.target_squad_id);
        execution.phase = Phase::Attacking;
    } else {
        execution.target_squad_id = EntityId::INVALID;
        execution.phase = Phase::Active;
    }
}

fn update_motion(world: &mut World, dt: f32, execution: &mut Execution) {
    let Some((leader_id, position)) =
        owner_snapshot(world, execution.player_id, execution.owner_squad_id)
    else {
        return;
    };
    let (delta, magnitude) = if execution.flags.uses_pather() {
        let delta = execution.move_target - position;
        (Vec3::new(delta.x, 0.0, delta.z), 1.0)
    } else {
        let input = Vec3::new(execution.move_input.x, 0.0, execution.move_input.z);
        (input, input.length().clamp(0.0, 1.0))
    };
    let radius = world
        .get_unit(leader_id)
        .map_or(0.0, crate::entities::Unit::obstruction_radius);
    if delta.length() <= radius.max(f32::EPSILON) || magnitude <= f32::EPSILON {
        stop_motion(world, execution);
        return;
    }
    let direction = delta.normalize();
    let speed = world.get_unit(leader_id).map_or(0.0, |unit| {
        unit.speed * unit.effective_velocity_scalar() * magnitude
    });
    let distance = if execution.flags.uses_pather() {
        (speed * dt).min(delta.length())
    } else {
        speed * dt
    };
    let mut next = position + direction * distance;
    if world.is_outside_playable_bounds(next, true) {
        stop_motion(world, execution);
        return;
    }
    if let Some(height) = world.terrain_height(next, true) {
        next.y = height;
    }
    let _moved = world.teleport_squad(execution.owner_squad_id, next);
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.base.velocity = direction * speed;
        squad.base.set_forward(direction);
        squad.mode = SquadMode::Power;
        squad.state = SquadState::Working;
    }
    execution.phase = Phase::Moving;
}

fn stop_motion(world: &mut World, execution: &mut Execution) {
    if !execution.flags.uses_pather() {
        execution.move_input = Vec3::ZERO;
    }
    execution.phase = Phase::Active;
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.base.velocity = Vec3::ZERO;
        squad.mode = SquadMode::Power;
        squad.state = SquadState::Working;
    }
}

fn charge_upkeep(world: &mut World, dt: f32, execution: &mut Execution) -> bool {
    execution.elapsed_seconds += dt;
    while execution.elapsed_seconds > execution.next_tick_time {
        let cost = if execution.phase == Phase::Attacking {
            execution.supplies_per_tick_attacking
        } else {
            execution.supplies_per_tick
        };
        if !execution.flags.ignores_requirements() && !pay_supplies(world, execution, cost) {
            return false;
        }
        execution.next_tick_time += execution.tick_length;
    }
    true
}

fn pay_supplies(world: &mut World, execution: &Execution, cost: f32) -> bool {
    let Some(player) = world.get_player_mut(execution.player_id) else {
        return false;
    };
    if player.resources.get(execution.supplies_resource_id) < cost {
        return false;
    }
    player
        .resources
        .subtract(execution.supplies_resource_id, cost);
    true
}

pub(super) fn submit_input_by_execution(
    world: &mut World,
    database: &Database,
    execution_id: PowerExecutionId,
    input: NativePowerInput,
    no_cost: bool,
) -> bool {
    submit_input(world, database, input, no_cost, |execution| {
        execution.id == execution_id
    })
}

pub(super) fn submit_input_by_user(
    world: &mut World,
    database: &Database,
    power_user_id: PowerUserId,
    input: NativePowerInput,
    no_cost: bool,
) -> bool {
    if !power_user_id.is_valid() {
        return false;
    }
    submit_input(world, database, input, no_cost, |execution| {
        execution.power_user_id == power_user_id
    })
}

fn submit_input(
    world: &mut World,
    database: &Database,
    input: NativePowerInput,
    no_cost: bool,
    matches: impl Fn(&Execution) -> bool,
) -> bool {
    let active = std::mem::take(&mut world.power_manager.rage_executions);
    let mut remaining = Vec::with_capacity(active.len());
    let mut accepted = false;
    for mut execution in active {
        if !accepted && matches(&execution) {
            let result = handle_input(world, database, &mut execution, input, no_cost);
            accepted = result.accepted;
            if result.keep_running {
                remaining.push(execution);
            } else {
                shutdown(world, &mut execution);
            }
        } else {
            remaining.push(execution);
        }
    }
    world.power_manager.rage_executions = remaining;
    accepted
}

#[derive(Debug, Clone, Copy)]
struct InputResult {
    accepted: bool,
    keep_running: bool,
}

fn handle_input(
    world: &mut World,
    database: &Database,
    execution: &mut Execution,
    input: NativePowerInput,
    no_cost: bool,
) -> InputResult {
    match input {
        NativePowerInput::Shutdown => InputResult {
            accepted: true,
            keep_running: false,
        },
        NativePowerInput::Direction(direction) => {
            direction_input(world, database, execution, direction, no_cost)
        }
        NativePowerInput::Position(position) => position_input(world, execution, position),
        NativePowerInput::Confirm(_) => InputResult {
            accepted: true,
            keep_running: true,
        },
    }
}

fn direction_input(
    world: &mut World,
    database: &Database,
    execution: &mut Execution,
    direction: Vec3,
    no_cost: bool,
) -> InputResult {
    if !direction.is_finite() {
        return rejected_input();
    }
    if execution.teleport_remaining > 0.0 || execution.retarget_remaining > 0.0 {
        return accepted_input();
    }
    let ignores_cost = no_cost || execution.flags.ignores_requirements();
    if !ignores_cost && !can_afford_supplies(world, execution, execution.supplies_per_jump) {
        return InputResult {
            accepted: true,
            keep_running: false,
        };
    }
    if targeting::begin_jump(world, database, execution, direction) && !ignores_cost {
        let _paid = pay_supplies(world, execution, execution.supplies_per_jump);
    }
    accepted_input()
}

fn position_input(world: &mut World, execution: &mut Execution, position: Vec3) -> InputResult {
    if !position.is_finite() {
        return rejected_input();
    }
    if execution.teleport_remaining > 0.0 {
        return accepted_input();
    }
    execution.target_squad_id = EntityId::INVALID;
    execution.phase = Phase::Active;
    if execution.flags.uses_pather() {
        execution.move_target = position;
        execution.move_input = Vec3::ZERO;
    } else {
        execution.move_input = position;
    }
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.remove_all_orders();
        squad.mode = SquadMode::Power;
        squad.state = SquadState::Working;
    }
    accepted_input()
}

fn accepted_input() -> InputResult {
    InputResult {
        accepted: true,
        keep_running: true,
    }
}

fn rejected_input() -> InputResult {
    InputResult {
        accepted: false,
        keep_running: true,
    }
}

fn can_afford_supplies(world: &World, execution: &Execution, cost: f32) -> bool {
    world
        .get_player(execution.player_id)
        .is_some_and(|player| player.resources.get(execution.supplies_resource_id) >= cost)
}

fn shutdown(world: &mut World, execution: &mut Execution) {
    aura::clear(world, execution);
    for attachment_id in std::mem::take(&mut execution.hand_attachment_ids) {
        let _removed = world.remove_object(attachment_id);
    }
    let unit_ids = world
        .get_squad(execution.owner_squad_id)
        .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
    adjust_owner_units(world, execution, &unit_ids, true);
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.remove_all_orders();
        if squad.rage.end() {
            squad.mode = SquadMode::Normal;
            if squad.is_alive() {
                squad.state = SquadState::Idle;
            }
        }
    }
}

fn adjust_owner_units(world: &mut World, execution: &Execution, ids: &[EntityId], reset: bool) {
    let values = [
        (UnitDataScalar::Damage, execution.damage_multiplier),
        (
            UnitDataScalar::DamageTaken,
            execution.damage_taken_multiplier,
        ),
        (UnitDataScalar::Velocity, execution.speed_multiplier),
    ];
    for id in ids {
        let Some(unit) = world.get_unit_mut(*id) else {
            continue;
        };
        for (scalar, value) in values {
            unit.adjust_data_scalar(scalar, if reset { value.recip() } else { value });
        }
    }
}

fn owner_snapshot(world: &World, player_id: u8, squad_id: EntityId) -> Option<(EntityId, Vec3)> {
    let squad = world
        .get_squad(squad_id)
        .filter(|squad| squad.base.player_id == player_id && squad.is_alive())?;
    if squad.garrison.is_garrisoned() {
        return None;
    }
    let leader_id = squad
        .unit_ids
        .iter()
        .find(|id| {
            world
                .get_unit(**id)
                .is_some_and(crate::entities::Unit::is_operational)
        })
        .copied()?;
    Some((leader_id, squad.base.position))
}

fn owner_leader_id(world: &World, squad_id: EntityId) -> Option<EntityId> {
    world.get_squad(squad_id)?.unit_ids.iter().find_map(|id| {
        world
            .get_unit(*id)
            .filter(|unit| unit.is_alive())
            .map(|_| *id)
    })
}

fn target_is_alive(world: &World, squad_id: EntityId) -> bool {
    world
        .get_squad(squad_id)
        .filter(|squad| squad.is_alive())
        .and_then(|squad| squad.unit_ids.iter().find_map(|id| world.get_unit(*id)))
        .is_some_and(crate::entities::Unit::is_attackable)
}

fn set_attack_order(world: &mut World, owner_squad_id: EntityId, target_squad_id: EntityId) {
    if let Some(squad) = world.get_squad_mut(owner_squad_id) {
        squad.attack_target = Some(target_squad_id);
        squad.attack_range = 0.0;
        squad.attack_ability_id = None;
        squad.move_target = None;
        squad.mode = SquadMode::Power;
        squad.state = SquadState::Attacking;
        squad.base.velocity = Vec3::ZERO;
    }
}

pub(super) fn resolve_pending_kills(world: &mut World, database: Option<&Database>) {
    combat::resolve_pending_kills(world, database);
}

#[cfg(test)]
mod tests;
