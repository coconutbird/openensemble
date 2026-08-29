//! Retail-compatible four-slot game timer manager.

use crate::player::PlayerId;
use crate::sync::SyncChecksum;

use super::World;

const GAME_TIMER_CAPACITY: usize = 4;

/// UI users eligible to see a trigger-created game timer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameTimerAudience {
    /// Version 4, or version 5 without targets, uses the primary local user.
    PrimaryUser,
    /// Version 5 targets users associated with any listed player.
    Players(Vec<PlayerId>),
}

impl GameTimerAudience {
    /// Whether this audience includes the renderer's local player/user.
    #[must_use]
    pub fn includes(&self, player_id: PlayerId, primary_user: bool) -> bool {
        match self {
            Self::PrimaryUser => primary_user,
            Self::Players(players) => players.contains(&player_id),
        }
    }
}

/// One active retail `BGameTimer` plus its trigger-authored UI metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameTimer {
    id: i32,
    slot: u8,
    count_up: bool,
    start_time_ms: u32,
    stop_time_ms: u32,
    current_time_ms: u32,
    last_update_time_ms: u32,
    done: bool,
    paused: bool,
    label_string_id: Option<i32>,
    audience: GameTimerAudience,
    presentation_revision: u32,
}

impl GameTimer {
    /// Trigger-visible timer identifier.
    #[must_use]
    pub const fn id(&self) -> i32 {
        self.id
    }

    /// Fixed manager slot occupied by this timer.
    #[must_use]
    pub const fn slot(&self) -> u8 {
        self.slot
    }

    /// Whether this timer advances toward its stop value.
    #[must_use]
    pub const fn count_up(&self) -> bool {
        self.count_up
    }

    /// Authored initial time in milliseconds.
    #[must_use]
    pub const fn start_time_ms(&self) -> u32 {
        self.start_time_ms
    }

    /// Authored completion time in milliseconds.
    #[must_use]
    pub const fn stop_time_ms(&self) -> u32 {
        self.stop_time_ms
    }

    /// Current authoritative timer value in milliseconds.
    #[must_use]
    pub const fn current_time_ms(&self) -> u32 {
        self.current_time_ms
    }

    /// Whether the timer has reached its authored stop value.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.done
    }

    /// Whether authoritative time advancement is paused.
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Optional localized label requested by the trigger.
    #[must_use]
    pub const fn label_string_id(&self) -> Option<i32> {
        self.label_string_id
    }

    /// UI users eligible to display this timer.
    #[must_use]
    pub const fn audience(&self) -> &GameTimerAudience {
        &self.audience
    }

    /// Monotonic UI selection signal authored when this timer was created.
    #[must_use]
    pub const fn presentation_revision(&self) -> u32 {
        self.presentation_revision
    }

    fn update(&mut self, game_time_ms: u32) {
        let elapsed = game_time_ms.wrapping_sub(self.last_update_time_ms);
        self.last_update_time_ms = game_time_ms;
        if self.done || self.paused {
            return;
        }
        if self.count_up {
            self.current_time_ms = self.current_time_ms.wrapping_add(elapsed);
            self.current_time_ms = self.current_time_ms.min(self.stop_time_ms);
        } else {
            self.current_time_ms = self.current_time_ms.saturating_sub(elapsed);
            self.current_time_ms = self.current_time_ms.max(self.stop_time_ms);
        }
        self.done = self.current_time_ms == self.stop_time_ms;
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.id);
        checksum.hash_u32(u32::from(self.slot));
        checksum.hash_u32(u32::from(self.count_up));
        checksum.hash_u32(self.start_time_ms);
        checksum.hash_u32(self.stop_time_ms);
        checksum.hash_u32(self.current_time_ms);
        checksum.hash_u32(self.last_update_time_ms);
        checksum.hash_u32(u32::from(self.done));
        checksum.hash_u32(u32::from(self.paused));
        checksum.hash_i32(self.label_string_id.unwrap_or(-1));
        match &self.audience {
            GameTimerAudience::PrimaryUser => checksum.hash_u32(0),
            GameTimerAudience::Players(players) => {
                checksum.hash_u32(1);
                checksum.hash_u32(u32::try_from(players.len()).unwrap_or(u32::MAX));
                for player_id in players {
                    checksum.hash_u32(u32::from(*player_id));
                }
            }
        }
        checksum.hash_u32(self.presentation_revision);
    }
}

#[derive(Debug, Default)]
pub(super) struct GameTimerState {
    slots: [Option<GameTimer>; GAME_TIMER_CAPACITY],
    next_id: i32,
    presentation_revision: u32,
}

impl World {
    /// Create a retail game timer, returning `-1` when all four slots are busy.
    pub fn create_game_timer(
        &mut self,
        count_up: bool,
        start_time_ms: u32,
        stop_time_ms: u32,
        label_string_id: Option<i32>,
        audience: GameTimerAudience,
    ) -> i32 {
        let Some(slot) = self.game_timers.slots.iter().position(Option::is_none) else {
            return -1;
        };
        let id = self.game_timers.next_id;
        self.game_timers.next_id = self.game_timers.next_id.wrapping_add(1);
        self.game_timers.presentation_revision = self
            .game_timers
            .presentation_revision
            .wrapping_add(1)
            .max(1);
        self.game_timers.slots[slot] = Some(GameTimer {
            id,
            slot: u8::try_from(slot).unwrap_or(u8::MAX),
            count_up,
            start_time_ms,
            stop_time_ms,
            current_time_ms: start_time_ms,
            last_update_time_ms: self.game_time_ms,
            done: false,
            paused: false,
            label_string_id,
            audience,
            presentation_revision: self.game_timers.presentation_revision,
        });
        id
    }

    /// Destroy one active game timer by trigger-visible ID.
    pub fn destroy_game_timer(&mut self, timer_id: i32) -> bool {
        let Some(timer) = self
            .game_timers
            .slots
            .iter_mut()
            .find(|timer| timer.as_ref().is_some_and(|timer| timer.id == timer_id))
        else {
            return false;
        };
        *timer = None;
        true
    }

    /// Look up one active game timer.
    #[must_use]
    pub fn game_timer(&self, timer_id: i32) -> Option<&GameTimer> {
        self.game_timers
            .slots
            .iter()
            .flatten()
            .find(|timer| timer.id == timer_id)
    }

    /// Iterate active timers in fixed manager-slot order.
    pub fn game_timers(&self) -> impl Iterator<Item = &GameTimer> {
        self.game_timers.slots.iter().flatten()
    }

    /// Latest timer creation revision used by renderer-local UI adapters.
    #[must_use]
    pub const fn game_timer_presentation_revision(&self) -> u32 {
        self.game_timers.presentation_revision
    }

    /// Pause or resume one timer while retaining retail's elapsed-time discard.
    pub fn set_game_timer_paused(&mut self, timer_id: i32, paused: bool) -> bool {
        let Some(timer) = self
            .game_timers
            .slots
            .iter_mut()
            .flatten()
            .find(|timer| timer.id == timer_id)
        else {
            return false;
        };
        timer.paused = paused;
        true
    }

    pub(crate) fn update_game_timers(&mut self) {
        for timer in self.game_timers.slots.iter_mut().flatten() {
            timer.update(self.game_time_ms);
        }
    }
}

impl GameTimerState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.next_id);
        checksum.hash_u32(self.presentation_revision);
        for timer in &self.slots {
            if let Some(timer) = timer {
                checksum.hash_u32(1);
                timer.hash_state(checksum);
            } else {
                checksum.hash_u32(0);
            }
        }
    }
}

#[cfg(test)]
#[path = "timers/tests.rs"]
mod tests;
