//! Entity types for the simulation.
//!
//! Hierarchy based on original source:
//! - BEntity (base) - position, velocity, player, actions
//!   - BObject - visual, physics (not needed for MVP)
//!     - BUnit - individual unit
//!     - BDopple - fog of war ghost
//!     - BProjectile - bullets, missiles
//!   - BSquad - group of units (primary controllable entity)
//!   - BPlatoon - group of squads
//!   - BArmy - player's forces
//!
//! For MVP, we implement Squad as the primary entity.

mod base;
mod squad;

pub use base::BaseEntity;
pub use squad::{Squad, SquadState};

