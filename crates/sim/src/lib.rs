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
pub(crate) mod math;
pub mod order;
pub mod packet;
pub mod physics;
pub mod player;
pub(crate) mod prototypes;
pub mod random;
pub mod scenario;
pub mod serialize;
pub mod session;
pub mod simulation;
pub mod spawn;
pub mod sync;
pub mod time_sync;
pub mod trigger;
mod work_action;
pub mod world;

pub use command::{Command, CommandType, EntityType};
pub use command_queue::{CommandEntry, CommandQueue};
pub use commands::{
    BuildingCommand, BuildingCommandType, CommandPayload, GameCommand, GameCommandType,
    PowerCommand, PowerCommandType, PowerInputCommand, PowerInputCommandType, PowerUserId,
    UserWorkTarget, WorkCommand,
};
pub use dispatcher::{CommandDispatcher, DispatchError};
pub use entities::{
    AIR_TRAFFIC_LANDING_SPOT_COUNT, AirTrafficControl, AirTrafficLandingSpot, AircraftCrashPhase,
    AmbientLifeBehavior, Base, BaseEntity, BaseId, BombPhase, BuildingProduction, CapturePhase,
    ConstructionKind, ConstructionProgress, ConstructionTask, DopplePolicy, EnergyShieldPhase,
    EnergyShieldPresentationKind, FlightControllerKind, GatherPhase, GroundMovePhase, HealPhase,
    IconObject, InfectionExposure, InfectionPhase, Object, ObjectKind, ObjectState,
    PowerTransportPhase, Projectile, RallyPoint, RecoveryType, RepairOtherPhase, ResearchProgress,
    ResearchTask, Revealer, ScriptedAnimation, ShieldCoverage, Squad, SquadArchetype,
    SquadBoardState, SquadCarpetBombPhase, SquadChangeModePhase, SquadContainmentState,
    SquadCryoState, SquadDetonatePhase, SquadFormation, SquadGarrison, SquadMergeState, SquadMode,
    SquadPowerTransport, SquadPullPhase, SquadRecovery, SquadShields, SquadState,
    SquadTrainedAirBirth, SquadTransportFlyIn, TargetingSelection, TowerWallAction,
    TrainedSquadBirth, TrainingKind, TrainingProgress, TrainingRecharge, TrainingTask,
    TransportFlyInPhase, TurnInPlaceDirection, TurretAttackKind, TurretAttackState, Unit,
    UnitActions, UnitAmmunition, UnitArchetype, UnitChangeModePhase, UnitDataScalar,
    UnitDetonatePhase, UnitEnergyShieldAction, UnitGarrison, UnitKind, UnitShields, UnitState,
    UnitTurretCombat, UnitVisualMeshMask,
};
pub use entity::{Entity, EntityManager, MAX_ENTITY_SLOTS};
pub use entity_id::{EntityClass, EntityId};
pub use executor::CommandExecutor;
pub use gameplay::{
    AbilityGameplay, AbilityRecoveryStart, AirAvoidanceActionProfile,
    AirTrafficControlActionProfile, AmbientLifeSpawnerProfile, AreaDamageProfile,
    AttackAccuracyProfile, AttackAnimation, AttackAnimationAnchor, AttackAnimationEvent,
    AttackAnimationEventKind, AttackAttachmentPose, AttackControlProfile, AttackHardpointProfile,
    AttackProfile, AttackQuery, AttackQueryFlags, AutoRepairProfile, BombActionProfile,
    CaptureActionProfile, ChargeActionProfile, ChargeEffectProfile, ChargedAttackAnimation,
    CollisionAttackProfile, DamagePartProfile, DetonateActionProfile, DetonateDurationProfile,
    DetonateThrowProfile, EnergyShieldActionProfile, EnergyShieldVisualProfile, GameplayCatalog,
    GameplayLoadIssue, GatherActionProfile, HealActionProfile, HeroRevivalProfile,
    ImpactEffectProfile, ImpactEffectSize, InfectActionProfile, JoinActionProfile, JoinKind,
    JoinMergeType, KamikazeWeaponProfile, MergedSquadProfile, MineActionProfile, ObjectGameplay,
    PhysicsImpulseEvent, PhysicsLoadIssue, ProjectileInitialPerturbance,
    ProjectilePerturbanceProfile, ProjectileProfile, PullAttackProfile, RangedAction,
    RepairOtherActionProfile, ReviveActionProfile, TacticRelation, TacticStateId,
    TacticStateProfile, ThrownDamagePart, UnitRevivalProfile,
};
pub use order::{JumpOrderRequest, JumpOrderType, OrderType};
pub use packet::{ChannelPacketHeader, PacketError};
pub use physics::{
    ObjectPhysicsTemplates, PhysicsBlueprintBody, PhysicsTemplate, SimPhysics, UnitPhysics,
};
pub use player::{
    CivId, DEFAULT_PLAYER_DIFFICULTY, GAIA_PLAYER, LeaderId, MAX_POP_TYPES, MAX_RESOURCES,
    MAX_TEAMS, Player, PlayerId, PlayerResearchState, PlayerState, PlayerTechState, PlayerType,
    Population, PopulationCost, PowerEntry, PowerEntryItem, ProtoPowerId, Resources, TeamId,
    TeamRelation, TechStatus,
};
pub use random::Random;
pub use scenario::{
    GameSettings, LoadedGameScenario, LoadedScenario, PlayerSettings, SKIRMISH_GAME_MODE,
    ScenarioAssetLoadError, ScenarioData, ScenarioObject, ScenarioPlayer, ScenarioPosition,
    ScenarioSetupError, SlotType, load_scenario_from_game_dir, load_scenario_into_world,
};
pub use serialize::{SerializeError, deserialize_command, serialize_command};
pub use session::{ClientState, PlayerInfo, Session, SessionState};
pub use simulation::{MAX_CATCH_UP_MS, MS_PER_TICK, ScenarioTick, SimState, Simulation, TICK_RATE};
pub use spawn::{
    MAX_SPAWN_BATCH, SpawnError, object_prototype_id, spawn_object_at, spawn_squad_at,
    spawn_squad_from_base, spawn_squad_from_base_by_name, spawn_squads_at, squad_prototype_id,
    squad_prototype_name,
};
pub use sync::{SimpleChecksum, SyncChecksum};
pub use time_sync::{ClientTimeHistory, TimeSync, TimingRecord};
pub use trigger::{
    AISquadAnalysis, AISquadAnalysisComponent, BuildingCommandState, Condition, ConditionMode,
    ConditionResult, ConditionType, Effect, EffectType, ObjectiveId, Trigger, TriggerColor,
    TriggerCost, TriggerEngine, TriggerId, TriggerScript, TriggerScriptId, TriggerUpdate,
    TriggerValue, TriggerVec3, VarId, VarType,
};
pub use work_action::{DefaultWorkAction, default_work_action};
pub use world::{
    CameraControlPermissions, CameraDirective, CameraShake, CarpetBomb, CarpetBombingPhase,
    CarpetBombingPowerError, CarpetBombingPowerExecution, CarpetBombingPowerInvocation,
    ChatRequest, CinematicRequest, CleansingPowerError, CleansingPowerExecution,
    CleansingPowerInvocation, ConstructionError, ConstructionQueueResult, CryoPowerError,
    CryoPowerExecution, CryoPowerInvocation, CustomCommand, CustomCommandFlags, DesignLineId,
    DesignSphere, DisruptionPowerError, DisruptionPowerExecution, DisruptionPowerInvocation,
    GameResult, GameTimer, GameTimerAudience, GameType, GarrisonError, GeneralEvent,
    GeneralEventType, HintCallout, HintCalloutAnchor, HudItem, ImpactEffectRequest, ImpactSurface,
    MAX_PLAYERS, MAX_TRAIN_BATCH, NativePowerError, NativePowerInput, NativePowerInvocation,
    ObjectCostError, ObjectivePointer, ObjectiveState, OdstDrop, OdstPowerError,
    OdstPowerExecution, OdstPowerInvocation, OrbitalPowerError, OrbitalPowerExecution,
    OrbitalPowerInvocation, OrbitalShot, PlayerPresentationState, PowerExecutionId,
    PresentationRequest, ProductionUpdate, RagePowerError, RagePowerExecution, RagePowerInvocation,
    RagePowerPhase, RepairPowerError, RepairPowerExecution, RepairPowerInvocation, ResearchError,
    ResearchQueueResult, ResearchQuote, RumbleMotor, RumbleRequest, ScenarioScoreInfo,
    ScreenFadeOverlay, ScreenFadeSequence, TechnologyError, TrainingError, TrainingQueueResult,
    TrainingQuote, TransportPowerError, TransportPowerExecution, TransportPowerInvocation,
    WaveCapturedObject, WaveFakeObject, WaveGravityBallState, WavePowerError, WavePowerExecution,
    WavePowerInvocation, World, power_prototype_id, technology_prototype_id,
};
