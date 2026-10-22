//! Rejections that construction validation returns.

use crate::entities::ConstructionKind;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use crate::world::ObjectCostError;

/// Rejection produced while validating authoritative construction.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConstructionError {
    /// The world has no player with this ID.
    #[error("player {0} is not present in the world")]
    PlayerNotFound(PlayerId),
    /// The world has no unit with this builder ID.
    #[error("builder {0:?} is not present in the world")]
    BuilderNotFound(EntityId),
    /// The builder belongs to a player that the command does not reach: neither
    /// the command player, Gaia nor a human co-op teammate.
    #[error("builder {builder_id:?} does not accept commands from player {player_id}")]
    BuilderNotOwned {
        /// The builder that the command names.
        builder_id: EntityId,
        /// The player that issued the command.
        player_id: PlayerId,
    },
    /// The builder is itself still under construction.
    #[error("builder {0:?} has not completed construction")]
    BuilderUnbuilt(EntityId),
    /// The database has no prototype with this runtime ID.
    #[error("runtime building prototype ID {0} was not found")]
    PrototypeNotFound(i32),
    /// The named prototype is not a building.
    #[error("construction target '{0}' is not a building")]
    PrototypeNotBuilding(String),
    /// The builder has no command of this kind for the prototype.
    #[error("builder {builder_id:?} does not offer {kind:?} '{prototype}'")]
    CommandUnavailable {
        /// The builder that the command names.
        builder_id: EntityId,
        /// The construction command kind (`Build` or `BuildOther`).
        kind: ConstructionKind,
        /// The name of the requested prototype.
        prototype: String,
    },
    /// The object cost lookup failed.
    #[error(transparent)]
    ObjectCost(#[from] ObjectCostError),
    /// The named prototype has build points that are not usable.
    #[error("construction '{0}' has invalid build points")]
    InvalidBuildPoints(String),
    /// The named prototype uses population, but the database has no population table.
    #[error("construction '{0}' has population but the database has no population table")]
    MissingPopulationTable(String),
    /// The prototype names a population type that the database does not define.
    #[error("'{prototype}' references unknown population type '{population}'")]
    UnknownPopulation {
        /// The name of the prototype.
        prototype: String,
        /// The unknown population type name.
        population: String,
    },
    /// The player does not have the resources to pay for the prototype.
    #[error("player {player_id} cannot afford '{prototype}'")]
    InsufficientResources {
        /// The player that must pay.
        player_id: PlayerId,
        /// The name of the requested prototype.
        prototype: String,
    },
    /// The player does not have the population room for the prototype.
    #[error("player {player_id} has no population room for '{prototype}'")]
    PopulationFull {
        /// The player that must supply the population.
        player_id: PlayerId,
        /// The name of the requested prototype.
        prototype: String,
    },
    /// The builder already has the maximum count of this prototype.
    #[error("the train limit for '{prototype}' on builder {builder_id:?} is reached")]
    TrainLimitReached {
        /// The builder that owns the train limit.
        builder_id: EntityId,
        /// The name of the requested prototype.
        prototype: String,
    },
    /// The builder already has `BuildOther` work in its queue or in progress.
    #[error("builder {0:?} already has queued or active BuildOther work")]
    BuildOtherAlreadyQueued(EntityId),
    /// The builder has no free socket for the prototype.
    #[error("builder {builder_id:?} has no available socket for '{prototype}'")]
    SocketUnavailable {
        /// The builder that owns the sockets.
        builder_id: EntityId,
        /// The name of the requested prototype.
        prototype: String,
    },
    /// The construction position or facing is not finite.
    #[error("construction position and facing must be finite")]
    InvalidTransform,
    /// The new building could not start its construction worker.
    #[error("new building {0:?} could not start its construction worker")]
    WorkerUnavailable(EntityId),
}
