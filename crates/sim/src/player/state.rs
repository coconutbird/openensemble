//! Player end states: resign, defeat, disconnect and win (`BPlayer_setPlayerState`).

use super::{Player, PlayerId, PlayerState};
use crate::sync::SyncChecksum;

/// The world event that retail posts when a player state changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum PlayerStateEvent {
    /// The player resigned (world event 53).
    Resigned = 53,
    /// The player was defeated (world event 54).
    Defeated = 54,
    /// The player won (world event 56).
    Won = 56,
}

/// One change of a player state, for the game layer and the AI to consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerStateChange {
    /// The player whose state changed.
    pub player_id: PlayerId,
    /// The state before the change.
    pub previous: PlayerState,
    /// The new state.
    pub state: PlayerState,
    /// The world event that retail posts, if any. A change to `Disconnected` posts no event.
    pub event: Option<PlayerStateEvent>,
}

/// Retail end-state fields of `BPlayer`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PlayerEndState {
    /// `+708`: the player asked to disconnect.
    disconnected: bool,
    /// `+780`: game time in whole seconds when the player first left the `Playing` state.
    end_time_seconds: Option<u32>,
    /// `+817 & 0x10`: the world must kill the units of the player at its next update.
    units_cleanup_pending: bool,
}

impl Player {
    /// Change the player state like `BPlayer_setPlayerState`.
    ///
    /// - A request for `Disconnected` sets the disconnected flag. While the player plays, the
    ///   request becomes `Resigned`.
    /// - The first change away from `Playing` stores the time in seconds, rounded to the
    ///   nearest second.
    /// - `Resigned` and `Defeated` mark the units of the player for cleanup.
    ///
    /// Returns `None` if the state does not change.
    pub fn set_player_state(
        &mut self,
        requested: PlayerState,
        game_time_ms: u32,
    ) -> Option<PlayerStateChange> {
        let mut state = requested;
        if requested == PlayerState::Disconnected {
            self.end_state.disconnected = true;
            if self.state == PlayerState::Playing {
                state = PlayerState::Resigned;
            }
        }
        if state == self.state {
            return None;
        }
        let previous = self.state;
        if state != PlayerState::Playing && previous == PlayerState::Playing {
            self.end_state.end_time_seconds = Some(game_time_ms.wrapping_add(500) / 1000);
        }
        self.state = state;
        if matches!(state, PlayerState::Resigned | PlayerState::Defeated) {
            self.end_state.units_cleanup_pending = true;
        }
        let event = match state {
            PlayerState::Resigned => Some(PlayerStateEvent::Resigned),
            PlayerState::Defeated => Some(PlayerStateEvent::Defeated),
            PlayerState::Won => Some(PlayerStateEvent::Won),
            PlayerState::Playing | PlayerState::Disconnected => None,
        };
        Some(PlayerStateChange {
            player_id: self.id,
            previous,
            state,
            event,
        })
    }

    /// Return whether the player asked to disconnect.
    #[must_use]
    pub const fn disconnected(&self) -> bool {
        self.end_state.disconnected
    }

    /// Return the game time in whole seconds when the player first left the `Playing` state.
    #[must_use]
    pub const fn end_time_seconds(&self) -> Option<u32> {
        self.end_state.end_time_seconds
    }

    /// Return whether the world must kill the units of this player at its next update.
    #[must_use]
    pub const fn units_cleanup_pending(&self) -> bool {
        self.end_state.units_cleanup_pending
    }

    /// Clear the cleanup flag after the world killed the units of this player.
    pub(crate) const fn clear_units_cleanup_pending(&mut self) {
        self.end_state.units_cleanup_pending = false;
    }

    pub(crate) fn hash_end_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.end_state.disconnected));
        checksum.hash_u32(self.end_state.end_time_seconds.unwrap_or(u32::MAX));
        checksum.hash_u32(u32::from(self.end_state.units_cleanup_pending));
    }
}

#[cfg(test)]
mod tests;
