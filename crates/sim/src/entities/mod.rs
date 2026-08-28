//! Entity types for the simulation.
//!
//! Hierarchy based on original source:
//! - `BEntity` (base) - position, velocity, player, actions
//!   - `BObject` - visual, physics (not needed for MVP)
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
mod squad;
mod unit;

pub use base::BaseEntity;
pub use base_site::{Base, BaseId};
pub use squad::{Squad, SquadState};
pub use unit::{Unit, UnitKind, UnitState};
