//! Deterministic rigid-body physics that follows the Havok 5.1 world of Halo Wars DE.
//!
//! The retail game steps an `hkpWorld` inside the simulation. This crate rebuilds the parts of
//! that world that the game uses: the terrain heightfield, convex and compound shapes, the
//! group collision filter, the contact solver, hinge constraints and deactivation.
//!
//! The crate knows nothing about the game. The `sim` crate creates the bodies, applies the
//! vehicle actions before each step and reads the results back.
//!
//! The results must be the same on each machine. The crate uses `f32` only, processes bodies
//! and pairs in a sorted order and never iterates a hash map.

mod body;
mod collide;
mod contact;
mod dynamics;
mod filter;
mod heightfield;
mod math;
mod shape;
mod world;

pub use body::{BodyDesc, BodyId, MotionType};
pub use collide::RayHit;
pub use contact::{ContactPoint, ContactProperties, Manifold, ManifoldId, ManifoldPoint};
pub use filter::{FilterInfo, GroupFilter, Layer};
pub use heightfield::{HeightField, HeightFieldError};
pub use math::Transform;
pub use shape::{DEFAULT_CONVEX_RADIUS, Shape};
pub use world::{
    ConstraintId, ContactAdded, ContactListener, ContactResponse, LimitedHingeDesc,
    MAX_SUBSTEP_SECONDS, World, WorldConfig,
};
