//! Retail `BPowerWave` gravity-ball movement, pulling, and explosion lifecycle.

mod combat;
mod debris;
mod profile;
mod pulling;

pub use debris::WaveFakeObject;

use super::common::{consume_power, validate_power_type, validate_requirements};
use super::{
    NativePowerError, NativePowerInput, NativePowerInvocation, PowerExecutionId, power_by_id,
};
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::entities::{SquadMode, SquadState};
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::player::{PlayerId, ProtoPowerId, TeamId};
use crate::spawn::{object_prototype_id, spawn_object_at};
use crate::sync::SyncChecksum;
use crate::world::{GeneralEvent, GeneralEventType, World};
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;
use profile::WaveProfile;

const WAVE_POWER_TYPE: u32 = 6;
const BALL_CENTER_HEIGHT: f32 = 12.5;
const BALL_HEIGHT_VARIANCE: f32 = 2.5;
const BALL_PERIOD_MS: u32 = 3_000;

/// Inputs accepted by retail's direct `InvokePower2` Wave path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WavePowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
    pub power_user_id: PowerUserId,
}

impl From<WavePowerInvocation> for NativePowerInvocation {
    fn from(invocation: WavePowerInvocation) -> Self {
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

/// Error returned by Wave invocation.
pub type WavePowerError = NativePowerError;

/// Synchronized retail gravity-ball animation/gameplay state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WaveGravityBallState {
    Stagnant = 1,
    Pulling = 2,
    PullingFull = 3,
    Exploding = 4,
}

#[derive(Debug, Clone, Copy)]
pub struct WaveCapturedObject {
    unit_id: EntityId,
    pickup_attachment_id: EntityId,
    color_player_id: PlayerId,
    was_auto_attackable: bool,
}

impl WaveCapturedObject {
    #[must_use]
    pub const fn unit_id(&self) -> EntityId {
        self.unit_id
    }

    #[must_use]
    pub const fn pickup_attachment_id(&self) -> EntityId {
        self.pickup_attachment_id
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct QueuedWaveObject {
    pub unit_id: EntityId,
    pub add_time: f32,
}

/// Authoritative state for one running retail `BPowerWave` execution.
#[derive(Debug, Clone)]
pub struct WavePowerExecution {
    id: PowerExecutionId,
    player_id: PlayerId,
    proto_power_id: ProtoPowerId,
    power_level: u32,
    owner_squad_id: EntityId,
    power_user_id: PowerUserId,
    target_location: Vec3,
    desired_ball_position: Vec3,
    ball_object_id: EntityId,
    state: WaveGravityBallState,
    profile: WaveProfile,
    ignore_requirements: bool,
    elapsed_seconds: f32,
    next_tick_time: f32,
    explode_cooldown_left: f32,
    current_explosion_damage_bank: f32,
    maximum_possible_explosion_damage_bank: f32,
    explosion_requested: bool,
    units_to_pull: Vec<EntityId>,
    queued_pickup_objects: Vec<QueuedWaveObject>,
    captured_objects: Vec<WaveCapturedObject>,
    fake_objects: Vec<WaveFakeObject>,
    active_projectile_ids: Vec<EntityId>,
    revealed_team_ids: Vec<TeamId>,
    reveal_entity_id: EntityId,
}

impl WavePowerExecution {
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
    pub const fn desired_ball_position(&self) -> Vec3 {
        self.desired_ball_position
    }

    #[must_use]
    pub const fn ball_object_id(&self) -> EntityId {
        self.ball_object_id
    }

    #[must_use]
    pub const fn state(&self) -> WaveGravityBallState {
        self.state
    }

    #[must_use]
    pub fn ball_prototype(&self) -> &str {
        &self.profile.ball_prototype
    }

    #[must_use]
    pub fn lightning_projectile_prototype(&self) -> &str {
        &self.profile.lightning_projectile
    }

    #[must_use]
    pub fn debris_projectile_prototype(&self) -> &str {
        &self.profile.debris_projectile
    }

    #[must_use]
    pub fn explode_projectile_prototype(&self) -> &str {
        &self.profile.explode_projectile
    }

    #[must_use]
    pub fn lightning_beam_prototype(&self) -> Option<&str> {
        self.profile.lightning_beam_visual.as_deref()
    }

    #[must_use]
    pub const fn tick_length(&self) -> f32 {
        self.profile.tick_length
    }

    #[must_use]
    pub const fn supplies_per_tick(&self) -> f32 {
        self.profile.supplies_per_tick
    }

    #[must_use]
    pub const fn pulling_range(&self) -> f32 {
        self.profile.pulling_range
    }

    #[must_use]
    pub const fn maximum_ball_speed_pulling(&self) -> f32 {
        self.profile.maximum_ball_speed_pulling
    }

    #[must_use]
    pub const fn command_interval_ms(&self) -> u32 {
        self.profile.command_interval_ms
    }

    #[must_use]
    pub const fn minimum_ball_distance(&self) -> f32 {
        self.profile.minimum_ball_distance
    }

    #[must_use]
    pub const fn maximum_ball_distance(&self) -> f32 {
        self.profile.maximum_ball_distance
    }

    #[must_use]
    pub const fn maximum_captured_objects(&self) -> usize {
        self.profile.maximum_captured_objects
    }

    #[must_use]
    pub const fn current_explosion_damage_bank(&self) -> f32 {
        self.current_explosion_damage_bank
    }

    #[must_use]
    pub const fn maximum_possible_explosion_damage_bank(&self) -> f32 {
        self.maximum_possible_explosion_damage_bank
    }

    #[must_use]
    pub const fn explode_cooldown_left(&self) -> f32 {
        self.explode_cooldown_left
    }

    #[must_use]
    pub fn captured_objects(&self) -> &[WaveCapturedObject] {
        &self.captured_objects
    }

    /// Short-lived damage parts pulled by the ball without entering its capture list.
    #[must_use]
    pub fn fake_objects(&self) -> &[WaveFakeObject] {
        &self.fake_objects
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
        hash_vec3(checksum, self.desired_ball_position);
        checksum.hash_u32(self.ball_object_id.as_u32());
        checksum.hash_u32(self.state as u32);
        self.profile.hash_state(checksum);
        checksum.hash_u32(u32::from(self.ignore_requirements));
        checksum.hash_f32(self.elapsed_seconds);
        checksum.hash_f32(self.next_tick_time);
        checksum.hash_f32(self.explode_cooldown_left);
        checksum.hash_f32(self.current_explosion_damage_bank);
        checksum.hash_f32(self.maximum_possible_explosion_damage_bank);
        checksum.hash_u32(u32::from(self.explosion_requested));
        hash_entity_ids(checksum, &self.units_to_pull);
        hash_queued(checksum, &self.queued_pickup_objects);
        hash_captured(checksum, &self.captured_objects);
        debris::hash_fake_objects(checksum, &self.fake_objects);
        hash_entity_ids(checksum, &self.active_projectile_ids);
        checksum.hash_u32(u32::try_from(self.revealed_team_ids.len()).unwrap_or(u32::MAX));
        for team_id in &self.revealed_team_ids {
            checksum.hash_u32(u32::from(*team_id));
        }
        checksum.hash_u32(self.reveal_entity_id.as_u32());
    }
}

impl World {
    /// Validate, pay for, and begin one synchronized Wave gravity ball.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed profile data, an unusable owner,
    /// failed requirements, invalid placement, or packed-user mismatch.
    pub fn invoke_wave_power(
        &mut self,
        database: &Database,
        invocation: WavePowerInvocation,
    ) -> Result<PowerExecutionId, WavePowerError> {
        invoke(self, database, invocation.into())
    }

    /// Submit direct synchronized input using a stable simulation execution ID.
    pub fn submit_wave_power_input(
        &mut self,
        database: &Database,
        execution_id: PowerExecutionId,
        input: NativePowerInput,
    ) -> bool {
        submit_input_by_execution(self, database, execution_id, input, false)
    }

    pub(in crate::world) fn wave_forces_entity_visibility(
        &self,
        team_id: TeamId,
        entity_id: EntityId,
    ) -> bool {
        self.power_manager.wave_executions.iter().any(|execution| {
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
    let attributes = validate_power_type(power, "Wave")?;
    validate_owner(world, invocation)?;
    validate_location(world, attributes, invocation.target_location)?;
    let owner_position = world
        .get_squad(invocation.squad_id)
        .map(|squad| squad.base.position)
        .ok_or(NativePowerError::InvalidTarget)?;
    validate_location(world, attributes, owner_position)?;
    let profile = WaveProfile::resolve(database, attributes, invocation.power_level)?;
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
    let ball_object_id = spawn_wave_ball(world, database, invocation, &profile.ball_prototype)?;
    let mut execution = create_execution(id, invocation, ball_object_id, profile);
    activate_owner(world, &execution);
    update_revealed_teams(world, &mut execution);
    world.power_manager.wave_executions.push(execution);
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
    matches: impl Fn(&WavePowerExecution) -> bool,
) -> bool {
    let mut accepted = false;
    for execution in &mut world.power_manager.wave_executions {
        if accepted || !matches(execution) {
            continue;
        }
        accepted = match input {
            NativePowerInput::Shutdown | NativePowerInput::Confirm(_) => {
                execution.explosion_requested = true;
                true
            }
            NativePowerInput::Position(position) if position.is_finite() => {
                if execution.state != WaveGravityBallState::Exploding {
                    execution.desired_ball_position = position;
                }
                true
            }
            NativePowerInput::Position(_) | NativePowerInput::Direction(_) => false,
        };
    }
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
    let active = std::mem::take(&mut world.power_manager.wave_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active {
        if update_execution(world, dt, database, gameplay, &mut execution) {
            remaining.push(execution);
        } else {
            cleanup_execution(world, &mut execution);
        }
    }
    world.power_manager.wave_executions = remaining;
}

fn update_execution(
    world: &mut World,
    dt: f32,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
) -> bool {
    execution.elapsed_seconds += dt;
    debris::update_fake_objects(world, execution, dt);
    if execution.state == WaveGravityBallState::Exploding {
        execution.explode_cooldown_left -= dt;
        return execution.explode_cooldown_left > 0.0;
    }
    if execution.explosion_requested {
        return combat::explode(world, gameplay, execution);
    }
    if !execution_is_usable(world, database, gameplay, execution) {
        execution.explosion_requested = true;
        return combat::explode(world, gameplay, execution);
    }
    move_ball(world, execution, dt);
    update_revealed_teams(world, execution);
    pulling::update(world, database, gameplay, execution, dt);
    execution
        .active_projectile_ids
        .retain(|projectile_id| world.get_projectile(*projectile_id).is_some());
    if execution.explosion_requested {
        combat::explode(world, gameplay, execution)
    } else {
        true
    }
}

fn execution_is_usable(
    world: &World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &WavePowerExecution,
) -> bool {
    world.get_player(execution.player_id).is_some()
        && owner_is_usable(world, execution)
        && world.get_unit(execution.ball_object_id).is_some()
        && super::projectile::first_power_attack(gameplay, &execution.profile.explode_projectile)
            .is_some()
        && super::projectile::first_power_attack(gameplay, &execution.profile.lightning_projectile)
            .is_some()
        && super::projectile::first_power_attack(gameplay, &execution.profile.debris_projectile)
            .is_some()
        && power_by_id(database, execution.proto_power_id)
            .and_then(|power| power.attributes.as_ref())
            .is_some_and(|attributes| locations_remain_valid(world, attributes, execution))
}

fn move_ball(world: &mut World, execution: &WavePowerExecution, dt: f32) {
    let Some(current) = world
        .get_unit(execution.ball_object_id)
        .map(|ball| ball.base.position)
    else {
        return;
    };
    let distance = current.distance(execution.desired_ball_position);
    let travel = execution.profile.maximum_ball_speed_pulling * dt;
    let planar = Vec3::new(
        execution.desired_ball_position.x - current.x,
        0.0,
        execution.desired_ball_position.z - current.z,
    );
    let mut position = execution.desired_ball_position;
    if distance >= travel && planar.length_squared() > f32::EPSILON {
        position = current + planar.normalize() * travel;
    }
    position = clamp_ball_position(world, position);
    let direction = Vec3::new(position.x - current.x, 0.0, position.z - current.z);
    if let Some(ball) = world.get_unit_mut(execution.ball_object_id) {
        ball.base.set_position(position);
        if direction.length_squared() > f32::EPSILON {
            ball.base.set_forward(direction.normalize());
        }
    }
    face_owner_toward_ball(world, execution.owner_squad_id, position);
}

fn clamp_ball_position(world: &World, mut position: Vec3) -> Vec3 {
    if let Some(bounds) = world.effective_playable_bounds() {
        position.x = position.x.clamp(bounds.min_x(), bounds.max_x());
        position.z = position.z.clamp(bounds.min_z(), bounds.max_z());
    }
    let terrain = world.terrain_height(position, true).unwrap_or_default();
    let phase = (world.game_time_ms % BALL_PERIOD_MS)
        .to_f32()
        .unwrap_or_default()
        / 3_000.0;
    position.y =
        terrain + BALL_CENTER_HEIGHT + (phase * std::f32::consts::TAU).sin() * BALL_HEIGHT_VARIANCE;
    position
}

fn face_owner_toward_ball(world: &mut World, owner_squad_id: EntityId, ball: Vec3) {
    let leader_id = owner_leader_id(world, owner_squad_id);
    let Some(position) = world.get_unit(leader_id).map(|unit| unit.base.position) else {
        return;
    };
    let direction = Vec3::new(ball.x - position.x, 0.0, ball.z - position.z).normalize_or_zero();
    if direction != Vec3::ZERO
        && let Some(leader) = world.get_unit_mut(leader_id)
    {
        leader.base.set_forward(direction);
    }
}

fn update_revealed_teams(world: &World, execution: &mut WavePowerExecution) {
    let mut team_ids = world
        .players()
        .map(|player| player.team_id)
        .collect::<Vec<_>>();
    team_ids.sort_unstable();
    team_ids.dedup();
    team_ids.retain(|team_id| world.is_entity_visible_to_team(*team_id, execution.ball_object_id));
    execution.revealed_team_ids = team_ids;
    execution.reveal_entity_id = owner_leader_id(world, execution.owner_squad_id);
}

fn validate_owner(
    world: &World,
    invocation: NativePowerInvocation,
) -> Result<(), NativePowerError> {
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

fn owner_is_usable(world: &World, execution: &WavePowerExecution) -> bool {
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
    position: Vec3,
) -> Result<(), NativePowerError> {
    if !position.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    if world.is_outside_playable_bounds(position, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(id) = super::disruption::disrupting_power_at(world, attributes, position) {
        return Err(NativePowerError::Disrupted(id));
    }
    Ok(())
}

fn locations_remain_valid(
    world: &World,
    attributes: &PowerAttributes,
    execution: &WavePowerExecution,
) -> bool {
    let Some(ball_position) = world.entity_position(execution.ball_object_id) else {
        return false;
    };
    let Some(owner_position) = world.entity_position(execution.owner_squad_id) else {
        return false;
    };
    validate_location(world, attributes, ball_position).is_ok()
        && validate_location(world, attributes, owner_position).is_ok()
}

fn activate_owner(world: &mut World, execution: &WavePowerExecution) {
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.remove_all_orders();
        squad.base.velocity = Vec3::ZERO;
        squad.mode = SquadMode::Power;
        squad.state = SquadState::Working;
    }
}

fn cleanup_execution(world: &mut World, execution: &mut WavePowerExecution) {
    let _ball = world.remove_unit(execution.ball_object_id);
    execution.ball_object_id = EntityId::INVALID;
    for captured in &execution.captured_objects {
        let _attachment = world.remove_object(captured.pickup_attachment_id);
        if let Some(unit) = world.get_unit_mut(captured.unit_id) {
            unit.set_auto_attackable(captured.was_auto_attackable);
        }
    }
    execution.captured_objects.clear();
    for fake in execution.fake_objects.drain(..) {
        let _part = world.remove_unit(fake.unit_id());
    }
    execution.queued_pickup_objects.clear();
    replace_units_to_pull(world, execution, Vec::new());
    if let Some(squad) = world.get_squad_mut(execution.owner_squad_id) {
        squad.remove_all_orders();
        squad.base.velocity = Vec3::ZERO;
        squad.mode = SquadMode::Normal;
        if squad.is_alive() {
            squad.state = SquadState::Idle;
        }
    }
}

fn replace_units_to_pull(
    world: &mut World,
    execution: &mut WavePowerExecution,
    units_to_pull_next: Vec<EntityId>,
) {
    for unit_id in &execution.units_to_pull {
        if !units_to_pull_next.contains(unit_id)
            && let Some(unit) = world.get_unit_mut(*unit_id)
        {
            unit.set_aircraft_can_kamikaze(true);
        }
    }
    for unit_id in &units_to_pull_next {
        if !execution.units_to_pull.contains(unit_id)
            && let Some(unit) = world.get_unit_mut(*unit_id)
        {
            unit.set_aircraft_can_kamikaze(false);
        }
    }
    execution.units_to_pull = units_to_pull_next;
}

fn create_execution(
    id: PowerExecutionId,
    invocation: NativePowerInvocation,
    ball_object_id: EntityId,
    profile: WaveProfile,
) -> WavePowerExecution {
    let maximum_possible_explosion_damage_bank = profile
        .maximum_captured_objects
        .to_f32()
        .unwrap_or(f32::MAX)
        * profile.maximum_explosion_damage_bank_per_captured;
    WavePowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        power_user_id: invocation.power_user_id,
        target_location: invocation.target_location,
        desired_ball_position: invocation.target_location,
        ball_object_id,
        state: WaveGravityBallState::Pulling,
        profile,
        ignore_requirements: invocation.ignore_requirements,
        elapsed_seconds: 0.0,
        next_tick_time: 0.0,
        explode_cooldown_left: -1.0,
        current_explosion_damage_bank: 0.0,
        maximum_possible_explosion_damage_bank,
        explosion_requested: false,
        units_to_pull: Vec::new(),
        queued_pickup_objects: Vec::new(),
        captured_objects: Vec::new(),
        fake_objects: Vec::new(),
        active_projectile_ids: Vec::new(),
        revealed_team_ids: Vec::new(),
        reveal_entity_id: EntityId::INVALID,
    }
}

fn validate_power_user(invocation: NativePowerInvocation) -> Result<(), NativePowerError> {
    let id = invocation.power_user_id;
    if id.is_valid()
        && (id.player_id() != i32::from(invocation.player_id) || id.power_type() != WAVE_POWER_TYPE)
    {
        return Err(NativePowerError::InvalidData("PowerUserID"));
    }
    Ok(())
}

pub(super) fn owner_leader_id(world: &World, squad_id: EntityId) -> EntityId {
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

fn spawn_wave_ball(
    world: &mut World,
    database: &Database,
    invocation: NativePowerInvocation,
    prototype: &str,
) -> Result<EntityId, NativePowerError> {
    let prototype_id = object_prototype_id(database, prototype)
        .ok_or_else(|| NativePowerError::UnknownPrototype(prototype.to_owned()))?;
    spawn_object_at(
        world,
        database,
        invocation.player_id,
        prototype_id,
        invocation.target_location,
        Vec3::Z,
    )
    .map_err(|_| NativePowerError::InvalidData("BallObject"))
}

pub(super) fn ball_position(world: &World, execution: &WavePowerExecution) -> Vec3 {
    world
        .get_unit(execution.ball_object_id)
        .map_or(execution.desired_ball_position, |ball| ball.base.position)
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

fn hash_queued(checksum: &mut SyncChecksum, values: &[QueuedWaveObject]) {
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_u32(value.unit_id.as_u32());
        checksum.hash_f32(value.add_time);
    }
}

fn hash_captured(checksum: &mut SyncChecksum, values: &[WaveCapturedObject]) {
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_u32(value.unit_id.as_u32());
        checksum.hash_u32(value.pickup_attachment_id.as_u32());
        checksum.hash_u32(u32::from(value.color_player_id));
        checksum.hash_u32(u32::from(value.was_auto_attackable));
    }
}

#[cfg(test)]
mod tests;
