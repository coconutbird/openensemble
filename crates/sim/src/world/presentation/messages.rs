//! Trigger text messages (`ShowMessage`) for renderer and UI adapters.

use std::collections::BTreeMap;

use super::super::World;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// The last text message that a trigger showed to one player.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerMessage {
    revision: u32,
    string_id: i32,
    sound: Option<String>,
    queue_sound: bool,
    duration_seconds: u32,
}

impl TriggerMessage {
    /// Monotonic identity. A new message has a larger revision.
    #[must_use]
    pub const fn revision(&self) -> u32 {
        self.revision
    }

    /// Localized string ID of the message text.
    #[must_use]
    pub const fn string_id(&self) -> i32 {
        self.string_id
    }

    /// Sound cue to play with the message, if any.
    #[must_use]
    pub fn sound(&self) -> Option<&str> {
        self.sound.as_deref()
    }

    /// Whether the sound goes into the sound queue.
    #[must_use]
    pub const fn queue_sound(&self) -> bool {
        self.queue_sound
    }

    /// Display time in whole seconds. Retail divides the authored milliseconds by 1000.
    #[must_use]
    pub const fn duration_seconds(&self) -> u32 {
        self.duration_seconds
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct TriggerMessageState {
    next_revision: u32,
    players: BTreeMap<PlayerId, TriggerMessage>,
}

impl World {
    /// Return the last trigger text message for one player.
    #[must_use]
    pub fn trigger_message(&self, player_id: PlayerId) -> Option<&TriggerMessage> {
        self.presentation_control.messages.players.get(&player_id)
    }

    pub(crate) fn show_trigger_message(
        &mut self,
        player_id: PlayerId,
        string_id: i32,
        sound: Option<String>,
        queue_sound: bool,
        duration_ms: u32,
    ) {
        if self.get_player(player_id).is_none() {
            return;
        }
        let messages = &mut self.presentation_control.messages;
        messages.next_revision = messages.next_revision.wrapping_add(1).max(1);
        let message = TriggerMessage {
            revision: messages.next_revision,
            string_id,
            sound,
            queue_sound,
            duration_seconds: duration_ms / 1000,
        };
        messages.players.insert(player_id, message);
    }
}

impl TriggerMessageState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.next_revision);
        checksum.hash_u32(u32::try_from(self.players.len()).unwrap_or(u32::MAX));
        for (player_id, message) in &self.players {
            checksum.hash_u32(u32::from(*player_id));
            checksum.hash_u32(message.revision);
            checksum.hash_i32(message.string_id);
            checksum.hash_string(message.sound.as_deref().unwrap_or_default());
            checksum.hash_u32(u32::from(message.sound.is_some()));
            checksum.hash_u32(u32::from(message.queue_sound));
            checksum.hash_u32(message.duration_seconds);
        }
    }
}
