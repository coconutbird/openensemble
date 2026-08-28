//! Per-unit energy-shield state.

use crate::sync::SyncChecksum;

/// Shield coverage authored by a proto object's `DamageType` entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ShieldCoverage {
    /// This unit has no integral damage-absorbing shield.
    #[default]
    None,
    /// The shield absorbs damage from every direction.
    Full,
}

/// Runtime energy-shield values and recharge-action state.
#[derive(Debug, Clone, Copy)]
pub struct UnitShields {
    /// Shield coverage supplied by the proto object.
    pub coverage: ShieldCoverage,
    /// Current shield points.
    pub current: f32,
    /// Player-modified maximum shield points.
    pub maximum: f32,
    regen_rate_scalar: f32,
    regen_delay_scalar: f32,
    recharge_requested: bool,
    seconds_since_damage: Option<f32>,
    recharge_time_remaining: f32,
}

impl Default for UnitShields {
    fn default() -> Self {
        Self {
            coverage: ShieldCoverage::None,
            current: 0.0,
            maximum: 0.0,
            regen_rate_scalar: 1.0,
            regen_delay_scalar: 1.0,
            recharge_requested: false,
            seconds_since_damage: None,
            recharge_time_remaining: 0.0,
        }
    }
}

impl UnitShields {
    /// Configure integral shield coverage and its initial maximum.
    ///
    /// Retail units begin with zero shield points and immediately request a
    /// recharge action, even when technology has not granted a positive
    /// maximum yet.
    pub fn configure(&mut self, coverage: ShieldCoverage, maximum: f32) {
        self.coverage = coverage;
        self.current = 0.0;
        self.maximum = valid_nonnegative(maximum).unwrap_or_default();
        self.recharge_requested = self.is_enabled();
        self.seconds_since_damage = None;
        self.recharge_time_remaining = 0.0;
    }

    /// Return whether the proto object supplies an integral shield.
    #[must_use]
    pub fn is_enabled(self) -> bool {
        self.coverage != ShieldCoverage::None
    }

    /// Set current shield points, clamped to the current maximum.
    pub fn set_current(&mut self, current: f32) {
        if let Some(current) = valid_nonnegative(current) {
            self.current = current.min(self.maximum);
        }
    }

    /// Change the player-modified maximum without changing current points.
    ///
    /// The return value reports whether the maximum increased enough to need
    /// a new recharge request.
    pub(crate) fn set_maximum(&mut self, maximum: f32) -> bool {
        let Some(maximum) = valid_nonnegative(maximum) else {
            return false;
        };
        let previous = self.maximum;
        self.maximum = maximum;
        maximum > previous + comparison_tolerance(maximum, previous)
    }

    pub(crate) fn set_regen_scalars(&mut self, rate: f32, delay: f32) {
        self.regen_rate_scalar = valid_nonnegative(rate).unwrap_or(1.0);
        self.regen_delay_scalar = valid_nonnegative(delay).unwrap_or(1.0);
    }

    pub(crate) fn set_regen_delay_scalar(&mut self, delay: f32) {
        self.regen_delay_scalar = valid_nonnegative(delay).unwrap_or(1.0);
    }

    pub(crate) fn regen_delay_scalar(self) -> f32 {
        self.regen_delay_scalar
    }

    /// Absorb already-modified incoming damage and return its HP overflow.
    pub(crate) fn absorb_damage(&mut self, damage: f32) -> f32 {
        if self.coverage != ShieldCoverage::Full || self.current <= 0.0 {
            return damage;
        }
        let absorbed = damage.min(self.current);
        self.current -= absorbed;
        (damage - absorbed).max(0.0)
    }

    pub(crate) fn notify_damaged(&mut self) {
        if self.is_enabled() {
            self.recharge_requested = true;
            self.seconds_since_damage = Some(0.0);
        }
    }

    pub(crate) fn request_recharge(&mut self) {
        if self.is_enabled() && self.current < self.maximum {
            self.recharge_requested = true;
        }
    }

    pub(crate) fn clear_recharge_request(&mut self) {
        self.recharge_requested = false;
    }

    pub(crate) fn take_recharge_request(&mut self, delay: f32) -> bool {
        if !self.recharge_requested {
            return false;
        }
        let ready = self
            .seconds_since_damage
            .is_none_or(|elapsed| elapsed > delay.max(0.0));
        if ready {
            self.recharge_requested = false;
        }
        ready
    }

    pub(crate) fn start_recharge(&mut self, duration: f32) {
        if self.is_enabled() && duration.is_finite() && duration > 0.0 {
            self.recharge_time_remaining = duration;
        }
    }

    pub(crate) fn advance_recharge(&mut self, dt: f32, player_rate: f32) {
        if self.recharge_time_remaining <= 0.0 || dt <= 0.0 {
            return;
        }
        let rate = self.maximum * player_rate * self.regen_rate_scalar;
        if rate.is_finite() && rate >= 0.0 {
            self.current = (self.current + rate * dt).min(self.maximum);
        }
        self.recharge_time_remaining = (self.recharge_time_remaining - dt).max(0.0);
    }

    pub(crate) fn advance_damage_clock(&mut self, dt: f32) {
        if let Some(elapsed) = &mut self.seconds_since_damage {
            *elapsed += dt;
        }
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.coverage as u32);
        checksum.hash_f32(self.current);
        checksum.hash_f32(self.maximum);
        checksum.hash_f32(self.regen_rate_scalar);
        checksum.hash_f32(self.regen_delay_scalar);
        checksum.hash_u32(u32::from(self.recharge_requested));
        checksum.hash_f32(self.seconds_since_damage.unwrap_or(-1.0));
        checksum.hash_f32(self.recharge_time_remaining);
    }
}

fn valid_nonnegative(value: f32) -> Option<f32> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn comparison_tolerance(left: f32, right: f32) -> f32 {
    f32::EPSILON * left.abs().max(right.abs()).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_shield_absorbs_damage_before_returning_overflow() {
        let mut shields = UnitShields::default();
        shields.configure(ShieldCoverage::Full, 10.0);
        shields.set_current(10.0);

        assert!((shields.absorb_damage(6.0) - 0.0).abs() < f32::EPSILON);
        assert!((shields.current - 4.0).abs() < f32::EPSILON);
        assert!((shields.absorb_damage(9.0) - 5.0).abs() < f32::EPSILON);
        assert!(shields.current.abs() < f32::EPSILON);
    }

    #[test]
    fn recharge_request_uses_strict_post_damage_delay() {
        let mut shields = UnitShields::default();
        shields.configure(ShieldCoverage::Full, 10.0);
        assert!(shields.take_recharge_request(20.0));

        shields.notify_damaged();
        shields.advance_damage_clock(20.0);
        assert!(!shields.take_recharge_request(20.0));
        shields.advance_damage_clock(0.05);
        assert!(shields.take_recharge_request(20.0));
    }
}
