//! Native power invocation inputs, live inputs and invocation errors.

use super::PowerExecutionId;
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::player::{PlayerId, ProtoPowerId};
use glam::Vec3;
use thiserror::Error;

/// Inputs accepted by retail's direct `InvokePower2` native-power path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativePowerInvocation {
    /// The player that uses the power.
    pub player_id: PlayerId,
    /// The runtime table index of the power prototype.
    pub proto_power_id: ProtoPowerId,
    /// The authored power level that selects the power data.
    pub power_level: u32,
    /// The squad that owns the power charge, or an invalid ID for a player power.
    pub squad_id: EntityId,
    /// The world location that the power targets.
    pub target_location: Vec3,
    /// If `true`, skip the requirement checks and the payment.
    pub ignore_requirements: bool,
    /// The packed retail power user ID, or `PowerUserId::INVALID` if there is none.
    pub power_user_id: PowerUserId,
}

/// Live input accepted by an already-running native power.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NativePowerInput {
    /// Confirm an action at a world location.
    Confirm(Vec3),
    /// Move the controlled power position to a world location.
    Position(Vec3),
    /// Set the power direction.
    Direction(Vec3),
    /// Stop the power.
    Shutdown,
}

/// Inputs accepted by retail's direct `InvokePower2` Cryo path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CryoPowerInvocation {
    /// The player that uses the power.
    pub player_id: PlayerId,
    /// The runtime table index of the power prototype.
    pub proto_power_id: ProtoPowerId,
    /// The authored power level that selects the power data.
    pub power_level: u32,
    /// The squad that owns the power charge, or an invalid ID for a player power.
    pub squad_id: EntityId,
    /// The world location that the power targets.
    pub target_location: Vec3,
    /// If `true`, skip the requirement checks and the payment.
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
    /// The player that uses the power.
    pub player_id: PlayerId,
    /// The runtime table index of the power prototype.
    pub proto_power_id: ProtoPowerId,
    /// The authored power level that selects the power data.
    pub power_level: u32,
    /// The squad that owns the power charge, or an invalid ID for a player power.
    pub squad_id: EntityId,
    /// The world location that the power targets.
    pub target_location: Vec3,
    /// If `true`, skip the requirement checks and the payment.
    pub ignore_requirements: bool,
}

/// Inputs accepted by retail's direct `InvokePower2` Repair path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RepairPowerInvocation {
    /// The player that uses the power.
    pub player_id: PlayerId,
    /// The runtime table index of the power prototype.
    pub proto_power_id: ProtoPowerId,
    /// The authored power level that selects the power data.
    pub power_level: u32,
    /// The squad that owns the power charge, or an invalid ID for a player power.
    pub squad_id: EntityId,
    /// The world location that the power targets.
    pub target_location: Vec3,
    /// If `true`, skip the requirement checks and the payment.
    pub ignore_requirements: bool,
}

/// Inputs accepted by retail's direct `InvokePower2` Rage path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RagePowerInvocation {
    /// The player that uses the power.
    pub player_id: PlayerId,
    /// The runtime table index of the power prototype.
    pub proto_power_id: ProtoPowerId,
    /// The authored power level that selects the power data.
    pub power_level: u32,
    /// The squad that owns the power charge, or an invalid ID for a player power.
    pub squad_id: EntityId,
    /// The world location that the power targets.
    pub target_location: Vec3,
    /// If `true`, skip the requirement checks and the payment.
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
    /// The player does not exist.
    #[error("player {0} does not exist")]
    PlayerNotFound(PlayerId),
    /// The power prototype does not exist.
    #[error("power prototype {0} does not exist")]
    PowerNotFound(ProtoPowerId),
    /// The `PowerType` of the power is not the expected type.
    #[error("power '{power}' is not a {expected} power")]
    WrongPowerType {
        /// The name of the power.
        power: String,
        /// The expected `PowerType` value.
        expected: &'static str,
    },
    /// The power data does not define the power level.
    #[error("power level {0} is not defined")]
    InvalidPowerLevel(u32),
    /// The target location has a coordinate that is not finite.
    #[error("target location must contain only finite coordinates")]
    InvalidTarget,
    /// The power cannot be placed at the target.
    #[error("the target is not a valid power placement location")]
    InvalidPlacement,
    /// The power data does not have a required value.
    #[error("required power data '{0}' is missing")]
    MissingData(&'static str),
    /// A value in the power data is not valid.
    #[error("power data '{0}' is invalid")]
    InvalidData(&'static str),
    /// The power data names a proto-object that does not exist.
    #[error("power references unknown proto-object '{0}'")]
    UnknownPrototype(String),
    /// The power data names a proto-squad that does not exist.
    #[error("power references unknown proto-squad '{0}'")]
    UnknownSquadPrototype(String),
    /// The power data names an object type that does not exist.
    #[error("power references unknown object type '{0}'")]
    UnknownObjectType(String),
    /// The player has no charge of the power.
    #[error("the player has no available charge for this power")]
    PowerUnavailable,
    /// The player does not have the resources to pay for the power.
    #[error("the player cannot afford this power")]
    InsufficientResources,
    /// A technology that the power requires is not active.
    #[error("required technology '{0}' is not active")]
    MissingTechnology(String),
    /// The population cost of the power does not fit in the population limit.
    #[error("the power's population requirement does not fit")]
    PopulationLimit,
    /// An active Disruption power covers the target.
    #[error("the target is inside active disruption power {0:?}")]
    Disrupted(PowerExecutionId),
    /// The simulation does not implement this `PowerType`.
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
