//! Authoritative full-screen fade timing consumed by renderer/UI clients.

use super::PresentationControlState;
use crate::sync::SyncChecksum;
use crate::world::{GeneralEvent, GeneralEventType, World};

/// Timing program for one retail screen transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenFadeSequence {
    /// A single fade from clear to the authored color, or the reverse.
    ToColor { duration_ms: u32, fade_in: bool },
    /// Fade down, hold the color, then fade up. `reverse` swaps both fades.
    Transition {
        fade_down_ms: u32,
        hold_ms: u32,
        fade_up_ms: u32,
        reverse: bool,
    },
}

/// Current sim-authored full-screen color overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenFadeOverlay {
    revision: u32,
    color: [u8; 3],
    alpha: u8,
}

impl ScreenFadeOverlay {
    /// Monotonic identity of the transition that owns this overlay.
    #[must_use]
    pub const fn revision(self) -> u32 {
        self.revision
    }

    /// Authored red, green, and blue channels.
    #[must_use]
    pub const fn color(self) -> [u8; 3] {
        self.color
    }

    /// Current authoritative opacity in the inclusive zero-to-one range.
    #[must_use]
    pub fn opacity(self) -> f32 {
        f32::from(self.alpha) / 255.0
    }

    /// Convert the sim-owned opacity to retail's eight-bit overlay alpha.
    #[must_use]
    pub fn rgba(self) -> [u8; 4] {
        let [red, green, blue] = self.color;
        [red, green, blue, self.alpha]
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct ScreenFadeState {
    next_revision: u32,
    active: Option<ActiveScreenFade>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveScreenFade {
    revision: u32,
    color: [u8; 3],
    start_time_ms: u32,
    alpha: u8,
    sequence: ScreenFadeSequence,
}

impl World {
    /// Start or replace one synchronized full-screen fade program.
    pub fn start_screen_fade(&mut self, color: [u8; 3], sequence: ScreenFadeSequence) -> u32 {
        let interrupted = self.presentation_control.screen_fade.active.is_some();
        self.prepare_fade_completion(interrupted);
        let revision = self.presentation_control.screen_fade.allocate_revision();
        self.presentation_control.screen_fade.active = Some(ActiveScreenFade {
            revision,
            color,
            start_time_ms: self.game_time_ms,
            alpha: sequence.initial_alpha(),
            sequence,
        });
        revision
    }

    /// Return the current overlay, or `None` after its transition completes.
    #[must_use]
    pub fn screen_fade_overlay(&self) -> Option<ScreenFadeOverlay> {
        self.presentation_control
            .screen_fade
            .active
            .map(ActiveScreenFade::overlay)
    }

    /// Whether the most recently created retail fade subscriber has fired.
    #[must_use]
    pub fn screen_fade_completed(&self) -> bool {
        self.fade_completed()
    }

    pub(crate) fn update_screen_fade(&mut self) {
        let completed = self
            .presentation_control
            .screen_fade
            .active
            .as_mut()
            .is_some_and(|fade| fade.update(self.game_time_ms));
        if !completed {
            return;
        }
        self.presentation_control.screen_fade.active = None;
        self.fire_general_event(&GeneralEvent::new(GeneralEventType::FadeCompleted, -1));
    }
}

impl PresentationControlState {
    pub(super) fn hash_screen_fade(&self, checksum: &mut SyncChecksum) {
        self.screen_fade.hash_state(checksum);
    }
}

impl ScreenFadeState {
    fn allocate_revision(&mut self) -> u32 {
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        self.next_revision
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.next_revision);
        if let Some(active) = self.active {
            checksum.hash_u32(1);
            active.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
    }
}

impl ActiveScreenFade {
    fn overlay(self) -> ScreenFadeOverlay {
        ScreenFadeOverlay {
            revision: self.revision,
            color: self.color,
            alpha: self.alpha,
        }
    }

    fn update(&mut self, game_time_ms: u32) -> bool {
        let elapsed_ms = game_time_ms.wrapping_sub(self.start_time_ms);
        let Some(alpha) = self.sequence.alpha_at(elapsed_ms) else {
            return true;
        };
        self.alpha = alpha;
        false
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.revision);
        for channel in self.color {
            checksum.hash_u32(u32::from(channel));
        }
        checksum.hash_u32(self.start_time_ms);
        checksum.hash_u32(u32::from(self.alpha));
        self.sequence.hash_state(checksum);
    }
}

impl ScreenFadeSequence {
    const fn initial_alpha(self) -> u8 {
        match self {
            Self::ToColor { fade_in, .. } => {
                if fade_in {
                    u8::MAX
                } else {
                    0
                }
            }
            Self::Transition { reverse, .. } => {
                if reverse {
                    u8::MAX
                } else {
                    0
                }
            }
        }
    }

    fn alpha_at(self, elapsed_ms: u32) -> Option<u8> {
        match self {
            Self::ToColor {
                duration_ms,
                fade_in,
            } => single_fade_alpha(elapsed_ms, duration_ms, fade_in),
            Self::Transition {
                fade_down_ms,
                hold_ms,
                fade_up_ms,
                reverse,
            } => transition_alpha(elapsed_ms, fade_down_ms, hold_ms, fade_up_ms, reverse),
        }
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        match self {
            Self::ToColor {
                duration_ms,
                fade_in,
            } => {
                checksum.hash_u32(0);
                checksum.hash_u32(duration_ms);
                checksum.hash_u32(u32::from(fade_in));
            }
            Self::Transition {
                fade_down_ms,
                hold_ms,
                fade_up_ms,
                reverse,
            } => {
                checksum.hash_u32(1);
                checksum.hash_u32(fade_down_ms);
                checksum.hash_u32(hold_ms);
                checksum.hash_u32(fade_up_ms);
                checksum.hash_u32(u32::from(reverse));
            }
        }
    }
}

fn single_fade_alpha(elapsed_ms: u32, duration_ms: u32, fade_in: bool) -> Option<u8> {
    if elapsed_ms >= duration_ms {
        return None;
    }
    Some(if fade_in {
        fade_up_alpha(elapsed_ms, duration_ms)
    } else {
        fade_down_alpha(elapsed_ms, duration_ms)
    })
}

fn transition_alpha(
    elapsed_ms: u32,
    fade_down_ms: u32,
    hold_ms: u32,
    fade_up_ms: u32,
    reverse: bool,
) -> Option<u8> {
    let first_ms = if reverse { fade_up_ms } else { fade_down_ms };
    let second_ms = if reverse { fade_down_ms } else { fade_up_ms };
    let hold_end = first_ms.saturating_add(hold_ms);
    let total_ms = hold_end.saturating_add(second_ms);
    if elapsed_ms >= total_ms {
        return None;
    }
    if elapsed_ms < first_ms {
        return Some(if reverse {
            fade_up_alpha(elapsed_ms, first_ms)
        } else {
            fade_down_alpha(elapsed_ms, first_ms)
        });
    }
    if elapsed_ms < hold_end {
        return Some(if reverse { 0 } else { u8::MAX });
    }
    Some(if reverse {
        fade_down_alpha(elapsed_ms - hold_end, second_ms)
    } else {
        fade_up_alpha(elapsed_ms - hold_end, second_ms)
    })
}

fn fade_down_alpha(elapsed_ms: u32, duration_ms: u32) -> u8 {
    scaled_alpha(elapsed_ms, duration_ms)
}

fn fade_up_alpha(elapsed_ms: u32, duration_ms: u32) -> u8 {
    scaled_alpha(duration_ms.saturating_sub(elapsed_ms), duration_ms)
}

fn scaled_alpha(numerator: u32, denominator: u32) -> u8 {
    let scaled = u64::from(numerator).saturating_mul(255) / u64::from(denominator.max(1));
    u8::try_from(scaled).unwrap_or(u8::MAX)
}

#[cfg(test)]
#[path = "fades/tests.rs"]
mod tests;
