//! Retail `BPowerCleansing` beam movement, upkeep, and damage-tick lifecycle.

mod physics;
mod profile;

use super::common::{
    consume_power, create_power_visual, hash_string, validate_power_type, validate_requirements,
};
use super::projectile::{PowerProjectileLaunch, first_power_attack, launch_power_projectile};
use super::{
    NativePowerError, NativePowerInput, NativePowerInvocation, PowerExecutionId, power_by_id,
};
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::entities::{SquadMode, SquadState};
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::player::{PlayerId, ProtoPowerId, TeamId};
use crate::sync::SyncChecksum;
use crate::world::{GeneralEvent, GeneralEventType, World};
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;
use profile::CleansingProfile;

const CLEANSING_POWER_TYPE: u32 = 1;
const PROJECTILE_LAUNCH_HEIGHT: f32 = 5.0;

/// Inputs accepted by retail's direct `InvokePower2` Cleansing path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CleansingPowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
    pub power_user_id: PowerUserId,
}

impl From<CleansingPowerInvocation> for NativePowerInvocation {
    fn from(invocation: CleansingPowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
            power_user_id: invocation.power_user_id,
        }
    }
}

/// Error returned by Cleansing invocation.
pub type CleansingPowerError = NativePowerError;

/// Authoritative state for one running retail `BPowerCleansing` execution.
#[derive(Debug, Clone)]
pub struct CleansingPowerExecution {
    id: PowerExecutionId,
    player_id: PlayerId,
    proto_power_id: ProtoPowerId,
    power_level: u32,
    owner_squad_id: EntityId,
    power_user_id: PowerUserId,
    target_location: Vec3,
    desired_beam_position: Vec3,
    beam_object_id: EntityId,
    air_impact_object_id: EntityId,
    beam_prototype: String,
    projectile_prototype: String,
    air_impact_prototype: Option<String>,
    tick_length: f32,
    supplies_per_tick: f32,
    supplies_resource_id: usize,
    minimum_beam_distance: f32,
    maximum_beam_distance: f32,
    command_interval_ms: u32,
    maximum_beam_speed: f32,
    requires_los: bool,
    ignore_requirements: bool,
    elapsed_seconds: f32,
    next_damage_time: f32,
    active_projectile_ids: Vec<EntityId>,
    revealed_team_ids: Vec<TeamId>,
    reveal_entity_id: EntityId,
}

impl CleansingPowerExecution {
    #[must_use]
    pub const fn id(&self) -> PowerExecutionId {
        self.id
    }

    #[must_use]
    pub const fn player_id(&self) -> PlayerId {
        self.player_id
    }

    #[must_use]
    pub const fn proto_power_id(&self) -> ProtoPowerId {
        self.proto_power_id
    }

    #[must_use]
    pub const fn power_level(&self) -> u32 {
        self.power_level
    }

    #[must_use]
    pub const fn owner_squad_id(&self) -> EntityId {
        self.owner_squad_id
    }

    #[must_use]
    pub const fn power_user_id(&self) -> PowerUserId {
        self.power_user_id
    }

    #[must_use]
    pub const fn target_location(&self) -> Vec3 {
        self.target_location
    }

    #[must_use]
    pub const fn desired_beam_position(&self) -> Vec3 {
        self.desired_beam_position
    }

    #[must_use]
    pub const fn beam_object_id(&self) -> EntityId {
        self.beam_object_id
    }

    #[must_use]
    pub const fn air_impact_object_id(&self) -> EntityId {
        self.air_impact_object_id
    }

    #[must_use]
    pub fn beam_prototype(&self) -> &str {
        &self.beam_prototype
    }

    #[must_use]
    pub fn projectile_prototype(&self) -> &str {
        &self.projectile_prototype
    }

    #[must_use]
    pub fn air_impact_prototype(&self) -> Option<&str> {
        self.air_impact_prototype.as_deref()
    }

    #[must_use]
    pub const fn tick_length(&self) -> f32 {
        self.tick_length
    }

    #[must_use]
    pub const fn supplies_per_tick(&self) -> f32 {
        self.supplies_per_tick
    }

    #[must_use]
    pub const fn minimum_beam_distance(&self) -> f32 {
        self.minimum_beam_distance
    }

    #[must_use]
    pub const fn maximum_beam_distance(&self) -> f32 {
        self.maximum_beam_distance
    }

    #[must_use]
    pub const fn command_interval_ms(&self) -> u32 {
        self.command_interval_ms
    }

    #[must_use]
    pub const fn maximum_beam_speed(&self) -> f32 {
        self.maximum_beam_speed
    }

    #[must_use]
    pub const fn requires_los(&self) -> bool {
        self.requires_los
    }

    #[must_use]
    pub const fn elapsed_seconds(&self) -> f32 {
        self.elapsed_seconds
    }

    #[must_use]
    pub const fn next_damage_time(&self) -> f32 {
        self.next_damage_time
    }

    #[must_use]
    pub fn active_projectile_ids(&self) -> &[EntityId] {
        &self.active_projectile_ids
    }

    #[must_use]
    pub fn revealed_team_ids(&self) -> &[TeamId] {
        &self.revealed_team_ids
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id.get());
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_u32(self.power_level);
        checksum.hash_u32(self.owner_squad_id.as_u32());
        checksum.hash_u32(self.power_user_id.raw());
        hash_vec3(checksum, self.target_location);
        hash_vec3(checksum, self.desired_beam_position);
        checksum.hash_u32(self.beam_object_id.as_u32());
        checksum.hash_u32(self.air_impact_object_id.as_u32());
        hash_string(checksum, &self.beam_prototype);
        hash_string(checksum, &self.projectile_prototype);
        hash_optional_string(checksum, self.air_impact_prototype.as_deref());
        checksum.hash_f32(self.tick_length);
        checksum.hash_f32(self.supplies_per_tick);
        checksum.hash_u32(u32::try_from(self.supplies_resource_id).unwrap_or(u32::MAX));
        checksum.hash_f32(self.minimum_beam_distance);
        checksum.hash_f32(self.maximum_beam_distance);
        checksum.hash_u32(self.command_interval_ms);
        checksum.hash_f32(self.maximum_beam_speed);
        checksum.hash_u32(u32::from(self.requires_los));
        checksum.hash_u32(u32::from(self.ignore_requirements));
        checksum.hash_f32(self.elapsed_seconds);
        checksum.hash_f32(self.next_damage_time);
        hash_entity_ids(checksum, &self.active_projectile_ids);
        checksum.hash_u32(u32::try_from(self.revealed_team_ids.len()).unwrap_or(u32::MAX));
        for team_id in &self.revealed_team_ids {
            checksum.hash_u32(u32::from(*team_id));
        }
        checksum.hash_u32(self.reveal_entity_id.as_u32());
    }
}

impl World {
    /// Validate, pay for, and begin one synchronized Cleansing beam.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed profile data, an unusable owner,
    /// failed requirements, invalid placement, or packed-user mismatch.
    pub fn invoke_cleansing_power(
        &mut self,
        database: &Database,
        invocation: CleansingPowerInvocation,
    ) -> Result<PowerExecutionId, CleansingPowerError> {
        invoke(self, database, invocation.into())
    }

    /// Submit direct synchronized input using a stable simulation execution ID.
    pub fn submit_cleansing_power_input(
        &mut self,
        database: &Database,
        execution_id: PowerExecutionId,
        input: NativePowerInput,
    ) -> bool {
        submit_input_by_execution(self, database, execution_id, input, false)
    }

    pub(in crate::world) fn cleansing_forces_entity_visibility(
        &self,
        team_id: TeamId,
        entity_id: EntityId,
    ) -> bool {
        self.power_manager
            .cleansing_executions
            .iter()
            .any(|execution| {
                execution.reveal_entity_id == entity_id
                    && execution.revealed_team_ids.contains(&team_id)
            })
    }
}

pub(super) fn invoke(
    world: &mut World,
    database: &Database,
    invocation: NativePowerInvocation,
) -> Result<PowerExecutionId, NativePowerError> {
    validate_power_user(invocation)?;
    if world.get_player(invocation.player_id).is_none() {
        return Err(NativePowerError::PlayerNotFound(invocation.player_id));
    }
    let power = power_by_id(database, invocation.proto_power_id)
        .ok_or(NativePowerError::PowerNotFound(invocation.proto_power_id))?;
    let attributes = validate_power_type(power, "Cleansing")?;
    let target = validate_initial_location(
        world,
        attributes,
        invocation.target_location,
        invocation.ignore_requirements,
    )?;
    validate_owner(world, invocation)?;
    let profile = CleansingProfile::resolve(database, attributes, invocation.power_level)?;
    let payment = if invocation.ignore_requirements {
        None
    } else {
        Some(validate_requirements(
            world,
            database,
            power,
            invocation.player_id,
            invocation.proto_power_id,
        )?)
    };
    if !invocation.ignore_requirements
        && let Some(owner_position) = owner_position(world, invocation.squad_id)
    {
        validate_location(world, attributes, owner_position)?;
    }
    if let Some(payment) = payment {
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
    let beam_id = create_power_visual(
        world,
        database,
        invocation.player_id,
        target,
        Vec3::Z,
        &profile.beam_prototype,
    );
    let mut execution = create_execution(id, invocation, target, beam_id, profile);
    activate_owner(world, &execution);
    update_revealed_teams(world, &mut execution);
    world.power_manager.cleansing_executions.push(execution);
    let _fired = world.fire_general_event(&GeneralEvent::new(
        GeneralEventType::UsedPower,
        i32::from(invocation.player_id),
    ));
    Ok(id)
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

fn submit_input_by_execution(
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

fn submit_input(
    world: &mut World,
    _database: &Database,
    input: NativePowerInput,
    _no_cost: bool,
    matches: impl Fn(&CleansingPowerExecution) -> bool,
) -> bool {
    let active = std::mem::take(&mut world.power_manager.cleansing_executions);
    let mut remaining = Vec::with_capacity(active.len());
    let mut accepted = false;
    for mut execution in active {
        if !accepted && matches(&execution) {
            match input {
                NativePowerInput::Shutdown => {
                    cleanup_execution(world, &mut execution);
                    accepted = true;
                }
                NativePowerInput::Position(position) if position.is_finite() => {
                    execution.desired_beam_position = position;
                    remaining.push(execution);
                    accepted = true;
                }
                NativePowerInput::Position(_)
                | NativePowerInput::Confirm(_)
                | NativePowerInput::Direction(_) => remaining.push(execution),
            }
        } else {
            remaining.push(execution);
        }
    }
    world.power_manager.cleansing_executions = remaining;
    accepted
}

pub(super) fn update(
    world: &mut World,
    dt: f32,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
) {
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let active = std::mem::take(&mut world.power_manager.cleansing_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active {
        if update_execution(world, dt, database, gameplay, &mut execution) {
            remaining.push(execution);
        } else {
            cleanup_execution(world, &mut execution);
        }
    }
    world.power_manager.cleansing_executions = remaining;
}

fn update_execution(
    world: &mut World,
    dt: f32,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut CleansingPowerExecution,
) -> bool {
    if world.get_player(execution.player_id).is_none()
        || world.get_object(execution.beam_object_id).is_none()
        || (!execution.ignore_requirements && !owner_is_usable(world, execution))
        || first_power_attack(gameplay, &execution.projectile_prototype).is_none()
    {
        return false;
    }
    let Some(attributes) =
        power_by_id(database, execution.proto_power_id).and_then(|power| power.attributes.as_ref())
    else {
        return false;
    };
    if !execution.ignore_requirements && !locations_remain_valid(world, attributes, execution) {
        return false;
    }
    move_beam(world, execution, dt);
    let current_beam_position = beam_position(world, execution);
    execution.air_impact_object_id = physics::update_air_impact(
        world,
        database,
        execution.player_id,
        execution.air_impact_prototype.as_deref(),
        current_beam_position,
        execution.air_impact_object_id,
    );
    update_revealed_teams(world, execution);
    execution.elapsed_seconds += dt;
    if !process_damage_ticks(world, gameplay, execution) {
        return false;
    }
    execution
        .active_projectile_ids
        .retain(|projectile_id| world.get_projectile(*projectile_id).is_some());
    true
}

fn process_damage_ticks(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut CleansingPowerExecution,
) -> bool {
    while execution.elapsed_seconds > execution.next_damage_time {
        if !execution.ignore_requirements && !pay_upkeep(world, execution) {
            return false;
        }
        let target = beam_position(world, execution);
        physics::impulse_air_units(world, target);
        let source_id = owner_leader_id(world, execution.owner_squad_id);
        let projectile_id = launch_power_projectile(
            world,
            gameplay,
            PowerProjectileLaunch {
                execution_id: execution.id,
                player_id: execution.player_id,
                source_id,
                target_id: EntityId::INVALID,
                tactics_prototype: &execution.projectile_prototype,
                source: target + Vec3::Y * PROJECTILE_LAUNCH_HEIGHT,
                target,
                target_entity_position: target,
                target_offset: Vec3::ZERO,
                damage_bonus: 0.0,
                collides_with_all_units: true,
            },
        );
        if !projectile_id.is_invalid() {
            execution.active_projectile_ids.push(projectile_id);
        }
        execution.next_damage_time += execution.tick_length;
    }
    true
}

fn pay_upkeep(world: &mut World, execution: &CleansingPowerExecution) -> bool {
    let Some(player) = world.get_player_mut(execution.player_id) else {
        return false;
    };
    if player.resources.get(execution.supplies_resource_id) < execution.supplies_per_tick {
        return false;
    }
    player
        .resources
        .subtract(execution.supplies_resource_id, execution.supplies_per_tick);
    true
}

fn move_beam(world: &mut World, execution: &CleansingPowerExecution, dt: f32) {
    let current = beam_position(world, execution);
    let desired = execution.desired_beam_position;
    let delta = Vec3::new(desired.x - current.x, 0.0, desired.z - current.z);
    let distance = delta.length();
    let travel = execution.maximum_beam_speed * dt;
    let mut position = if distance <= travel || distance <= f32::EPSILON {
        desired
    } else if travel > 0.0 {
        current + delta / distance * travel
    } else {
        current
    };
    position = clamp_position(world, position);
    if let Some(beam) = world.get_object_mut(execution.beam_object_id) {
        beam.base.set_position(position);
        if delta.length_squared() > f32::EPSILON {
            beam.base.set_forward(delta.normalize());
        }
    }
}

fn update_revealed_teams(world: &World, execution: &mut CleansingPowerExecution) {
    let mut team_ids = world
        .players()
        .map(|player| player.team_id)
        .collect::<Vec<_>>();
    team_ids.sort_unstable();
    team_ids.dedup();
    team_ids.retain(|team_id| world.is_entity_visible_to_team(*team_id, execution.beam_object_id));
    execution.revealed_team_ids = team_ids;
    let leader_id = owner_leader_id(world, execution.owner_squad_id);
    execution.reveal_entity_id = if leader_id.is_invalid() {
        execution.beam_object_id
    } else {
        leader_id
    };
}

fn locations_remain_valid(
    world: &World,
    attributes: &PowerAttributes,
    execution: &CleansingPowerExecution,
) -> bool {
    let beam = beam_position(world, execution);
    if validate_location(world, attributes, beam).is_err() {
        return false;
    }
    owner_position(world, execution.owner_squad_id)
        .is_some_and(|position| validate_location(world, attributes, position).is_ok())
}

fn validate_owner(
    world: &World,
    invocation: NativePowerInvocation,
) -> Result<(), NativePowerError> {
    if invocation.ignore_requirements {
        return Ok(());
    }
    let squad = world
        .get_squad(invocation.squad_id)
        .filter(|squad| squad.base.player_id == invocation.player_id)
        .ok_or(NativePowerError::InvalidTarget)?;
    if !squad.is_alive()
        || squad.garrison.is_garrisoned()
        || owner_leader_id(world, invocation.squad_id).is_invalid()
    {
        return Err(NativePowerError::PowerUnavailable);
    }
    Ok(())
}

fn owner_is_usable(world: &World, execution: &CleansingPowerExecution) -> bool {
    world
        .get_squad(execution.owner_squad_id)
        .is_some_and(|squad| {
            squad.base.player_id == execution.player_id
                && squad.is_alive()
                && !squad.garrison.is_garrisoned()
                && !owner_leader_id(world, execution.owner_squad_id).is_invalid()
        })
}

fn validate_location(
    world: &World,
    attributes: &PowerAttributes,
    mut position: Vec3,
) -> Result<Vec3, NativePowerError> {
    if !position.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    if world.is_outside_playable_bounds(position, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(id) = super::disruption::disrupting_power_at(world, attributes, position) {
        return Err(NativePowerError::Disrupted(id));
    }
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    Ok(position)
}

fn validate_initial_location(
    world: &World,
    attributes: &PowerAttributes,
    mut position: Vec3,
    ignore_requirements: bool,
) -> Result<Vec3, NativePowerError> {
    if !position.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    if world.is_outside_playable_bounds(position, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if !ignore_requirements
        && let Some(id) = super::disruption::disrupting_power_at(world, attributes, position)
    {
        return Err(NativePowerError::Disrupted(id));
    }
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    Ok(position)
}

fn activate_owner(world: &mut World, execution: &CleansingPowerExecution) {
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.remove_all_orders();
        squad.base.velocity = Vec3::ZERO;
        squad.mode = SquadMode::Power;
        squad.state = SquadState::Working;
    }
}

fn cleanup_execution(world: &mut World, execution: &mut CleansingPowerExecution) {
    let _beam = world.remove_object(execution.beam_object_id);
    let _impact = world.remove_object(execution.air_impact_object_id);
    execution.beam_object_id = EntityId::INVALID;
    execution.air_impact_object_id = EntityId::INVALID;
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.remove_all_orders();
        squad.base.velocity = Vec3::ZERO;
        squad.mode = SquadMode::Normal;
        if squad.is_alive() {
            squad.state = SquadState::Idle;
        }
    }
}

fn create_execution(
    id: PowerExecutionId,
    invocation: NativePowerInvocation,
    target: Vec3,
    beam_object_id: EntityId,
    profile: CleansingProfile,
) -> CleansingPowerExecution {
    CleansingPowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        power_user_id: invocation.power_user_id,
        target_location: target,
        desired_beam_position: target,
        beam_object_id,
        air_impact_object_id: EntityId::INVALID,
        beam_prototype: profile.beam_prototype,
        projectile_prototype: profile.projectile_prototype,
        air_impact_prototype: profile.air_impact_prototype,
        tick_length: profile.tick_length,
        supplies_per_tick: profile.supplies_per_tick,
        supplies_resource_id: profile.supplies_resource_id,
        minimum_beam_distance: profile.minimum_beam_distance,
        maximum_beam_distance: profile.maximum_beam_distance,
        command_interval_ms: profile.command_interval_ms,
        maximum_beam_speed: profile.maximum_beam_speed,
        requires_los: profile.requires_los && !invocation.ignore_requirements,
        ignore_requirements: invocation.ignore_requirements,
        elapsed_seconds: 0.0,
        next_damage_time: 0.0,
        active_projectile_ids: Vec::new(),
        revealed_team_ids: Vec::new(),
        reveal_entity_id: EntityId::INVALID,
    }
}

fn validate_power_user(invocation: NativePowerInvocation) -> Result<(), NativePowerError> {
    let id = invocation.power_user_id;
    if id.is_valid()
        && (id.player_id() != i32::from(invocation.player_id)
            || id.power_type() != CLEANSING_POWER_TYPE)
    {
        return Err(NativePowerError::InvalidData("PowerUserID"));
    }
    Ok(())
}

fn owner_position(world: &World, squad_id: EntityId) -> Option<Vec3> {
    world.get_squad(squad_id).map(|squad| squad.base.position)
}

fn owner_leader_id(world: &World, squad_id: EntityId) -> EntityId {
    world
        .get_squad(squad_id)
        .and_then(|squad| {
            squad.unit_ids.iter().find_map(|unit_id| {
                world
                    .get_unit(*unit_id)
                    .filter(|unit| unit.is_operational())
                    .map(|_| *unit_id)
            })
        })
        .unwrap_or(EntityId::INVALID)
}

fn beam_position(world: &World, execution: &CleansingPowerExecution) -> Vec3 {
    world
        .get_object(execution.beam_object_id)
        .map_or(execution.desired_beam_position, |beam| beam.base.position)
}

fn clamp_position(world: &World, mut position: Vec3) -> Vec3 {
    if let Some(bounds) = world.effective_playable_bounds() {
        position.x = position.x.clamp(bounds.min_x(), bounds.max_x());
        position.z = position.z.clamp(bounds.min_z(), bounds.max_z());
    }
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    position
}

fn hash_optional_string(checksum: &mut SyncChecksum, value: Option<&str>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        hash_string(checksum, value);
    } else {
        checksum.hash_u32(0);
    }
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}

fn hash_entity_ids(checksum: &mut SyncChecksum, values: &[EntityId]) {
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_u32(value.as_u32());
    }
}

#[cfg(test)]
mod tests;
