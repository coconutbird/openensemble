//! Player state changes and the queue of changes for the game layer.

use super::World;
use crate::player::{PlayerId, PlayerState, PlayerStateChange};
use crate::sync::SyncChecksum;

impl World {
    /// Change the state of a player like `BPlayer_setPlayerState` and queue the change.
    ///
    /// Retail then posts a world event and tells the AI manager that the player left. The game
    /// layer reads the queued changes with
    /// [`take_player_state_changes`](Self::take_player_state_changes). Returns `true` if the
    /// state changed.
    pub fn set_player_state(&mut self, player_id: PlayerId, state: PlayerState) -> bool {
        let game_time_ms = self.game_time_ms;
        let Some(change) = self
            .get_player_mut(player_id)
            .and_then(|player| player.set_player_state(state, game_time_ms))
        else {
            return false;
        };
        self.game_end.player_state_changes.push(change);
        true
    }

    /// Return the queued player state changes in the order they occurred.
    #[must_use]
    pub fn player_state_changes(&self) -> &[PlayerStateChange] {
        &self.game_end.player_state_changes
    }

    /// Take the queued player state changes. Call this once per tick from the game layer.
    pub fn take_player_state_changes(&mut self) -> Vec<PlayerStateChange> {
        std::mem::take(&mut self.game_end.player_state_changes)
    }

    pub(super) fn hash_player_state_changes(&self, checksum: &mut SyncChecksum) {
        let changes = &self.game_end.player_state_changes;
        checksum.hash_u32(u32::try_from(changes.len()).unwrap_or(u32::MAX));
        for change in changes {
            checksum.hash_u32(u32::from(change.player_id));
            checksum.hash_u32(change.previous as u32);
            checksum.hash_u32(change.state as u32);
            checksum.hash_u32(change.event.map_or(0, |event| u32::from(event as u16)));
        }
    }
}
