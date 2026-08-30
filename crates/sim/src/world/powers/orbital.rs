//! Retail `BPowerOrbital` targeting, shot queue, projectile, and impact lifecycle.

use super::common::{
    PowerPayment, consume_power, create_power_visual, hash_string, validate_power_type,
    validate_requirements,
};
use super::projectile::{PowerProjectileLaunch, launch_power_projectile};
use super::{
    NativePowerError, NativePowerInput, NativePowerInvocation, PowerExecutionId, power_by_id, rules,
};
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::gameplay::GameplayCatalog;
use crate::player::{PlayerId, ProtoPowerId};
use crate::sync::SyncChecksum;
use crate::world::{GeneralEvent, GeneralEventType, World};
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

mod debris;
mod profile;

use profile::OrbitalProfile;

const ORBITAL_POWER_TYPE: u32 = 2;
const REVEALER_RADIUS: f32 = 12.0;
const REVEALER_LIFETIME_MS: u32 = 10_000;
pub(super) const MAX_AUTHORED_SHOTS: u32 = 10_000;

/// Inputs accepted by retail's direct `InvokePower2` Orbital path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitalPowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
    pub power_user_id: PowerUserId,
}

impl From<OrbitalPowerInvocation> for NativePowerInvocation {
    fn from(invocation: OrbitalPowerInvocation) -> Self {
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

/// Error returned by Orbital invocation.
pub type OrbitalPowerError = NativePowerError;

/// One synchronized Orbital shot waiting for its targeting delay.
#[derive(Debug, Clone, Copy)]
pub struct OrbitalShot {
    launch_position: Vec3,
    target_position: Vec3,
    launch_time_ms: u32,
    create_laser_time_ms: u32,
    laser_object_id: EntityId,
    laser_created: bool,
}

impl OrbitalShot {
    #[must_use]
    pub const fn launch_position(&self) -> Vec3 {
        self.launch_position
    }

    #[must_use]
    pub const fn target_position(&self) -> Vec3 {
        self.target_position
    }

    #[must_use]
    pub const fn launch_time_ms(&self) -> u32 {
        self.launch_time_ms
    }

    #[must_use]
    pub const fn laser_object_id(&self) -> EntityId {
        self.laser_object_id
    }

    #[must_use]
    pub const fn laser_created(&self) -> bool {
        self.laser_created
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        hash_vec3(checksum, self.launch_position);
        hash_vec3(checksum, self.target_position);
        checksum.hash_u32(self.launch_time_ms);
        checksum.hash_u32(self.create_laser_time_ms);
        checksum.hash_u32(self.laser_object_id.as_u32());
        checksum.hash_u32(u32::from(self.laser_created));
    }
}

/// Authoritative state for one running retail `BPowerOrbital` execution.
#[derive(Debug, Clone)]
pub struct OrbitalPowerExecution {
    id: PowerExecutionId,
    player_id: PlayerId,
    proto_power_id: ProtoPowerId,
    power_level: u32,
    owner_squad_id: EntityId,
    power_user_id: PowerUserId,
    target_location: Vec3,
    desired_targeting_position: Vec3,
    real_targeting_laser_id: EntityId,
    target_beam_prototype: String,
    target_beam_speed: f32,
    projectile_prototype: String,
    effect_prototype: String,
    rock_small_prototype: String,
    rock_medium_prototype: String,
    rock_large_prototype: String,
    shots_remaining: u32,
    impacts_to_process: u32,
    fired_initial_shot: bool,
    ignore_requirements: bool,
    payment: Option<PowerPayment>,
    targeting_delay_ms: u32,
    auto_shot_delay_ms: u32,
    auto_shot_inner_radius: f32,
    auto_shot_outer_radius: f32,
    launch_offset: Vec3,
    requires_los: bool,
    elapsed_seconds: f32,
    shots: Vec<OrbitalShot>,
    active_projectile_ids: Vec<EntityId>,
}

impl OrbitalPowerExecution {
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
    pub const fn desired_targeting_position(&self) -> Vec3 {
        self.desired_targeting_position
    }

    #[must_use]
    pub const fn real_targeting_laser_id(&self) -> EntityId {
        self.real_targeting_laser_id
    }

    #[must_use]
    pub const fn shots_remaining(&self) -> u32 {
        self.shots_remaining
    }

    #[must_use]
    pub const fn impacts_to_process(&self) -> u32 {
        self.impacts_to_process
    }

    #[must_use]
    pub const fn fired_initial_shot(&self) -> bool {
        self.fired_initial_shot
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
    pub fn pending_shots(&self) -> &[OrbitalShot] {
        &self.shots
    }

    #[must_use]
    pub fn active_projectile_ids(&self) -> &[EntityId] {
        &self.active_projectile_ids
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id.get());
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_u32(self.power_level);
        checksum.hash_u32(self.owner_squad_id.as_u32());
        checksum.hash_u32(self.power_user_id.raw());
        hash_vec3(checksum, self.target_location);
        hash_vec3(checksum, self.desired_targeting_position);
        checksum.hash_u32(self.real_targeting_laser_id.as_u32());
        for value in [
            &self.target_beam_prototype,
            &self.projectile_prototype,
            &self.effect_prototype,
            &self.rock_small_prototype,
            &self.rock_medium_prototype,
            &self.rock_large_prototype,
        ] {
            hash_string(checksum, value);
        }
        checksum.hash_f32(self.target_beam_speed);
        checksum.hash_u32(self.shots_remaining);
        checksum.hash_u32(self.impacts_to_process);
        checksum.hash_u32(u32::from(self.fired_initial_shot));
        checksum.hash_u32(u32::from(self.ignore_requirements));
        hash_payment(checksum, self.payment);
        checksum.hash_u32(self.targeting_delay_ms);
        checksum.hash_u32(self.auto_shot_delay_ms);
        checksum.hash_f32(self.auto_shot_inner_radius);
        checksum.hash_f32(self.auto_shot_outer_radius);
        hash_vec3(checksum, self.launch_offset);
        checksum.hash_u32(u32::from(self.requires_los));
        checksum.hash_f32(self.elapsed_seconds);
        checksum.hash_u32(u32::try_from(self.shots.len()).unwrap_or(u32::MAX));
        for shot in &self.shots {
            shot.hash_state(checksum);
        }
        hash_entity_ids(checksum, &self.active_projectile_ids);
    }
}

impl World {
    /// Validate and begin one synchronized Orbital targeting session.
    ///
    /// Retail defers payment until the first confirmed shot.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed layered data, ownership, requirements,
    /// placement, or a mismatched packed power-user ID.
    pub fn invoke_orbital_power(
        &mut self,
        database: &Database,
        invocation: OrbitalPowerInvocation,
    ) -> Result<PowerExecutionId, OrbitalPowerError> {
        invoke(self, database, invocation.into())
    }

    /// Submit direct synchronized input using a stable simulation execution ID.
    pub fn submit_orbital_power_input(
        &mut self,
        database: &Database,
        execution_id: PowerExecutionId,
        input: NativePowerInput,
    ) -> bool {
        submit_input_by_execution(self, database, execution_id, input, false)
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
    let attributes = validate_power_type(power, "Orbital")?;
    let target = initial_target(world, invocation.target_location)?;
    let profile = OrbitalProfile::resolve(database, attributes, invocation.power_level)?;
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
    let id = world.power_manager.allocate_id();
    let beam_id = create_power_visual(
        world,
        database,
        invocation.player_id,
        target,
        Vec3::Z,
        &profile.target_beam_prototype,
    );
    world
        .power_manager
        .orbital_executions
        .push(create_execution(
            id, invocation, target, beam_id, profile, payment,
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
    database: &Database,
    input: NativePowerInput,
    no_cost: bool,
    matches: impl Fn(&OrbitalPowerExecution) -> bool,
) -> bool {
    let Some(index) = world
        .power_manager
        .orbital_executions
        .iter()
        .position(matches)
    else {
        return false;
    };
    let mut execution = world.power_manager.orbital_executions.remove(index);
    let accepted = handle_input(world, database, &mut execution, input, no_cost);
    world
        .power_manager
        .orbital_executions
        .insert(index, execution);
    accepted
}

fn handle_input(
    world: &mut World,
    database: &Database,
    execution: &mut OrbitalPowerExecution,
    input: NativePowerInput,
    no_cost: bool,
) -> bool {
    match input {
        NativePowerInput::Confirm(target) => {
            confirm_shot(world, database, execution, target, no_cost)
        }
        NativePowerInput::Position(position) => {
            if !position.is_finite() {
                return false;
            }
            execution.desired_targeting_position = position;
            true
        }
        NativePowerInput::Shutdown => {
            execution.shots_remaining = 0;
            remove_real_targeting_laser(world, execution);
            true
        }
        NativePowerInput::Direction(_) => false,
    }
}

fn confirm_shot(
    world: &mut World,
    database: &Database,
    execution: &mut OrbitalPowerExecution,
    target: Vec3,
    no_cost: bool,
) -> bool {
    let Some(power) = power_by_id(database, execution.proto_power_id) else {
        return false;
    };
    let Some(attributes) = power.attributes.as_ref() else {
        return false;
    };
    let Ok(target) = validate_shot_target(world, attributes, target) else {
        return false;
    };
    if execution.shots_remaining == 0 {
        return true;
    }
    if execution.fired_initial_shot {
        restart_recharge(world, power, execution);
    } else {
        if !consume_initial_shot(world, power, execution, no_cost) {
            return false;
        }
        execution.fired_initial_shot = true;
        let _fired = world.fire_general_event(&GeneralEvent::new(
            GeneralEventType::UsedPower,
            i32::from(execution.player_id),
        ));
    }
    queue_shot(world, execution, target);
    true
}

fn consume_initial_shot(
    world: &mut World,
    power: &pipeline::database::hw1::Power,
    execution: &mut OrbitalPowerExecution,
    no_cost: bool,
) -> bool {
    if execution.ignore_requirements || no_cost {
        execution.payment = None;
        return true;
    }
    let Some(payment) = execution.payment.take() else {
        return false;
    };
    if consume_power(
        world,
        power,
        execution.player_id,
        execution.proto_power_id,
        execution.owner_squad_id,
        &payment,
    )
    .is_ok()
    {
        true
    } else {
        execution.payment = Some(payment);
        false
    }
}

fn restart_recharge(
    world: &mut World,
    power: &pipeline::database::hw1::Power,
    execution: &OrbitalPowerExecution,
) {
    if execution.ignore_requirements {
        return;
    }
    let auto_recharge_ms = power
        .attributes
        .as_ref()
        .and_then(|attributes| attributes.auto_recharge)
        .unwrap_or_default();
    let game_time_ms = world.game_time_ms;
    if let Some(player) = world.get_player_mut(execution.player_id) {
        let _restarted = player.restart_power_recharge(
            execution.proto_power_id,
            EntityId::INVALID,
            rules(power),
            auto_recharge_ms,
            game_time_ms,
        );
    }
}

fn queue_shot(world: &mut World, execution: &mut OrbitalPowerExecution, target: Vec3) {
    let current_time = world.game_time_ms;
    execution.shots.push(OrbitalShot {
        launch_position: target + execution.launch_offset,
        target_position: target,
        launch_time_ms: current_time.wrapping_add(execution.targeting_delay_ms),
        create_laser_time_ms: current_time,
        laser_object_id: EntityId::INVALID,
        laser_created: false,
    });
    execution.shots_remaining = execution.shots_remaining.saturating_sub(1);
    execution.impacts_to_process = execution.impacts_to_process.saturating_add(1);
    execution.desired_targeting_position = target;
    if execution.shots_remaining == 0 {
        remove_real_targeting_laser(world, execution);
    }
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
    debris::update(world, dt);
    let mut active = std::mem::take(&mut world.power_manager.orbital_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active.drain(..) {
        if world.get_player(execution.player_id).is_none() {
            cleanup_execution(world, &mut execution);
            continue;
        }
        execution.elapsed_seconds += dt;
        update_targeting_laser(world, &execution, dt);
        reconcile_missing_projectiles(world, &mut execution);
        process_ready_shots(world, database, gameplay, &mut execution);
        if execution_is_complete(&execution) {
            cleanup_execution(world, &mut execution);
        } else {
            remaining.push(execution);
        }
    }
    world.power_manager.orbital_executions = remaining;
}

fn update_targeting_laser(world: &mut World, execution: &OrbitalPowerExecution, dt: f32) {
    let Some(current) = world
        .get_object(execution.real_targeting_laser_id)
        .map(|laser| laser.base.position)
    else {
        return;
    };
    let desired = clamp_beam_position(world, execution.desired_targeting_position);
    let delta = Vec3::new(desired.x - current.x, 0.0, desired.z - current.z);
    let distance = delta.length();
    let travel = execution.target_beam_speed * dt;
    let mut position = if distance <= travel || distance <= f32::EPSILON {
        desired
    } else if travel > 0.0 {
        current + delta / distance * travel
    } else {
        current
    };
    position = clamp_beam_position(world, position);
    if let Some(laser) = world.get_object_mut(execution.real_targeting_laser_id) {
        laser.base.set_position(position);
    }
}

fn process_ready_shots(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut OrbitalPowerExecution,
) {
    let current_time = world.game_time_ms;
    let mut index = 0;
    while index < execution.shots.len() {
        create_shot_laser_if_ready(world, database, execution, index, current_time);
        if current_time < execution.shots[index].launch_time_ms {
            index += 1;
            continue;
        }
        let shot = execution.shots.remove(index);
        if !shot.laser_object_id.is_invalid() {
            let _removed = world.remove_object(shot.laser_object_id);
        }
        let projectile_id = launch_orbital_projectile(world, gameplay, execution, shot);
        if projectile_id.is_invalid() {
            execution.impacts_to_process = execution.impacts_to_process.saturating_sub(1);
            continue;
        }
        execution.active_projectile_ids.push(projectile_id);
        let _effect = create_power_visual(
            world,
            database,
            execution.player_id,
            shot.target_position,
            Vec3::Z,
            &execution.effect_prototype,
        );
        create_impact_revealer(world, database, execution.player_id, shot.target_position);
    }
}

fn create_shot_laser_if_ready(
    world: &mut World,
    database: &Database,
    execution: &mut OrbitalPowerExecution,
    index: usize,
    current_time: u32,
) {
    let shot = &mut execution.shots[index];
    if shot.laser_created || current_time < shot.create_laser_time_ms {
        return;
    }
    shot.laser_created = true;
    shot.laser_object_id = create_power_visual(
        world,
        database,
        execution.player_id,
        shot.target_position,
        Vec3::Z,
        &execution.target_beam_prototype,
    );
}

fn launch_orbital_projectile(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &OrbitalPowerExecution,
    shot: OrbitalShot,
) -> EntityId {
    launch_power_projectile(
        world,
        gameplay,
        PowerProjectileLaunch {
            execution_id: execution.id,
            player_id: execution.player_id,
            source_id: EntityId::INVALID,
            target_id: EntityId::INVALID,
            tactics_prototype: &execution.projectile_prototype,
            source: shot.launch_position,
            target: shot.target_position,
            target_entity_position: shot.target_position,
            target_offset: Vec3::ZERO,
            damage_bonus: 0.0,
            collides_with_all_units: true,
        },
    )
}

fn create_impact_revealer(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    position: Vec3,
) {
    let Some(team_id) = world.get_player(player_id).map(|player| player.team_id) else {
        return;
    };
    let _revealer = world.create_revealer(
        database,
        team_id,
        position,
        REVEALER_RADIUS,
        Some(REVEALER_LIFETIME_MS),
    );
}

fn reconcile_missing_projectiles(world: &World, execution: &mut OrbitalPowerExecution) {
    let previous = execution.active_projectile_ids.len();
    execution
        .active_projectile_ids
        .retain(|projectile_id| world.get_projectile(*projectile_id).is_some());
    let missing = previous.saturating_sub(execution.active_projectile_ids.len());
    execution.impacts_to_process = execution
        .impacts_to_process
        .saturating_sub(u32::try_from(missing).unwrap_or(u32::MAX));
}

pub(super) fn notify_projectile_impact(
    world: &mut World,
    database: Option<&Database>,
    execution_id: u32,
    projectile_id: EntityId,
    position: Vec3,
    direction: Vec3,
) {
    let Some(index) = world
        .power_manager
        .orbital_executions
        .iter()
        .position(|execution| execution.id.get() == execution_id)
    else {
        return;
    };
    let mut execution = world.power_manager.orbital_executions.remove(index);
    execution
        .active_projectile_ids
        .retain(|active| *active != projectile_id);
    execution.impacts_to_process = execution.impacts_to_process.saturating_sub(1);
    if let Some(database) = database {
        debris::spawn_impact(world, database, &execution, position, direction);
    }
    world
        .power_manager
        .orbital_executions
        .insert(index, execution);
}

fn execution_is_complete(execution: &OrbitalPowerExecution) -> bool {
    execution.shots.is_empty()
        && execution.shots_remaining == 0
        && execution.impacts_to_process == 0
}

fn cleanup_execution(world: &mut World, execution: &mut OrbitalPowerExecution) {
    remove_real_targeting_laser(world, execution);
    for shot in &execution.shots {
        if !shot.laser_object_id.is_invalid() {
            let _removed = world.remove_object(shot.laser_object_id);
        }
    }
}

fn remove_real_targeting_laser(world: &mut World, execution: &mut OrbitalPowerExecution) {
    if execution.real_targeting_laser_id.is_invalid() {
        return;
    }
    let _removed = world.remove_object(execution.real_targeting_laser_id);
    execution.real_targeting_laser_id = EntityId::INVALID;
}

fn create_execution(
    id: PowerExecutionId,
    invocation: NativePowerInvocation,
    target: Vec3,
    beam_id: EntityId,
    profile: OrbitalProfile,
    payment: Option<PowerPayment>,
) -> OrbitalPowerExecution {
    OrbitalPowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        power_user_id: invocation.power_user_id,
        target_location: target,
        desired_targeting_position: target,
        real_targeting_laser_id: beam_id,
        target_beam_prototype: profile.target_beam_prototype,
        target_beam_speed: profile.target_beam_speed,
        projectile_prototype: profile.projectile_prototype,
        effect_prototype: profile.effect_prototype,
        rock_small_prototype: profile.rock_small_prototype,
        rock_medium_prototype: profile.rock_medium_prototype,
        rock_large_prototype: profile.rock_large_prototype,
        shots_remaining: profile.shots,
        impacts_to_process: 0,
        fired_initial_shot: false,
        ignore_requirements: invocation.ignore_requirements,
        payment,
        targeting_delay_ms: profile.targeting_delay_ms,
        auto_shot_delay_ms: profile.auto_shot_delay_ms,
        auto_shot_inner_radius: profile.auto_shot_inner_radius,
        auto_shot_outer_radius: profile.auto_shot_outer_radius,
        launch_offset: profile.launch_offset,
        requires_los: profile.requires_los && !invocation.ignore_requirements,
        elapsed_seconds: 0.0,
        shots: Vec::new(),
        active_projectile_ids: Vec::new(),
    }
}

fn validate_power_user(invocation: NativePowerInvocation) -> Result<(), NativePowerError> {
    let id = invocation.power_user_id;
    if id.is_valid()
        && (id.player_id() != i32::from(invocation.player_id)
            || id.power_type() != ORBITAL_POWER_TYPE)
    {
        return Err(NativePowerError::InvalidData("PowerUserID"));
    }
    Ok(())
}

fn initial_target(world: &World, mut target: Vec3) -> Result<Vec3, NativePowerError> {
    if !target.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    if world.is_outside_playable_bounds(target, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(height) = world.terrain_height(target, true) {
        target.y = height;
    }
    Ok(target)
}

fn validate_shot_target(
    world: &World,
    attributes: &PowerAttributes,
    mut target: Vec3,
) -> Result<Vec3, NativePowerError> {
    if !target.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    if world.is_outside_playable_bounds(target, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(id) = super::disruption::disrupting_power_at(world, attributes, target) {
        return Err(NativePowerError::Disrupted(id));
    }
    if let Some(height) = world.terrain_height(target, true) {
        target.y = height;
    }
    Ok(target)
}

fn clamp_beam_position(world: &World, mut position: Vec3) -> Vec3 {
    if let Some(bounds) = world.effective_playable_bounds() {
        position.x = position.x.clamp(bounds.min_x(), bounds.max_x());
        position.z = position.z.clamp(bounds.min_z(), bounds.max_z());
    }
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    position
}

fn hash_payment(checksum: &mut SyncChecksum, payment: Option<PowerPayment>) {
    if let Some(payment) = payment {
        checksum.hash_u32(1);
        payment.hash_state(checksum);
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
