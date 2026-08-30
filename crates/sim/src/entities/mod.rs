//! Entity types for the simulation.
//!
//! Hierarchy based on original source:
//! - `BEntity` (base) - position, velocity, player, actions
//!   - `BObject` - visual and physics state
//!     - `BUnit` - individual unit
//!     - `BDopple` - fog of war ghost
//!     - `BProjectile` - bullets, missiles
//!   - `BSquad` - group of units (primary controllable entity)
//!   - `BPlatoon` - group of squads
//!   - `BArmy` - player's forces
//!
//! The MVP implements mobile units, buildings, squads, and base ownership.

mod base;
mod base_site;
mod idle;
mod object_state;
pub mod objects;
pub mod projectiles;
pub mod squads;
pub mod units;

pub use base::BaseEntity;
pub(crate) use base_site::BasePlasmaShield;
pub use base_site::{Base, BaseId};
pub(crate) use idle::EntityIdle;
pub use object_state::{DopplePolicy, ObjectState, ScriptedAnimation, TargetingSelection};
pub use objects::{IconObject, Object, ObjectKind, Revealer};
pub use projectiles::Projectile;
pub use squads::{
    AmbientLifeBehavior, JoinKind, JoinMergeType, PowerTransportPhase, RecoveryType,
    RepairOtherPhase, Squad, SquadArchetype, SquadBoardState, SquadCarpetBombPhase,
    SquadContainmentState, SquadCryoState, SquadDetonatePhase, SquadFormation, SquadGarrison,
    SquadMergeState, SquadMode, SquadPowerTransport, SquadPullPhase, SquadRecovery, SquadShields,
    SquadState, SquadTrainedAirBirth, SquadTransportFlyIn, TransportFlyInPhase,
};
pub use units::{
    AIR_TRAFFIC_LANDING_SPOT_COUNT, AirTrafficControl, AirTrafficLandingSpot, AircraftCrashPhase,
    BombPhase, BuildingProduction, CapturePhase, ConstructionKind, ConstructionProgress,
    ConstructionTask, EnergyShieldPhase, EnergyShieldPresentationKind, FlightControllerKind,
    GatherPhase, GroundMovePhase, HealPhase, InfectionExposure, InfectionPhase, RallyPoint,
    ResearchProgress, ResearchTask, ShieldCoverage, TowerWallAction, TrainedSquadBirth,
    TrainingKind, TrainingProgress, TrainingRecharge, TrainingTask, Unit, UnitActions,
    UnitAmmunition, UnitArchetype, UnitDataScalar, UnitDetonatePhase, UnitEnergyShieldAction,
    UnitGarrison, UnitKind, UnitShields, UnitState, UnitVisualMeshMask,
};
pub(crate) use units::{AttackAdvance, UnitScalarModifiers};
