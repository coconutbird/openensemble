//! Retail `BPowerCarpetBombing` targeting, bomber, bomb-train, and impact lifecycle.

use super::common::{
    Bomber, BomberProfile, PowerPayment, consume_power, create_power_visual, hash_string,
    validate_power_type, validate_requirements,
};
use super::projectile::{PowerProjectileLaunch, launch_power_projectile};
use super::{
    NativePowerError, NativePowerInput, NativePowerInvocation, PowerExecutionId, power_by_id,
};
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::gameplay::GameplayCatalog;
use crate::player::{PlayerId, ProtoPowerId};
use crate::sync::SyncChecksum;
use crate::world::{GeneralEvent, GeneralEventType, World};
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

mod profile;

use profile::CarpetBombingProfile;

const CARPET_BOMBING_POWER_TYPE: u32 = 3;
const BOMB_TICK_SECONDS: f32 = 0.067;
const BOMB_REVEALER_RADIUS: f32 = 10.0;
const BOMB_REVEALER_LIFETIME_MS: u32 = 5_000;
const PROJECTILE_LAUNCH_HEIGHT: f32 = 5.0;
const MAX_AUTHORED_BOMB_CLUSTERS: u32 = 10_000;

/// Inputs accepted by retail's direct `InvokePower2` Carpet Bombing path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarpetBombingPowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
    pub power_user_id: PowerUserId,
}

impl From<CarpetBombingPowerInvocation> for NativePowerInvocation {
    fn from(invocation: CarpetBombingPowerInvocation) -> Self {
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

/// Error returned by Carpet Bombing invocation.
pub type CarpetBombingPowerError = NativePowerError;

/// Synchronized phase of one retail Carpet Bombing execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarpetBombingPhase {
    WaitingForInputs,
    Active,
}

/// One ground impact marker waiting for its authored fuse.
#[derive(Debug, Clone, Copy)]
pub struct CarpetBomb {
    impact_object_id: EntityId,
    position: Vec3,
    explode_at_seconds: f32,
}

impl CarpetBomb {
    #[must_use]
    pub const fn impact_object_id(&self) -> EntityId {
        self.impact_object_id
    }

    #[must_use]
    pub const fn position(&self) -> Vec3 {
        self.position
    }

    #[must_use]
    pub const fn explode_at_seconds(&self) -> f32 {
        self.explode_at_seconds
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.impact_object_id.as_u32());
        hash_vec3(checksum, self.position);
        checksum.hash_f32(self.explode_at_seconds);
    }
}

/// Authoritative state for one running retail `BPowerCarpetBombing`.
#[derive(Debug, Clone)]
pub struct CarpetBombingPowerExecution {
    id: PowerExecutionId,
    player_id: PlayerId,
    proto_power_id: ProtoPowerId,
    power_level: u32,
    owner_squad_id: EntityId,
    power_user_id: PowerUserId,
    target_location: Vec3,
    phase: CarpetBombingPhase,
    start_location: Option<Vec3>,
    start_direction: Option<Vec3>,
    right: Vec3,
    bomber: Option<Bomber>,
    projectile_prototype: String,
    impact_prototype: String,
    explosion_prototype: String,
    bomber_profile: Option<BomberProfile>,
    requires_los: bool,
    initial_delay: f32,
    fuse_time: f32,
    maximum_bomb_clusters: u32,
    maximum_bomb_offset: f32,
    bomb_spacing: f32,
    length_multiplier: f32,
    wedge_length_multiplier: f32,
    wedge_minimum_offset: f32,
    nudge_multiplier: f32,
    elapsed_seconds: f32,
    next_bomb_time: f32,
    last_bomb_time: f32,
    bomb_clusters_dropped: u32,
    pending_bombs: Vec<CarpetBomb>,
    active_projectile_ids: Vec<EntityId>,
    nudged_unit_ids: Vec<EntityId>,
}

impl CarpetBombingPowerExecution {
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
    pub const fn power_user_id(&self) -> PowerUserId {
        self.power_user_id
    }

    #[must_use]
    pub const fn target_location(&self) -> Vec3 {
        self.target_location
    }

    #[must_use]
    pub const fn phase(&self) -> CarpetBombingPhase {
        self.phase
    }

    #[must_use]
    pub const fn start_location(&self) -> Option<Vec3> {
        self.start_location
    }

    #[must_use]
    pub const fn start_direction(&self) -> Option<Vec3> {
        self.start_direction
    }

    #[must_use]
    pub const fn bomber_object_id(&self) -> Option<EntityId> {
        match &self.bomber {
            Some(bomber) => Some(bomber.object_id()),
            None => None,
        }
    }

    #[must_use]
    pub fn bomber_prototype(&self) -> Option<&str> {
        self.bomber.as_ref().map(Bomber::prototype)
    }

    #[must_use]
    pub const fn maximum_bomb_clusters(&self) -> u32 {
        self.maximum_bomb_clusters
    }

    #[must_use]
    pub const fn bomb_clusters_dropped(&self) -> u32 {
        self.bomb_clusters_dropped
    }

    #[must_use]
    pub fn pending_bombs(&self) -> &[CarpetBomb] {
        &self.pending_bombs
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
        checksum.hash_u32(self.phase as u32);
        hash_optional_vec3(checksum, self.start_location);
        hash_optional_vec3(checksum, self.start_direction);
        hash_vec3(checksum, self.right);
        if let Some(bomber) = &self.bomber {
            checksum.hash_u32(1);
            bomber.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        for value in [
            &self.projectile_prototype,
            &self.impact_prototype,
            &self.explosion_prototype,
        ] {
            hash_string(checksum, value);
        }
        if let Some(profile) = &self.bomber_profile {
            checksum.hash_u32(1);
            profile.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::from(self.requires_los));
        for value in [
            self.initial_delay,
            self.fuse_time,
            self.maximum_bomb_offset,
            self.bomb_spacing,
            self.length_multiplier,
            self.wedge_length_multiplier,
            self.wedge_minimum_offset,
            self.nudge_multiplier,
            self.elapsed_seconds,
            self.next_bomb_time,
            self.last_bomb_time,
        ] {
            checksum.hash_f32(value);
        }
        checksum.hash_u32(self.maximum_bomb_clusters);
        checksum.hash_u32(self.bomb_clusters_dropped);
        hash_entity_ids(checksum, &self.active_projectile_ids);
        hash_entity_ids(checksum, &self.nudged_unit_ids);
        checksum.hash_u32(u32::try_from(self.pending_bombs.len()).unwrap_or(u32::MAX));
        for bomb in &self.pending_bombs {
            bomb.hash_state(checksum);
        }
    }
}

impl World {
    /// Validate, pay for, and begin one synchronized Carpet Bombing session.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed scenario-layered data, placement,
    /// ownership, cost, or a mismatched packed power-user ID.
    pub fn invoke_carpet_bombing_power(
        &mut self,
        database: &Database,
        invocation: CarpetBombingPowerInvocation,
    ) -> Result<PowerExecutionId, CarpetBombingPowerError> {
        invoke(self, database, invocation.into())
    }

    /// Submit direct synchronized input using a stable simulation execution ID.
    pub fn submit_carpet_bombing_power_input(
        &mut self,
        database: &Database,
        execution_id: PowerExecutionId,
        input: NativePowerInput,
    ) -> bool {
        submit_input_by_execution(self, database, execution_id, input)
    }
}

pub(super) fn invoke(
    world: &mut World,
    database: &Database,
    invocation: NativePowerInvocation,
) -> Result<PowerExecutionId, NativePowerError> {
    validate_power_user(invocation)?;
    let power = power_by_id(database, invocation.proto_power_id)
        .ok_or(NativePowerError::PowerNotFound(invocation.proto_power_id))?;
    let attributes = validate_power_type(power, "CarpetBombing")?;
    let target_location = validate_location(world, attributes, invocation.target_location)?;
    if world.get_player(invocation.player_id).is_none() {
        return Err(NativePowerError::PlayerNotFound(invocation.player_id));
    }
    let profile = CarpetBombingProfile::resolve(database, attributes, invocation.power_level)?;
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
    world
        .power_manager
        .carpet_bombing_executions
        .push(create_execution(id, invocation, target_location, profile));
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
    _no_cost: bool,
) -> bool {
    if !power_user_id.is_valid() {
        return false;
    }
    submit_input(world, database, input, |execution| {
        execution.power_user_id == power_user_id
    })
}

fn submit_input_by_execution(
    world: &mut World,
    database: &Database,
    execution_id: PowerExecutionId,
    input: NativePowerInput,
) -> bool {
    submit_input(world, database, input, |execution| {
        execution.id == execution_id
    })
}

fn submit_input(
    world: &mut World,
    database: &Database,
    input: NativePowerInput,
    matches: impl Fn(&CarpetBombingPowerExecution) -> bool,
) -> bool {
    let Some(index) = world
        .power_manager
        .carpet_bombing_executions
        .iter()
        .position(matches)
    else {
        return false;
    };
    let mut execution = world.power_manager.carpet_bombing_executions.remove(index);
    let accepted = handle_input(world, database, &mut execution, input);
    world
        .power_manager
        .carpet_bombing_executions
        .insert(index, execution);
    accepted
}

fn handle_input(
    world: &mut World,
    database: &Database,
    execution: &mut CarpetBombingPowerExecution,
    input: NativePowerInput,
) -> bool {
    if execution.phase == CarpetBombingPhase::Active {
        return matches!(
            input,
            NativePowerInput::Position(_)
                | NativePowerInput::Direction(_)
                | NativePowerInput::Shutdown
        );
    }
    match input {
        NativePowerInput::Position(position) => {
            if !position.is_finite() {
                return false;
            }
            execution.start_location = Some(position);
            try_activate(world, database, execution)
        }
        NativePowerInput::Direction(direction) => {
            let direction = horizontal_direction(direction);
            let Some(direction) = direction else {
                return false;
            };
            execution.start_direction = Some(direction);
            try_activate(world, database, execution)
        }
        NativePowerInput::Shutdown => true,
        NativePowerInput::Confirm(_) => false,
    }
}

fn try_activate(
    world: &mut World,
    database: &Database,
    execution: &mut CarpetBombingPowerExecution,
) -> bool {
    let (Some(start), Some(direction)) = (execution.start_location, execution.start_direction)
    else {
        return true;
    };
    let Some(attributes) =
        power_by_id(database, execution.proto_power_id).and_then(|power| power.attributes.as_ref())
    else {
        return false;
    };
    let Ok(start) = validate_power_line(
        world,
        attributes,
        start,
        direction,
        execution.length_multiplier,
    ) else {
        return false;
    };
    execution.start_location = Some(start);
    execution.right = Vec3::Y.cross(direction).normalize_or(Vec3::X);
    execution.next_bomb_time = execution.elapsed_seconds + execution.initial_delay;
    let Some(profile) = execution.bomber_profile.take() else {
        return false;
    };
    execution.bomber = Some(Bomber::spawn(
        world,
        database,
        execution.player_id,
        start,
        direction,
        profile,
    ));
    execution.phase = CarpetBombingPhase::Active;
    true
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
    let mut active = std::mem::take(&mut world.power_manager.carpet_bombing_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active.drain(..) {
        execution.elapsed_seconds += dt;
        if execution.phase == CarpetBombingPhase::Active {
            update_active_execution(world, database, gameplay, &mut execution, dt);
        }
        if execution_is_complete(world, &execution) {
            if let Some(bomber) = &mut execution.bomber {
                bomber.kill(world);
            }
        } else {
            remaining.push(execution);
        }
    }
    world.power_manager.carpet_bombing_executions = remaining;
}

fn update_active_execution(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut CarpetBombingPowerExecution,
    dt: f32,
) {
    let start = execution
        .start_location
        .unwrap_or(execution.target_location);
    let direction = execution.start_direction.unwrap_or(Vec3::Z);
    if let Some(bomber) = &mut execution.bomber {
        bomber.update(world, start, direction, execution.elapsed_seconds, dt);
    }
    while execution.elapsed_seconds > execution.next_bomb_time
        && execution.bomb_clusters_dropped < execution.maximum_bomb_clusters
    {
        drop_bomb_cluster(world, database, execution);
        execution.last_bomb_time = execution.elapsed_seconds;
        execution.next_bomb_time += BOMB_TICK_SECONDS;
    }
    explode_ready_bombs(world, database, gameplay, execution);
    execution
        .active_projectile_ids
        .retain(|projectile_id| world.get_projectile(*projectile_id).is_some());
}

fn drop_bomb_cluster(
    world: &mut World,
    database: &Database,
    execution: &mut CarpetBombingPowerExecution,
) {
    let count = execution.bomb_clusters_dropped;
    let count_ratio = count.to_f32().unwrap_or(f32::MAX)
        / execution.maximum_bomb_clusters.to_f32().unwrap_or(1.0);
    let position_multiplier =
        count_ratio * execution.length_multiplier - execution.wedge_length_multiplier;
    let maximum_offset = current_maximum_offset(execution, position_multiplier);
    let first_offset = world.trigger_random_float(-maximum_offset, 0.0);
    create_pending_bomb(
        world,
        database,
        execution,
        position_multiplier,
        first_offset,
    );
    let second_offset =
        world.trigger_random_float(first_offset + execution.bomb_spacing, maximum_offset);
    create_pending_bomb(
        world,
        database,
        execution,
        position_multiplier,
        second_offset,
    );
    execution.bomb_clusters_dropped = execution.bomb_clusters_dropped.saturating_add(1);
}

fn current_maximum_offset(
    execution: &CarpetBombingPowerExecution,
    position_multiplier: f32,
) -> f32 {
    if position_multiplier >= 0.0 {
        return execution.maximum_bomb_offset;
    }
    let wedge = execution.wedge_length_multiplier;
    execution.wedge_minimum_offset * (-position_multiplier / wedge)
        + execution.maximum_bomb_offset * (-(position_multiplier + wedge) / wedge)
}

fn create_pending_bomb(
    world: &mut World,
    database: &Database,
    execution: &mut CarpetBombingPowerExecution,
    position_multiplier: f32,
    lateral_offset: f32,
) {
    let start = execution
        .start_location
        .unwrap_or(execution.target_location);
    let direction = execution.start_direction.unwrap_or(Vec3::Z);
    let mut position = start + direction * position_multiplier + execution.right * lateral_offset;
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    let impact_object_id = create_power_visual(
        world,
        database,
        execution.player_id,
        position,
        -direction,
        &execution.impact_prototype,
    );
    create_bomb_revealer(world, database, execution.player_id, position);
    execution.pending_bombs.push(CarpetBomb {
        impact_object_id,
        position,
        explode_at_seconds: execution.elapsed_seconds + execution.fuse_time,
    });
}

fn create_bomb_revealer(
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
        BOMB_REVEALER_RADIUS,
        Some(BOMB_REVEALER_LIFETIME_MS),
    );
}

fn explode_ready_bombs(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut CarpetBombingPowerExecution,
) {
    while execution
        .pending_bombs
        .first()
        .is_some_and(|bomb| bomb.explode_at_seconds <= execution.elapsed_seconds)
    {
        let bomb = execution.pending_bombs.remove(0);
        let direction = execution.start_direction.unwrap_or(Vec3::Z);
        let _explosion = create_power_visual(
            world,
            database,
            execution.player_id,
            bomb.position,
            -direction,
            &execution.explosion_prototype,
        );
        let projectile_id = launch_damage_projectile(world, gameplay, execution, bomb.position);
        if !projectile_id.is_invalid() {
            execution.active_projectile_ids.push(projectile_id);
        }
    }
}

fn launch_damage_projectile(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &CarpetBombingPowerExecution,
    target: Vec3,
) -> EntityId {
    let source = target + Vec3::Y * PROJECTILE_LAUNCH_HEIGHT;
    launch_power_projectile(
        world,
        gameplay,
        PowerProjectileLaunch {
            execution_id: execution.id,
            player_id: execution.player_id,
            source_id: EntityId::INVALID,
            target_id: EntityId::INVALID,
            tactics_prototype: &execution.projectile_prototype,
            source,
            target,
            target_entity_position: target,
            target_offset: Vec3::ZERO,
            damage_bonus: 0.0,
            collides_with_all_units: true,
        },
    )
}

pub(super) fn notify_projectile_impact(
    world: &mut World,
    execution_id: u32,
    projectile_id: EntityId,
    position: Vec3,
    damaged_unit_ids: &[EntityId],
) {
    let Some(index) = world
        .power_manager
        .carpet_bombing_executions
        .iter()
        .position(|execution| execution.id.get() == execution_id)
    else {
        return;
    };
    let mut execution = world.power_manager.carpet_bombing_executions.remove(index);
    execution
        .active_projectile_ids
        .retain(|active| *active != projectile_id);
    nudge_damaged_units(world, &mut execution, position, damaged_unit_ids);
    world
        .power_manager
        .carpet_bombing_executions
        .insert(index, execution);
}

fn nudge_damaged_units(
    world: &mut World,
    execution: &mut CarpetBombingPowerExecution,
    origin: Vec3,
    damaged_unit_ids: &[EntityId],
) {
    if execution.nudge_multiplier <= 0.0 {
        return;
    }
    for &unit_id in damaged_unit_ids {
        if execution.nudged_unit_ids.contains(&unit_id) {
            continue;
        }
        let Some((mass, point, impulse)) = world.get_unit(unit_id).and_then(|unit| {
            let body = unit.physics.as_ref()?;
            let mut radial = Vec3::new(
                unit.base.position.x - origin.x,
                0.0,
                unit.base.position.z - origin.z,
            )
            .normalize_or(Vec3::Z);
            let point = unit.base.position + body.collider().center_offset
                - radial * unit.obstruction_radius();
            radial.y = 1.0;
            Some((
                body.material().mass.max(0.0),
                point,
                radial * execution.nudge_multiplier,
            ))
        }) else {
            continue;
        };
        if world
            .get_unit_mut(unit_id)
            .is_some_and(|unit| unit.apply_impulse_at_point(impulse * mass, point))
        {
            execution.nudged_unit_ids.push(unit_id);
        }
    }
}

fn execution_is_complete(world: &World, execution: &CarpetBombingPowerExecution) -> bool {
    execution.phase == CarpetBombingPhase::Active
        && execution.bomb_clusters_dropped >= execution.maximum_bomb_clusters
        && execution.pending_bombs.is_empty()
        && execution
            .active_projectile_ids
            .iter()
            .all(|projectile_id| world.get_projectile(*projectile_id).is_none())
}

fn create_execution(
    id: PowerExecutionId,
    invocation: NativePowerInvocation,
    target_location: Vec3,
    profile: CarpetBombingProfile,
) -> CarpetBombingPowerExecution {
    CarpetBombingPowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        power_user_id: invocation.power_user_id,
        target_location,
        phase: CarpetBombingPhase::WaitingForInputs,
        start_location: None,
        start_direction: None,
        right: Vec3::ZERO,
        bomber: None,
        projectile_prototype: profile.projectile_prototype,
        impact_prototype: profile.impact_prototype,
        explosion_prototype: profile.explosion_prototype,
        bomber_profile: Some(profile.bomber),
        requires_los: profile.requires_los,
        initial_delay: profile.initial_delay,
        fuse_time: profile.fuse_time,
        maximum_bomb_clusters: profile.maximum_bomb_clusters,
        maximum_bomb_offset: profile.maximum_bomb_offset,
        bomb_spacing: profile.bomb_spacing,
        length_multiplier: profile.length_multiplier,
        wedge_length_multiplier: profile.wedge_length_multiplier,
        wedge_minimum_offset: profile.wedge_minimum_offset,
        nudge_multiplier: profile.nudge_multiplier,
        elapsed_seconds: 0.0,
        next_bomb_time: 0.0,
        last_bomb_time: 0.0,
        bomb_clusters_dropped: 0,
        pending_bombs: Vec::new(),
        active_projectile_ids: Vec::new(),
        nudged_unit_ids: Vec::new(),
    }
}

fn validate_power_user(invocation: NativePowerInvocation) -> Result<(), NativePowerError> {
    let id = invocation.power_user_id;
    if id.is_valid()
        && (id.player_id() != i32::from(invocation.player_id)
            || id.power_type() != CARPET_BOMBING_POWER_TYPE)
    {
        return Err(NativePowerError::InvalidData("PowerUserID"));
    }
    Ok(())
}

fn validate_location(
    world: &World,
    attributes: &PowerAttributes,
    mut location: Vec3,
) -> Result<Vec3, NativePowerError> {
    if !location.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    if world.is_outside_playable_bounds(location, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(id) = super::disruption::disrupting_power_at(world, attributes, location) {
        return Err(NativePowerError::Disrupted(id));
    }
    if let Some(height) = world.terrain_height(location, true) {
        location.y = height;
    }
    Ok(location)
}

fn validate_power_line(
    world: &World,
    attributes: &PowerAttributes,
    start: Vec3,
    direction: Vec3,
    length: f32,
) -> Result<Vec3, NativePowerError> {
    let start = validate_location(world, attributes, start)?;
    let end = start + direction * length;
    if world.is_outside_playable_bounds(end, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(id) = super::disruption::disrupting_power_segment(world, attributes, start, end) {
        return Err(NativePowerError::Disrupted(id));
    }
    Ok(start)
}

fn horizontal_direction(direction: Vec3) -> Option<Vec3> {
    if !direction.is_finite() {
        return None;
    }
    Vec3::new(direction.x, 0.0, direction.z).try_normalize()
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        hash_vec3(checksum, value);
    } else {
        checksum.hash_u32(0);
    }
}

fn hash_entity_ids(checksum: &mut SyncChecksum, values: &[EntityId]) {
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_u32(value.as_u32());
    }
}

#[cfg(test)]
mod tests;
