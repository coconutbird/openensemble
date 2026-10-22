//! Player state changes and the queue of changes for the game layer.

use super::World;
use crate::player::{Player, PlayerId, PlayerState, PlayerStateChange};
use crate::sync::SyncChecksum;

impl World {
    /// Change the state of a player like `BPlayer_setPlayerState` and queue the change.
    ///
    /// A change to `Resigned`, `Defeated` or `Won` posts a world event, and `BWorld_notify`
    /// then checks at once whether the game is over ([`game_result`](Self::game_result)). The
    /// game layer reads the queued changes with
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
        if change.event.is_some() {
            self.check_for_game_over();
        }
        true
    }

    /// Resign a playing player, like game command 7 (`BPlayer_resign`).
    ///
    /// Returns `false` if the player does not exist or does not play.
    pub fn resign_player(&mut self, player_id: PlayerId) -> bool {
        self.get_player(player_id).is_some_and(Player::is_playing)
            && self.set_player_state(player_id, PlayerState::Resigned)
    }

    /// Disconnect a playing player, like game command 8 (`BPlayer_disconnect`). The player
    /// resigns and keeps the disconnected flag.
    ///
    /// Returns `false` if the player does not exist or does not play.
    pub fn disconnect_player(&mut self, player_id: PlayerId) -> bool {
        self.get_player(player_id).is_some_and(Player::is_playing)
            && self.set_player_state(player_id, PlayerState::Disconnected)
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
