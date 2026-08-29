//! Retail squad cryo action state.

use super::Squad;
use crate::sync::SyncChecksum;

/// Current phase of retail's persistent `SquadCryo` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SquadCryoState {
    /// No active cryo action.
    #[default]
    None = 0,
    /// Cryo points have been depleted but remain above zero.
    Freezing = 1,
    /// Cryo points reached zero and member units shatter when killed.
    Frozen = 2,
    /// Cryo points are regenerating toward their maximum.
    Thawing = 3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SquadCryoConfig {
    pub maximum_points: f32,
    pub thaw_speed: f32,
    pub freezing_thaw_time: f32,
    pub frozen_thaw_time: f32,
    pub freezing_speed_modifier: f32,
    pub freezing_damage_modifier: f32,
    pub frozen_damage_modifier: f32,
}

impl SquadCryoConfig {
    pub(crate) fn is_valid(self) -> bool {
        self.maximum_points.is_finite() && self.maximum_points > 0.0
    }
}

impl Default for SquadCryoConfig {
    fn default() -> Self {
        Self {
            maximum_points: 0.0,
            thaw_speed: 0.0,
            freezing_thaw_time: 0.0,
            frozen_thaw_time: 0.0,
            freezing_speed_modifier: 1.0,
            freezing_damage_modifier: 1.0,
            frozen_damage_modifier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SquadCryoEffect {
    pub frozen: bool,
    pub movement_modifier: f32,
    pub damage_taken_modifier: f32,
}

impl Default for SquadCryoEffect {
    fn default() -> Self {
        Self {
            frozen: false,
            movement_modifier: 1.0,
            damage_taken_modifier: 1.0,
        }
    }
}

/// Persistent data owned by one retail `SquadCryo` action.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SquadCryo {
    state: SquadCryoState,
    current_points: f32,
    time_until_thaw: f32,
    config: SquadCryoConfig,
}

impl SquadCryo {
    pub(crate) fn add(&mut self, amount: f32, config: SquadCryoConfig) -> bool {
        if !amount.is_finite() || amount <= 0.0 {
            return false;
        }
        if self.state == SquadCryoState::None {
            if !config.is_valid() {
                return false;
            }
            self.config = config;
            self.current_points = config.maximum_points;
        } else {
            // Retail's power-specific thaw times are layered onto the existing
            // SquadCryo action with max semantics on every application.
            self.config.freezing_thaw_time = self
                .config
                .freezing_thaw_time
                .max(config.freezing_thaw_time);
            self.config.frozen_thaw_time =
                self.config.frozen_thaw_time.max(config.frozen_thaw_time);
        }
        self.current_points = (self.current_points - amount).max(0.0);
        match self.state {
            SquadCryoState::Freezing => {
                if self.current_points <= f32::EPSILON {
                    self.transition_to(SquadCryoState::Frozen);
                }
                // Retail overwrites startFrozen's timer on this transition.
                self.time_until_thaw = self.config.freezing_thaw_time;
            }
            SquadCryoState::Frozen => {
                self.time_until_thaw = self.config.frozen_thaw_time;
            }
            SquadCryoState::Thawing | SquadCryoState::None => {
                if self.current_points <= f32::EPSILON {
                    self.transition_to(SquadCryoState::Frozen);
                } else {
                    self.transition_to(SquadCryoState::Freezing);
                }
            }
        }
        true
    }

    pub(crate) fn advance(&mut self, dt: f32) -> bool {
        if !dt.is_finite() || dt <= 0.0 || self.state == SquadCryoState::None {
            return false;
        }
        let previous_effect = self.effect();
        match self.state {
            SquadCryoState::Thawing => {
                self.current_points = (self.current_points + self.config.thaw_speed * dt)
                    .clamp(0.0, self.config.maximum_points);
                if self.current_points >= self.config.maximum_points {
                    self.transition_to(SquadCryoState::None);
                }
            }
            SquadCryoState::Freezing | SquadCryoState::Frozen => {
                self.time_until_thaw -= dt;
                if self.time_until_thaw <= 0.0 {
                    self.transition_to(SquadCryoState::Thawing);
                }
            }
            SquadCryoState::None => {}
        }
        previous_effect != self.effect()
    }

    pub(crate) const fn state(self) -> SquadCryoState {
        self.state
    }

    pub(crate) const fn current_points(self) -> f32 {
        self.current_points
    }

    pub(crate) const fn maximum_points(self) -> f32 {
        self.config.maximum_points
    }

    pub(crate) const fn time_until_thaw(self) -> f32 {
        self.time_until_thaw
    }

    pub(crate) fn effect(self) -> SquadCryoEffect {
        match self.state {
            SquadCryoState::None => SquadCryoEffect::default(),
            SquadCryoState::Freezing | SquadCryoState::Thawing => SquadCryoEffect {
                frozen: false,
                movement_modifier: self.config.freezing_speed_modifier,
                damage_taken_modifier: self.config.freezing_damage_modifier,
            },
            SquadCryoState::Frozen => SquadCryoEffect {
                frozen: true,
                movement_modifier: 0.0,
                damage_taken_modifier: self.config.frozen_damage_modifier,
            },
        }
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.state as u32);
        checksum.hash_f32(self.current_points);
        checksum.hash_f32(self.time_until_thaw);
        checksum.hash_f32(self.config.maximum_points);
        checksum.hash_f32(self.config.thaw_speed);
        checksum.hash_f32(self.config.freezing_thaw_time);
        checksum.hash_f32(self.config.frozen_thaw_time);
        checksum.hash_f32(self.config.freezing_speed_modifier);
        checksum.hash_f32(self.config.freezing_damage_modifier);
        checksum.hash_f32(self.config.frozen_damage_modifier);
    }

    fn transition_to(&mut self, state: SquadCryoState) {
        self.state = state;
        self.time_until_thaw = match state {
            SquadCryoState::Freezing => self.config.freezing_thaw_time,
            SquadCryoState::Frozen => self.config.frozen_thaw_time,
            SquadCryoState::Thawing | SquadCryoState::None => 0.0,
        };
        if state == SquadCryoState::None {
            self.current_points = self.config.maximum_points;
        }
    }
}

impl Squad {
    /// Return the current authoritative cryo phase.
    #[must_use]
    pub const fn cryo_state(&self) -> SquadCryoState {
        self.cryo.state()
    }

    /// Return the remaining cryo resistance points.
    #[must_use]
    pub const fn cryo_points(&self) -> f32 {
        self.cryo.current_points()
    }

    /// Return the maximum cryo resistance captured when the action began.
    #[must_use]
    pub const fn maximum_cryo_points(&self) -> f32 {
        self.cryo.maximum_points()
    }

    /// Return the remaining delay before the squad begins thawing.
    #[must_use]
    pub const fn cryo_thaw_delay(&self) -> f32 {
        self.cryo.time_until_thaw()
    }

    /// Return whether movement and attacks are blocked by the frozen phase.
    #[must_use]
    pub const fn is_cryo_frozen(&self) -> bool {
        matches!(self.cryo_state(), SquadCryoState::Frozen)
    }

    pub(crate) fn cryo_movement_modifier(&self) -> f32 {
        self.cryo.effect().movement_modifier
    }

    pub(crate) fn hash_cryo_state(&self, checksum: &mut SyncChecksum) {
        self.cryo.hash_state(checksum);
    }
}
