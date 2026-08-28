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
pub mod squads;
pub mod units;

pub use base::BaseEntity;
pub use base_site::{Base, BaseId};
pub use squads::{Squad, SquadArchetype, SquadFormation, SquadState};
pub use units::{Unit, UnitArchetype, UnitKind, UnitState};
