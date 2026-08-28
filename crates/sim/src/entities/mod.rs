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
pub mod projectiles;
pub mod squads;
pub mod units;

pub use base::BaseEntity;
pub use base_site::{Base, BaseId};
pub(crate) use idle::EntityIdle;
pub use projectiles::Projectile;
pub use squads::{
    RecoveryType, Squad, SquadArchetype, SquadContainmentState, SquadFormation, SquadGarrison,
    SquadMode, SquadRecovery, SquadShields, SquadState,
};
pub use units::{
    BuildingProduction, ConstructionKind, ConstructionProgress, ConstructionTask, ResearchProgress,
    ResearchTask, ShieldCoverage, TrainingKind, TrainingProgress, TrainingTask, Unit, UnitActions,
    UnitArchetype, UnitDataScalar, UnitGarrison, UnitKind, UnitShields, UnitState,
};
