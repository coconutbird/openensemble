//! Trigger screen text slots (`BUser::mUserMessages`) for UI adapters.

use super::super::World;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// The number of user message slots (`USER_MESSAGES_MAX`).
pub const USER_MESSAGE_COUNT: usize = 20;
/// The text that leaves the slot text unchanged.
const NULL_TEXT: &str = "NULL";
/// The point size that gives scale 1 (`USER_MESSAGES_DEFAULT_POINT`).
const DEFAULT_POINT: f32 = 24.0;

/// The text alignment of a user message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UserMessageJustify {
    /// Left aligned (retail font flag 0).
    #[default]
    Left,
    /// Centered (retail font flag 1).
    Center,
    /// Right aligned (retail font flag 4).
    Right,
}

/// One user message slot. All users share the slots; the player mask selects who sees the
/// text.
#[derive(Debug, Clone, PartialEq)]
pub struct UserMessage {
    text: String,
    player_mask: u32,
    position: [f32; 2],
    justify: UserMessageJustify,
    scale: f32,
    alpha: f32,
    color: [u8; 3],
    enabled: bool,
}

impl Default for UserMessage {
    fn default() -> Self {
        Self {
            text: String::new(),
            player_mask: 0,
            position: [0.0, 0.0],
            justify: UserMessageJustify::Left,
            scale: 1.0,
            alpha: 1.0,
            color: [u8::MAX; 3],
            enabled: false,
        }
    }
}

impl UserMessage {
    /// The text of the slot.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the user of a player sees this slot. Retail uses bit `player - 1`.
    #[must_use]
    pub fn shown_to(&self, player_id: PlayerId) -> bool {
        self.player_mask & player_bit(i32::from(player_id)) != 0
    }

    /// The offset in pixels from the safe-area corner.
    #[must_use]
    pub const fn position(&self) -> [f32; 2] {
        self.position
    }

    /// The text alignment.
    #[must_use]
    pub const fn justify(&self) -> UserMessageJustify {
        self.justify
    }

    /// The font scale (the authored point size divided by 24).
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// The text opacity.
    #[must_use]
    pub const fn alpha(&self) -> f32 {
        self.alpha
    }

    /// The text color.
    #[must_use]
    pub const fn color(&self) -> [u8; 3] {
        self.color
    }

    /// Whether the slot shows.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_string(&self.text);
        checksum.hash_u32(self.player_mask);
        checksum.hash_f32(self.position[0]);
        checksum.hash_f32(self.position[1]);
        checksum.hash_u32(self.justify as u32);
        checksum.hash_f32(self.scale);
        checksum.hash_f32(self.alpha);
        for channel in self.color {
            checksum.hash_u32(u32::from(channel));
        }
        checksum.hash_u32(u32::from(self.enabled));
    }
}

/// The inputs of one `BUser_setUserMessage` call. A negative number keeps the old value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UserMessageUpdate {
    pub(crate) players: Vec<i32>,
    pub(crate) text: String,
    pub(crate) position: [f32; 2],
    pub(crate) justify: i32,
    pub(crate) point: f32,
    pub(crate) alpha: f32,
    pub(crate) color: Option<[u8; 3]>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct UserMessageState {
    slots: [UserMessage; USER_MESSAGE_COUNT],
}

impl World {
    /// Return a user message slot, or `None` for an index outside `0..20`.
    #[must_use]
    pub fn user_message(&self, index: usize) -> Option<&UserMessage> {
        self.presentation_control.user_messages.slots.get(index)
    }

    /// `BUser_setUserMessage` (`0x1405A5900`). Returns `false` for an index outside the slots;
    /// retail does not test the index.
    pub(crate) fn set_user_message(&mut self, index: i32, update: UserMessageUpdate) -> bool {
        let Some(slot) = usize::try_from(index)
            .ok()
            .and_then(|index| self.presentation_control.user_messages.slots.get_mut(index))
        else {
            return false;
        };
        if update.text != NULL_TEXT {
            slot.text = update.text;
        }
        if !update.players.is_empty() {
            slot.player_mask = update
                .players
                .iter()
                .fold(0, |mask, player| mask | player_bit(*player));
        }
        if update.position[0] >= 0.0 {
            slot.position[0] = update.position[0];
        }
        if update.position[1] >= 0.0 {
            slot.position[1] = update.position[1];
        }
        if update.point >= 0.0 {
            slot.scale = update.point / DEFAULT_POINT;
        }
        if update.alpha >= 0.0 {
            slot.alpha = update.alpha;
        }
        if let Some(color) = update.color {
            slot.color = color;
        }
        slot.enabled = update.enabled;
        match update.justify {
            0 => slot.justify = UserMessageJustify::Left,
            1 => slot.justify = UserMessageJustify::Center,
            2 => slot.justify = UserMessageJustify::Right,
            _ => {}
        }
        true
    }
}

impl UserMessageState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        for slot in &self.slots {
            slot.hash_state(checksum);
        }
    }
}

/// Retail `1 << (player - 1)`; the x64 shift uses the low five bits of the count.
fn player_bit(player: i32) -> u32 {
    1_u32.wrapping_shl(player.wrapping_sub(1).cast_unsigned())
}
