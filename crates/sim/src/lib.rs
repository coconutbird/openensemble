//! Deterministic simulation crate for Halo Wars network compatibility.
//!
//! This crate implements the core simulation logic that must match vanilla
//! Halo Wars exactly for network compatibility. The wire format for commands
//! must be byte-for-byte identical.

pub mod command;
pub mod entity_id;
pub mod serialize;

pub use command::{Command, CommandType, EntityType};
pub use entity_id::EntityId;
pub use serialize::{SerializeError, deserialize_command, serialize_command};
