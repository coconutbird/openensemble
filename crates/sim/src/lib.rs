//! Deterministic simulation crate for Halo Wars network compatibility.
//!
//! This crate implements the core simulation logic that must match vanilla
//! Halo Wars exactly for network compatibility. The wire format for commands
//! must be byte-for-byte identical.

pub mod command;
pub mod command_queue;
pub mod commands;
pub mod dispatcher;
pub mod entities;
pub mod entity;
pub mod entity_id;
pub mod executor;
pub mod order;
pub mod packet;
pub mod player;
pub mod random;
pub mod scenario;
pub mod serialize;
pub mod session;
pub mod simulation;
pub mod sync;
pub mod time_sync;
pub mod trigger;
pub mod world;

pub use command::{Command, CommandType, EntityType};
pub use command_queue::{CommandEntry, CommandQueue, QueuedCommand};
pub use commands::{GameCommand, PowerCommand, WorkCommand};
pub use dispatcher::{CommandDispatcher, DispatchError, DispatchedCommand};
pub use entities::{BaseEntity, Squad, SquadState};
pub use entity::{Entity, EntityManager};
pub use entity_id::{EntityClass, EntityId};
pub use executor::CommandExecutor;
pub use order::OrderType;
pub use packet::{ChannelPacketHeader, PacketError};
pub use player::{
    CivId, GAIA_PLAYER, LeaderId, MAX_POP_TYPES, MAX_RESOURCES, Player, PlayerId, PlayerState,
    PlayerType, Population, Resources, TeamId,
};
pub use random::Random;
pub use scenario::{
    LoadedScenario, ScenarioData, ScenarioObject, ScenarioPlayer, ScenarioPosition,
    load_scenario_into_world,
};
pub use serialize::{SerializeError, deserialize_command, serialize_command};
pub use session::{ClientState, PlayerInfo, Session, SessionState};
pub use simulation::{
    CommandHandler, MS_PER_TICK, SimState, SimUpdateResult, Simulation, TICK_RATE,
};
pub use sync::{SimpleChecksum, SyncChecksum};
pub use time_sync::{ClientTimeHistory, TimeSync, TimingRecord};
pub use trigger::{
    Condition, ConditionMode, ConditionType, Effect, EffectType, Trigger, TriggerEngine, TriggerId,
    TriggerScript, TriggerScriptId, TriggerValue, VarId, VarType,
};
pub use world::{MAX_PLAYERS, World};
