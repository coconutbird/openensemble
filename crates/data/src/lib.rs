//! Data loading crate for Halo Wars game definitions.
//!
//! Handles loading proto objects (units, buildings, techs, etc.) from game files.
//! Uses the `ensemble-rs` crates for file format parsing.

pub mod proto;

// Re-export file format parsers from ensemble-rs
pub use era;
pub use xmb;

pub use proto::{ProtoDatabase, ProtoObject, ProtoSquad, ProtoTech};

