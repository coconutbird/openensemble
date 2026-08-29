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
    BuildingCommand, BuildingCommandType, GameCommand, GameCommandType, PowerCommand,
    PowerCommandType, WorkCommand,
};
pub use dispatcher::{CommandDispatcher, DispatchError, DispatchedCommand};
pub use entities::{
    Base, BaseEntity, BaseId, BuildingProduction, ConstructionKind, ConstructionProgress,
    ConstructionTask, DopplePolicy, Object, ObjectKind, ObjectState, Projectile, RallyPoint,
    RecoveryType, ResearchProgress, ResearchTask, Revealer, ShieldCoverage, Squad, SquadArchetype,
    SquadContainmentState, SquadFormation, SquadGarrison, SquadMode, SquadRecovery, SquadShields,
    SquadState, SquadTransportFlyIn, TargetingSelection, TowerWallAction, TrainingKind,
    TrainingProgress, TrainingTask, TransportFlyInPhase, Unit, UnitActions, UnitArchetype,
    UnitDataScalar, UnitGarrison, UnitKind, UnitShields, UnitState,
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
    CivId, DEFAULT_PLAYER_DIFFICULTY, GAIA_PLAYER, LeaderId, MAX_POP_TYPES, MAX_RESOURCES,
    MAX_TEAMS, Player, PlayerId, PlayerResearchState, PlayerState, PlayerTechState, PlayerType,
    Population, PopulationCost, PowerEntry, PowerEntryItem, ProtoPowerId, Resources, TeamId,
    TeamRelation, TechStatus,
};
pub use random::Random;
pub use scenario::{
    LoadedGameScenario, LoadedScenario, ScenarioAssetLoadError, ScenarioData, ScenarioObject,
    ScenarioPlayer, ScenarioPosition, ScenarioPositionAxes, configure_player_leader,
    load_scenario_from_game_dir, load_scenario_into_world, scenario_object_direction_to_world,
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
    AISquadAnalysis, AISquadAnalysisComponent, BuildingCommandState, Condition, ConditionMode,
    ConditionResult, ConditionType, Effect, EffectType, ObjectiveId, Trigger, TriggerColor,
    TriggerCost, TriggerEngine, TriggerId, TriggerScript, TriggerScriptId, TriggerUpdate,
    TriggerValue, TriggerVec3, VarId, VarType,
};
pub use world::{
    CameraControlPermissions, CameraDirective, ChatRequest, CinematicRequest, ConstructionError,
    ConstructionQueueResult, CustomCommand, CustomCommandFlags, DesignLineId, GameTimer,
    GameTimerAudience, GarrisonError, GeneralEvent, GeneralEventType, HintCallout,
    HintCalloutAnchor, HudItem, MAX_PLAYERS, MAX_TRAIN_BATCH, ObjectiveState,
    PlayerPresentationState, PresentationRequest, ProductionUpdate, ResearchError,
    ResearchQueueResult, ScenarioScoreInfo, ScreenFadeOverlay, ScreenFadeSequence, TechnologyError,
    TrainingError, TrainingQueueResult, World, object_runtime_id, power_prototype_id,
    squad_runtime_id, technology_prototype_id,
};
