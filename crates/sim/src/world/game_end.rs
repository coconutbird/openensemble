//! Game type and end-of-game state of the world (`BWorld_checkForGameOver`).

use super::World;
use crate::player::PlayerStateChange;
use crate::sync::SyncChecksum;

/// The game type, game setting 5 (`GameType`).
///
/// Retail reads this setting to select the game-over rule and the game-mode scripts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum GameType {
    /// A skirmish game (0).
    #[default]
    Skirmish = 0,
    /// A campaign game (1).
    Campaign = 1,
    /// A scenario game (2).
    Scenario = 2,
}

/// End-of-game state of the world.
#[derive(Debug, Default)]
pub(super) struct GameEndState {
    /// Game setting 5 (`GameType`).
    game_type: GameType,
    /// Player state changes that the game layer did not take yet.
    pub(super) player_state_changes: Vec<PlayerStateChange>,
}

impl GameEndState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.game_type as u8));
    }
}

impl World {
    /// Return the game type.
    #[must_use]
    pub const fn game_type(&self) -> GameType {
        self.game_end.game_type
    }

    /// Set the game type. Scenario loaders set it from the game settings before they create
    /// players.
    pub const fn set_game_type(&mut self, game_type: GameType) {
        self.game_end.game_type = game_type;
    }
}
