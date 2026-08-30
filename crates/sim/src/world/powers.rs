//! Database-backed player power ownership transitions.

use super::World;
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::gameplay::GameplayCatalog;
use crate::player::{PlayerId, PowerGrant, PowerRules, ProtoPowerId};
use crate::sync::SyncChecksum;
use glam::Vec3;
use pipeline::database::hw1::{Database, Power};
use thiserror::Error;

mod carpet_bombing;
mod cleansing;
mod common;
mod cryo;
mod disruption;
mod manager;
mod odst;
mod orbital;
mod projectile;
mod rage;
mod repair;
mod transport;
mod wave;

pub use carpet_bombing::{
    CarpetBomb, CarpetBombingPhase, CarpetBombingPowerError, CarpetBombingPowerExecution,
    CarpetBombingPowerInvocation,
};
pub use cleansing::{CleansingPowerError, CleansingPowerExecution, CleansingPowerInvocation};
pub use odst::{OdstDrop, OdstPowerError, OdstPowerExecution, OdstPowerInvocation};
pub use orbital::{OrbitalPowerError, OrbitalPowerExecution, OrbitalPowerInvocation, OrbitalShot};
pub use rage::{RagePowerExecution, RagePowerPhase};
pub use repair::RepairPowerExecution;
pub use transport::{TransportPowerError, TransportPowerExecution, TransportPowerInvocation};
pub use wave::{
    WaveCapturedObject, WaveFakeObject, WaveGravityBallState, WavePowerError, WavePowerExecution,
    WavePowerInvocation,
};

/// Stable simulation ID for one running native power execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PowerExecutionId(u32);

impl PowerExecutionId {
    /// Raw deterministic execution ID.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Inputs accepted by retail's direct `InvokePower2` native-power path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativePowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
    pub power_user_id: PowerUserId,
}

/// Live input accepted by an already-running native power.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NativePowerInput {
    Confirm(Vec3),
    Position(Vec3),
    Direction(Vec3),
    Shutdown,
}

/// Inputs accepted by retail's direct `InvokePower2` Cryo path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CryoPowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
}

impl From<CryoPowerInvocation> for NativePowerInvocation {
    fn from(invocation: CryoPowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
            power_user_id: PowerUserId::INVALID,
        }
    }
}

/// Inputs accepted by retail's direct `InvokePower2` Disruption path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisruptionPowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
}

/// Inputs accepted by retail's direct `InvokePower2` Repair path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RepairPowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
}

/// Inputs accepted by retail's direct `InvokePower2` Rage path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RagePowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
}

impl From<DisruptionPowerInvocation> for NativePowerInvocation {
    fn from(invocation: DisruptionPowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
            power_user_id: PowerUserId::INVALID,
        }
    }
}

impl From<RepairPowerInvocation> for NativePowerInvocation {
    fn from(invocation: RepairPowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
            power_user_id: PowerUserId::INVALID,
        }
    }
}

impl From<RagePowerInvocation> for NativePowerInvocation {
    fn from(invocation: RagePowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
            power_user_id: PowerUserId::INVALID,
        }
    }
}

impl From<NativePowerInvocation> for CryoPowerInvocation {
    fn from(invocation: NativePowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
        }
    }
}

impl From<NativePowerInvocation> for DisruptionPowerInvocation {
    fn from(invocation: NativePowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
        }
    }
}

impl From<NativePowerInvocation> for RepairPowerInvocation {
    fn from(invocation: NativePowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
        }
    }
}

/// Why an authoritative native power could not be started.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NativePowerError {
    #[error("player {0} does not exist")]
    PlayerNotFound(PlayerId),
    #[error("power prototype {0} does not exist")]
    PowerNotFound(ProtoPowerId),
    #[error("power '{power}' is not a {expected} power")]
    WrongPowerType {
        power: String,
        expected: &'static str,
    },
    #[error("power level {0} is not defined")]
    InvalidPowerLevel(u32),
    #[error("target location must contain only finite coordinates")]
    InvalidTarget,
    #[error("the target is not a valid power placement location")]
    InvalidPlacement,
    #[error("required power data '{0}' is missing")]
    MissingData(&'static str),
    #[error("power data '{0}' is invalid")]
    InvalidData(&'static str),
    #[error("power references unknown proto-object '{0}'")]
    UnknownPrototype(String),
    #[error("power references unknown proto-squad '{0}'")]
    UnknownSquadPrototype(String),
    #[error("power references unknown object type '{0}'")]
    UnknownObjectType(String),
    #[error("the player has no available charge for this power")]
    PowerUnavailable,
    #[error("the player cannot afford this power")]
    InsufficientResources,
    #[error("required technology '{0}' is not active")]
    MissingTechnology(String),
    #[error("the power's population requirement does not fit")]
    PopulationLimit,
    #[error("the target is inside active disruption power {0:?}")]
    Disrupted(PowerExecutionId),
    #[error("native power type '{0}' is not implemented")]
    UnsupportedPowerType(String),
}

/// Backward-compatible error name for Cryo invocation.
pub type CryoPowerError = NativePowerError;

/// Error returned by Disruption invocation.
pub type DisruptionPowerError = NativePowerError;

/// Error returned by Repair invocation.
pub type RepairPowerError = NativePowerError;

/// Error returned by Rage invocation.
pub type RagePowerError = NativePowerError;

/// Authoritative state for one running retail `BPowerCryo` execution.
#[derive(Debug, Clone)]
pub struct CryoPowerExecution {
    pub(super) id: PowerExecutionId,
    pub(super) player_id: PlayerId,
    pub(super) proto_power_id: ProtoPowerId,
    pub(super) power_level: u32,
    pub(super) owner_squad_id: EntityId,
    pub(super) bomber: common::Bomber,
    pub(super) cryo_object_id: EntityId,
    pub(super) target_location: Vec3,
    pub(super) direction: Vec3,
    pub(super) cryo_object_prototype: String,
    pub(super) filter_type: String,
    pub(super) radius: f32,
    pub(super) minimum_falloff: f32,
    pub(super) tick_duration_ms: u32,
    pub(super) ticks_remaining: u32,
    pub(super) next_tick_time_ms: u32,
    pub(super) cryo_amount_per_tick: f32,
    pub(super) killable_hitpoints_left: f32,
    pub(super) freezing_thaw_time: f32,
    pub(super) frozen_thaw_time: f32,
    pub(super) elapsed_seconds: f32,
    pub(super) bomb_released: bool,
    pub(super) ignored_squads: Vec<EntityId>,
}

impl CryoPowerExecution {
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

    /// Class-0 sim object projected as the moving bomber.
    #[must_use]
    pub const fn bomber_object_id(&self) -> EntityId {
        self.bomber.object_id()
    }

    /// Class-0 sim object created at bomb release, or invalid beforehand.
    #[must_use]
    pub const fn cryo_object_id(&self) -> EntityId {
        self.cryo_object_id
    }

    #[must_use]
    pub const fn target_location(&self) -> Vec3 {
        self.target_location
    }

    #[must_use]
    pub const fn direction(&self) -> Vec3 {
        self.direction
    }

    #[must_use]
    pub fn cryo_object_prototype(&self) -> &str {
        &self.cryo_object_prototype
    }

    #[must_use]
    pub fn bomber_prototype(&self) -> &str {
        self.bomber.prototype()
    }

    #[must_use]
    pub fn filter_type(&self) -> &str {
        &self.filter_type
    }

    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.radius
    }

    #[must_use]
    pub const fn minimum_falloff(&self) -> f32 {
        self.minimum_falloff
    }

    #[must_use]
    pub const fn tick_duration_ms(&self) -> u32 {
        self.tick_duration_ms
    }

    #[must_use]
    pub const fn ticks_remaining(&self) -> u32 {
        self.ticks_remaining
    }

    #[must_use]
    pub const fn next_tick_time_ms(&self) -> u32 {
        self.next_tick_time_ms
    }

    /// Authored and checksummed for fidelity, but retail never consumes it.
    #[must_use]
    pub const fn cryo_amount_per_tick(&self) -> f32 {
        self.cryo_amount_per_tick
    }

    #[must_use]
    pub const fn killable_hitpoints_left(&self) -> f32 {
        self.killable_hitpoints_left
    }

    #[must_use]
    pub const fn elapsed_seconds(&self) -> f32 {
        self.elapsed_seconds
    }

    /// Current authoritative position of the renderer-facing bomber.
    #[must_use]
    pub const fn bomber_position(&self) -> Vec3 {
        self.bomber.position()
    }

    #[must_use]
    pub const fn bomber_visible(&self) -> bool {
        self.bomber.visible()
    }

    #[must_use]
    pub const fn bomb_released(&self) -> bool {
        self.bomb_released
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id.0);
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_u32(self.power_level);
        checksum.hash_u32(self.owner_squad_id.as_u32());
        self.bomber.hash_state(checksum);
        checksum.hash_u32(self.cryo_object_id.as_u32());
        checksum.hash_vec3(
            self.target_location.x,
            self.target_location.y,
            self.target_location.z,
        );
        checksum.hash_vec3(self.direction.x, self.direction.y, self.direction.z);
        hash_string(checksum, &self.cryo_object_prototype);
        hash_string(checksum, &self.filter_type);
        checksum.hash_f32(self.radius);
        checksum.hash_f32(self.minimum_falloff);
        checksum.hash_u32(self.tick_duration_ms);
        checksum.hash_u32(self.ticks_remaining);
        checksum.hash_u32(self.next_tick_time_ms);
        checksum.hash_f32(self.cryo_amount_per_tick);
        checksum.hash_f32(self.killable_hitpoints_left);
        checksum.hash_f32(self.freezing_thaw_time);
        checksum.hash_f32(self.frozen_thaw_time);
        checksum.hash_f32(self.elapsed_seconds);
        checksum.hash_u32(u32::from(self.bomb_released));
        checksum.hash_u32(u32::try_from(self.ignored_squads.len()).unwrap_or(u32::MAX));
        for squad_id in &self.ignored_squads {
            checksum.hash_u32(squad_id.as_u32());
        }
    }
}

/// Authoritative state for one running retail `BPowerDisruption` execution.
#[derive(Debug, Clone)]
pub struct DisruptionPowerExecution {
    pub(super) id: PowerExecutionId,
    pub(super) player_id: PlayerId,
    pub(super) proto_power_id: ProtoPowerId,
    pub(super) power_level: u32,
    pub(super) owner_squad_id: EntityId,
    pub(super) bomber: common::Bomber,
    pub(super) disruption_object_id: EntityId,
    pub(super) target_location: Vec3,
    pub(super) direction: Vec3,
    pub(super) right: Vec3,
    pub(super) disruption_object_prototype: String,
    pub(super) pulse_object_prototype: String,
    pub(super) strike_object_prototype: String,
    pub(super) pulse_sound: String,
    pub(super) radius: f32,
    pub(super) time_remaining_seconds: f32,
    pub(super) start_time_seconds: f32,
    pub(super) next_pulse_time_seconds: f32,
    pub(super) pulse_spacing_seconds: f32,
    pub(super) pulse_count: u32,
    pub(super) elapsed_seconds: f32,
}

impl DisruptionPowerExecution {
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
    pub const fn bomber_object_id(&self) -> EntityId {
        self.bomber.object_id()
    }

    #[must_use]
    pub const fn disruption_object_id(&self) -> EntityId {
        self.disruption_object_id
    }

    #[must_use]
    pub const fn target_location(&self) -> Vec3 {
        self.target_location
    }

    #[must_use]
    pub const fn direction(&self) -> Vec3 {
        self.direction
    }

    #[must_use]
    pub fn bomber_prototype(&self) -> &str {
        self.bomber.prototype()
    }

    #[must_use]
    pub fn disruption_object_prototype(&self) -> &str {
        &self.disruption_object_prototype
    }

    #[must_use]
    pub fn pulse_object_prototype(&self) -> &str {
        &self.pulse_object_prototype
    }

    #[must_use]
    pub fn strike_object_prototype(&self) -> &str {
        &self.strike_object_prototype
    }

    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.radius
    }

    #[must_use]
    pub const fn time_remaining_seconds(&self) -> f32 {
        self.time_remaining_seconds
    }

    #[must_use]
    pub const fn start_time_seconds(&self) -> f32 {
        self.start_time_seconds
    }

    #[must_use]
    pub const fn elapsed_seconds(&self) -> f32 {
        self.elapsed_seconds
    }

    #[must_use]
    pub const fn pulse_count(&self) -> u32 {
        self.pulse_count
    }

    /// Retail disruption becomes effective independently of visual creation.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.elapsed_seconds >= self.start_time_seconds
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id.0);
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_u32(self.power_level);
        checksum.hash_u32(self.owner_squad_id.as_u32());
        self.bomber.hash_state(checksum);
        checksum.hash_u32(self.disruption_object_id.as_u32());
        checksum.hash_vec3(
            self.target_location.x,
            self.target_location.y,
            self.target_location.z,
        );
        checksum.hash_vec3(self.direction.x, self.direction.y, self.direction.z);
        checksum.hash_vec3(self.right.x, self.right.y, self.right.z);
        hash_string(checksum, &self.disruption_object_prototype);
        hash_string(checksum, &self.pulse_object_prototype);
        hash_string(checksum, &self.strike_object_prototype);
        hash_string(checksum, &self.pulse_sound);
        checksum.hash_f32(self.radius);
        checksum.hash_f32(self.time_remaining_seconds);
        checksum.hash_f32(self.start_time_seconds);
        checksum.hash_f32(self.next_pulse_time_seconds);
        checksum.hash_f32(self.pulse_spacing_seconds);
        checksum.hash_u32(self.pulse_count);
        checksum.hash_f32(self.elapsed_seconds);
    }
}

pub(super) use manager::PowerManagerState;

impl World {
    /// Running Carpet Bombing sessions and bomb fuses are authoritative sim state.
    #[must_use]
    pub fn active_carpet_bombing_powers(&self) -> &[CarpetBombingPowerExecution] {
        &self.power_manager.carpet_bombing_executions
    }

    /// Running Cleansing beams, upkeep, and damage ticks are authoritative sim state.
    #[must_use]
    pub fn active_cleansing_powers(&self) -> &[CleansingPowerExecution] {
        &self.power_manager.cleansing_executions
    }

    /// Running Cryo powers are renderer-facing projections of simulation state.
    #[must_use]
    pub fn active_cryo_powers(&self) -> &[CryoPowerExecution] {
        &self.power_manager.cryo_executions
    }

    /// Running Disruption fields are authoritative simulation state.
    #[must_use]
    pub fn active_disruption_powers(&self) -> &[DisruptionPowerExecution] {
        &self.power_manager.disruption_executions
    }

    /// Running ODST drop sessions and their hidden pending squads.
    #[must_use]
    pub fn active_odst_powers(&self) -> &[OdstPowerExecution] {
        &self.power_manager.odst_executions
    }

    /// Running Orbital targeting sessions and shot queues are authoritative sim state.
    #[must_use]
    pub fn active_orbital_powers(&self) -> &[OrbitalPowerExecution] {
        &self.power_manager.orbital_executions
    }

    /// Running Rage actions are authoritative squad and unit state.
    #[must_use]
    pub fn active_rage_powers(&self) -> &[RagePowerExecution] {
        &self.power_manager.rage_executions
    }

    /// Running Repair fields and their current target sets are authoritative sim state.
    #[must_use]
    pub fn active_repair_powers(&self) -> &[RepairPowerExecution] {
        &self.power_manager.repair_executions
    }

    /// Running Transport targeting sessions are authoritative simulation state.
    #[must_use]
    pub fn active_transport_powers(&self) -> &[TransportPowerExecution] {
        &self.power_manager.transport_executions
    }

    /// Running Wave gravity balls and captured debris are authoritative sim state.
    #[must_use]
    pub fn active_wave_powers(&self) -> &[WavePowerExecution] {
        &self.power_manager.wave_executions
    }

    /// Resolve the database power type and invoke its native sim implementation.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed profiles, failed gameplay requirements,
    /// disrupted targets, or native power types that are not implemented yet.
    pub fn invoke_native_power(
        &mut self,
        database: &Database,
        invocation: NativePowerInvocation,
    ) -> Result<PowerExecutionId, NativePowerError> {
        let power = power_by_id(database, invocation.proto_power_id)
            .ok_or(NativePowerError::PowerNotFound(invocation.proto_power_id))?;
        let power_type = power
            .attributes
            .as_ref()
            .and_then(|attributes| attributes.power_type.as_deref())
            .map(str::trim)
            .filter(|power_type| !power_type.is_empty())
            .ok_or(NativePowerError::MissingData("PowerType"))?;
        if power_type.eq_ignore_ascii_case("Cryo") {
            return self.invoke_cryo_power(database, invocation.into());
        }
        if power_type.eq_ignore_ascii_case("Cleansing") {
            return cleansing::invoke(self, database, invocation);
        }
        if power_type.eq_ignore_ascii_case("CarpetBombing") {
            return carpet_bombing::invoke(self, database, invocation);
        }
        if power_type.eq_ignore_ascii_case("Orbital") {
            return orbital::invoke(self, database, invocation);
        }
        if power_type.eq_ignore_ascii_case("Disruption") {
            return self.invoke_disruption_power(database, invocation.into());
        }
        if power_type.eq_ignore_ascii_case("Repair") {
            return self.invoke_repair_power(database, invocation.into());
        }
        if power_type.eq_ignore_ascii_case("ODST") {
            return odst::invoke(self, database, invocation);
        }
        if power_type.eq_ignore_ascii_case("Rage") {
            return rage::invoke(self, database, invocation);
        }
        if power_type.eq_ignore_ascii_case("Transport") {
            return transport::invoke(self, database, invocation);
        }
        if power_type.eq_ignore_ascii_case("Wave") {
            return wave::invoke(self, database, invocation);
        }
        Err(NativePowerError::UnsupportedPowerType(
            power_type.to_owned(),
        ))
    }

    /// Validate, pay for, and start one authoritative Cryo power.
    ///
    /// # Errors
    ///
    /// Returns an error when the profile is malformed or the player does not
    /// meet the authored ownership, cost, technology, or population rules.
    pub fn invoke_cryo_power(
        &mut self,
        database: &Database,
        invocation: CryoPowerInvocation,
    ) -> Result<PowerExecutionId, CryoPowerError> {
        cryo::invoke(self, database, invocation)
    }

    /// Validate, pay for, and start one authoritative Disruption power.
    ///
    /// # Errors
    ///
    /// Returns an error when the profile is malformed or the player does not
    /// meet the authored ownership, cost, technology, or population rules.
    pub fn invoke_disruption_power(
        &mut self,
        database: &Database,
        invocation: DisruptionPowerInvocation,
    ) -> Result<PowerExecutionId, DisruptionPowerError> {
        disruption::invoke(self, database, invocation)
    }

    /// Validate, pay for, and start one authoritative Repair power.
    ///
    /// # Errors
    ///
    /// Returns an error when placement, disruption, profile data, or gameplay
    /// requirements reject the invocation.
    pub fn invoke_repair_power(
        &mut self,
        database: &Database,
        invocation: RepairPowerInvocation,
    ) -> Result<PowerExecutionId, RepairPowerError> {
        repair::invoke(self, database, invocation)
    }

    /// Validate, pay for, and start one authoritative Rage power.
    ///
    /// # Errors
    ///
    /// Returns an error when its owner, profile, placement, disruption, or
    /// gameplay requirements reject the invocation.
    pub fn invoke_rage_power(
        &mut self,
        database: &Database,
        invocation: RagePowerInvocation,
    ) -> Result<PowerExecutionId, RagePowerError> {
        rage::invoke(self, database, invocation.into())
    }

    /// Submit direct input using a stable simulation execution ID.
    pub fn submit_rage_power_input(
        &mut self,
        database: &Database,
        execution_id: PowerExecutionId,
        input: NativePowerInput,
    ) -> bool {
        rage::submit_input_by_execution(self, database, execution_id, input, false)
    }

    /// Route one wire power input to its packed retail user ID.
    pub fn submit_native_power_input(
        &mut self,
        database: &Database,
        power_user_id: PowerUserId,
        input: NativePowerInput,
        no_cost: bool,
    ) -> bool {
        match power_user_id.power_type() {
            1 => cleansing::submit_input_by_user(self, database, power_user_id, input, no_cost),
            2 => orbital::submit_input_by_user(self, database, power_user_id, input, no_cost),
            3 => {
                carpet_bombing::submit_input_by_user(self, database, power_user_id, input, no_cost)
            }
            5 => rage::submit_input_by_user(self, database, power_user_id, input, no_cost),
            6 => wave::submit_input_by_user(self, database, power_user_id, input, no_cost),
            8 => transport::submit_input_by_user(self, database, power_user_id, input, no_cost),
            9 => odst::submit_input_by_user(self, database, power_user_id, input, no_cost),
            _ => false,
        }
    }

    pub(in crate::world) fn notify_power_projectile_impact(
        &mut self,
        database: Option<&Database>,
        execution_id: u32,
        projectile_id: EntityId,
        position: Vec3,
        direction: Vec3,
        damaged_unit_ids: &[EntityId],
    ) {
        carpet_bombing::notify_projectile_impact(
            self,
            execution_id,
            projectile_id,
            position,
            damaged_unit_ids,
        );
        orbital::notify_projectile_impact(
            self,
            database,
            execution_id,
            projectile_id,
            position,
            direction,
        );
    }

    pub(super) fn update_active_powers(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: Option<&GameplayCatalog>,
    ) {
        cleansing::update(self, dt, database, gameplay);
        orbital::update(self, dt, database, gameplay);
        carpet_bombing::update(self, dt, database, gameplay);
        cryo::update(self, dt, database);
        disruption::update(self, dt, database);
        odst::update(self, dt, database, gameplay);
        rage::update(self, dt, database, gameplay);
        repair::update(self, database);
        wave::update(self, dt, database, gameplay);
    }

    pub(super) fn resolve_pending_rage_kills(&mut self, database: Option<&Database>) {
        rage::resolve_pending_kills(self, database);
    }

    pub(crate) fn queue_rage_kill(&mut self, attacker_id: EntityId, target_prototype: String) {
        self.power_manager
            .pending_rage_kills
            .push(rage::PendingRageKill {
                attacker_id,
                target_prototype,
            });
    }

    pub(super) fn update_power_visual_lifetimes(&mut self) {
        common::expire_transient_visuals(self);
    }

    pub(crate) fn update_player_power_recharges(&mut self, database: &Database) {
        let game_time_ms = self.game_time_ms;
        for player in &mut self.players {
            player.update_power_recharges(game_time_ms, |proto_power_id| {
                power_by_id(database, proto_power_id)
                    .and_then(|power| power.attributes.as_ref())
                    .map(|attributes| {
                        (
                            attributes.auto_recharge.unwrap_or_default(),
                            attributes.use_limit.unwrap_or_default(),
                        )
                    })
                    .unwrap_or_default()
            });
        }
    }
}

/// Resolve a scenario-layered power name to its retail runtime table index.
#[must_use]
pub fn power_prototype_id(database: &Database, name: &str) -> Option<ProtoPowerId> {
    name.trim()
        .parse()
        .ok()
        .filter(|id| power_by_id(database, *id).is_some())
        .or_else(|| {
            database
                .powers
                .iter()
                .position(|power| power.name.trim().eq_ignore_ascii_case(name.trim()))
                .and_then(|index| i32::try_from(index).ok())
        })
}

impl World {
    pub(crate) fn grant_player_power(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        mut grant: PowerGrant,
    ) -> bool {
        let Some(power) = power_by_id(database, grant.proto_power_id) else {
            return false;
        };
        if !grant.squad_id.is_invalid() && self.get_squad(grant.squad_id).is_none() {
            grant.squad_id = EntityId::INVALID;
        }
        let rules = rules(power);
        let Some(player) = self.get_player_mut(player_id) else {
            return false;
        };
        player.grant_power(grant, rules, |power_id, icon_location| {
            power_by_id(database, power_id)
                .and_then(|power| power.attributes.as_ref())
                .is_some_and(|attributes| attributes.icon_locations.contains(&icon_location))
        });
        true
    }

    pub(crate) fn revoke_player_power(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        proto_power_id: ProtoPowerId,
        mut squad_id: EntityId,
    ) -> bool {
        let Some(power) = power_by_id(database, proto_power_id) else {
            return false;
        };
        if !squad_id.is_invalid() && self.get_squad(squad_id).is_none() {
            squad_id = EntityId::INVALID;
        }
        let Some(player) = self.get_player_mut(player_id) else {
            return false;
        };
        player.revoke_power(proto_power_id, squad_id, rules(power));
        true
    }
}

pub(super) fn power_by_id(database: &Database, proto_power_id: ProtoPowerId) -> Option<&Power> {
    usize::try_from(proto_power_id)
        .ok()
        .and_then(|index| database.powers.get(index))
}

pub(super) fn rules(power: &Power) -> PowerRules {
    let attributes = power.attributes.as_ref();
    PowerRules {
        infinite_uses: attributes
            .and_then(|value| value.infinite_uses)
            .unwrap_or(false),
        multi_recharge: attributes
            .and_then(|value| value.multi_recharge_power)
            .unwrap_or(false),
        sequential_recharge: attributes
            .and_then(|value| value.sequential_recharge)
            .unwrap_or(false),
    }
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}

#[cfg(test)]
mod tests;
