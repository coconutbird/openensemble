//! Derived command types matching vanilla Halo Wars.
//!
//! Each command type extends the base Command with additional fields.

mod game;
mod power;
mod work;

pub use game::GameCommand;
pub use power::PowerCommand;
pub use work::WorkCommand;
