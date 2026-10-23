//! Trigger hint messages (`BHintManager`) for UI adapters.

use std::collections::BTreeMap;

use super::super::World;
use crate::player::{PlayerId, PlayerType};
use crate::sync::SyncChecksum;

/// The hint message that the user of one player shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HintMessage {
    string_id: i32,
    duration_s: f32,
    shown_at_ms: u32,
}

impl HintMessage {
    /// Localized string ID of the hint text.
    #[must_use]
    pub const fn string_id(self) -> i32 {
        self.string_id
    }

    /// Display time in seconds from [`shown_at_ms`](Self::shown_at_ms). Retail uses whole
    /// seconds. Zero means that the hint expires at once.
    #[must_use]
    pub const fn duration_s(self) -> f32 {
        self.duration_s
    }

    /// Game time in milliseconds when the trigger showed the hint or reset its time.
    #[must_use]
    pub const fn shown_at_ms(self) -> u32 {
        self.shown_at_ms
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct HintMessageState {
    players: BTreeMap<PlayerId, HintMessage>,
}

impl World {
    /// Return the hint message that the user of a player shows.
    #[must_use]
    pub fn hint_message(&self, player_id: PlayerId) -> Option<HintMessage> {
        self.presentation_control
            .hints
            .players
            .get(&player_id)
            .copied()
    }

    /// `BHintManager_addHint` (`0x1400473C0`) for the user of each human player.
    ///
    /// The same string ID as the current hint only resets its time. Another hint expires the
    /// current hint, and the new hint shows only if the player is in `recipients`.
    pub(crate) fn show_hint_message(
        &mut self,
        string_id: i32,
        recipients: &[i32],
        duration_s: f32,
    ) {
        let now_ms = self.game_time_ms;
        for player in &self.players {
            if player.player_type != PlayerType::Human {
                continue;
            }
            let hints = &mut self.presentation_control.hints.players;
            if let Some(current) = hints.get_mut(&player.id) {
                if current.string_id == string_id {
                    current.duration_s = duration_s;
                    current.shown_at_ms = now_ms;
                    continue;
                }
                hints.remove(&player.id);
            }
            if recipients.contains(&i32::from(player.id)) {
                hints.insert(
                    player.id,
                    HintMessage {
                        string_id,
                        duration_s,
                        shown_at_ms: now_ms,
                    },
                );
            }
        }
    }

    /// `HintMessageDestroy` (815): remove the current hint of each user. With a string ID,
    /// remove only a hint that has this ID.
    pub(crate) fn destroy_hint_message(&mut self, string_id: Option<i32>) {
        self.presentation_control
            .hints
            .players
            .retain(|_, hint| string_id.is_some_and(|string_id| hint.string_id != string_id));
    }
}

impl HintMessageState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.players.len()).unwrap_or(u32::MAX));
        for (player_id, hint) in &self.players {
            checksum.hash_u32(u32::from(*player_id));
            checksum.hash_i32(hint.string_id);
            checksum.hash_f32(hint.duration_s);
            checksum.hash_u32(hint.shown_at_ms);
        }
    }
}
