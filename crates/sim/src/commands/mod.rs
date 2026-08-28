//! Derived command types matching vanilla Halo Wars.
//!
//! Each command type extends the base Command with additional fields.

mod building;
mod game;
mod power;
mod work;

pub use building::{BuildingCommand, BuildingCommandType};
pub use game::{GameCommand, GameCommandType};
pub use power::command_flags as power_command_flags;
pub use power::{PowerCommand, PowerCommandType};
pub use work::WorkCommand;
pub use work::command_flags as work_command_flags;
