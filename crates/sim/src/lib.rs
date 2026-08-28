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
pub mod gameplay;
pub mod order;
pub mod packet;
pub mod physics;
pub mod player;
pub mod random;
pub mod scenario;
pub mod serialize;
pub mod session;
pub mod simulation;
pub mod spawn;
pub mod sync;
pub mod time_sync;
pub mod trigger;
pub mod world;

pub use command::{Command, CommandType, EntityType};
pub use command_queue::{CommandEntry, CommandQueue, QueuedCommand};
pub use commands::{
    BuildingCommand, BuildingCommandType, GameCommand, GameCommandType, PowerCommand, WorkCommand,
};
pub use dispatcher::{CommandDispatcher, DispatchError, DispatchedCommand};
pub use entities::{
    Base, BaseEntity, BaseId, BuildingProduction, Projectile, RecoveryType, ResearchProgress,
    ResearchTask, ShieldCoverage, Squad, SquadArchetype, SquadFormation, SquadMode, SquadRecovery,
    SquadShields, SquadState, Unit, UnitActions, UnitArchetype, UnitKind, UnitShields, UnitState,
};
pub use entity::{Entity, EntityManager, MAX_ENTITY_SLOTS};
pub use entity_id::{EntityClass, EntityId};
pub use executor::CommandExecutor;
pub use gameplay::{
    AbilityGameplay, AbilityRecoveryStart, AttackQuery, AttackQueryFlags, GameplayCatalog,
    GameplayLoadIssue, ObjectGameplay, RangedAction, TacticRelation,
};
pub use order::OrderType;
pub use packet::{ChannelPacketHeader, PacketError};
pub use physics::{BoxCollider, MotionType, PhysicsBody, PhysicsMaterial};
pub use player::{
    CivId, GAIA_PLAYER, LeaderId, MAX_POP_TYPES, MAX_RESOURCES, MAX_TEAMS, Player, PlayerId,
    PlayerResearchState, PlayerState, PlayerTechState, PlayerType, Population, Resources, TeamId,
    TeamRelation, TechStatus,
};
pub use random::Random;
pub use scenario::{
    LoadedGameScenario, LoadedScenario, ScenarioAssetLoadError, ScenarioData, ScenarioObject,
    ScenarioPlayer, ScenarioPosition, ScenarioPositionAxes, load_scenario_from_game_dir,
    load_scenario_into_world, scenario_object_direction_to_world,
    scenario_object_position_to_world,
};
pub use serialize::{SerializeError, deserialize_command, serialize_command};
pub use session::{ClientState, PlayerInfo, Session, SessionState};
pub use simulation::{
    CommandHandler, MS_PER_TICK, SimState, SimUpdateResult, Simulation, TICK_RATE,
};
pub use spawn::{
    MAX_SPAWN_BATCH, SpawnError, object_prototype_id, spawn_object_at, spawn_squad_at,
    spawn_squad_from_base, spawn_squad_from_base_by_name, spawn_squads_at, squad_prototype_id,
};
pub use sync::{SimpleChecksum, SyncChecksum};
pub use time_sync::{ClientTimeHistory, TimeSync, TimingRecord};
pub use trigger::{
    Condition, ConditionMode, ConditionResult, ConditionType, Effect, EffectType, Trigger,
    TriggerEngine, TriggerId, TriggerScript, TriggerScriptId, TriggerValue, VarId, VarType,
};
pub use world::{
    MAX_PLAYERS, ResearchError, ResearchQueueResult, TechnologyError, World,
    technology_prototype_id,
};
