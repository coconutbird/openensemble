//! The physics of the simulation: the retail Havok world of Halo Wars DE.
//!
//! Retail steps an `hkpWorld` inside the sim (`BPhysicsWorld_update`, `0x1407BBB40`). The
//! [`::physics`] crate rebuilds that world. This module holds the game rules around it:
//!
//! - which entities get a body and how (`bodies`, `template`, `shape`);
//! - the terrain body from the sim heights (`terrain`);
//! - the vehicle actions and the gravity ball pull action of the Wave, applied before each
//!   substep in action creation order (`vehicle_actions`, `gravity_ball`);
//! - the substep loop, the terrain penetration fix and the contact listeners (`step`);
//! - the readback of the bodies into their units and the physics vehicle squads (`readback`);
//! - the body access of the gameplay code (`access`).

mod access;
mod bodies;
pub(crate) mod death;
mod gravity_ball;
mod readback;
mod shape;
mod state;
mod step;
mod template;
mod terrain;
#[cfg(test)]
pub(crate) mod test_support;
mod unit;
pub(crate) mod vehicle_actions;
mod vehicle_hull;

pub use death::ClamshellAnimation;
pub(crate) use gravity_ball::GravityBallSpring;
pub use state::SimPhysics;
pub use template::{ObjectPhysicsTemplates, PhysicsBlueprintBody, PhysicsTemplate};
pub(crate) use terrain::terrain_height_field;
pub use unit::UnitPhysics;
