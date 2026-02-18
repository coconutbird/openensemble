//! Data loading crate for Halo Wars game definitions.
//!
//! Handles loading proto objects (units, buildings, techs, etc.) from game files.
//! Uses the `ensemble-rs` crates for file format parsing.

pub mod database;
pub mod paths;
pub mod proto;

// Re-export file format parsers from ensemble-rs
pub use era;
pub use xmb;
pub use xtd;

pub use database::{
    Ability, Civilization, DamageType, DatabaseError, GameDatabase, GameMode, Leader, Power,
    WeaponType,
};
pub use paths::{GAME_DIR_ENV_VAR, era_path, game_dir, game_file, is_valid_game_dir};
pub use proto::{ProtoDatabase, ProtoObject, ProtoSquad, ProtoTech, TechEffect};
