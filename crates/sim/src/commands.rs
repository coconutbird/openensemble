//! Derived command types matching vanilla Halo Wars.
//!
//! Each command type extends the base Command with additional fields.

mod building;
mod game;
mod power;
mod power_input;
mod work;

pub use building::{BuildingCommand, BuildingCommandType};
pub use game::{GameCommand, GameCommandType};
pub use power::command_flags as power_command_flags;
pub use power::{PowerCommand, PowerCommandType};
pub use power_input::command_flags as power_input_command_flags;
pub use power_input::{PowerInputCommand, PowerInputCommandType, PowerUserId};
pub use work::command_flags as work_command_flags;
pub use work::{UserWorkTarget, WorkCommand};

/// Parsed command of one supported command type.
///
/// The dispatcher returns this payload, and the command queue stores it.
#[derive(Debug, Clone)]
pub enum CommandPayload {
    /// Unit work command.
    Work(WorkCommand),
    /// Leader power activation command.
    Power(PowerCommand),
    /// Leader power input command.
    PowerInput(PowerInputCommand),
    /// Building command.
    Building(BuildingCommand),
    /// Game-level command.
    Game(GameCommand),
}

impl CommandPayload {
    /// Get the player ID from the base command.
    #[must_use]
    pub fn player_id(&self) -> i32 {
        match self {
            CommandPayload::Work(cmd) => cmd.base.player_id,
            CommandPayload::Power(cmd) => cmd.base.player_id,
            CommandPayload::PowerInput(cmd) => cmd.base.player_id,
            CommandPayload::Building(cmd) => cmd.base.player_id,
            CommandPayload::Game(cmd) => cmd.base.player_id,
        }
    }
}
