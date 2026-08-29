//! Persistent retail unit ammunition and regeneration state.

use crate::sync::SyncChecksum;

/// Authoritative ammunition state owned by one unit.
///
/// Retail creates the persistent regeneration action only when the effective
/// player prototype has a positive maximum at unit construction time. That
/// decision remains fixed for the lifetime of the unit, even if technology
/// later changes the prototype maximum.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UnitAmmunition {
    enabled: bool,
    current: f32,
    maximum: f32,
    regeneration_rate: f32,
    regeneration_started: bool,
}

impl UnitAmmunition {
    /// Return whether this unit constructed retail's ammunition action.
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    /// Return the raw ammunition amount stored on the unit.
    #[must_use]
    pub const fn current(self) -> f32 {
        self.current
    }

    /// Return the live player-prototype maximum visible to retail queries.
    #[must_use]
    pub fn maximum(self) -> f32 {
        if self.enabled { self.maximum } else { 0.0 }
    }

    /// Return the live player-prototype regeneration rate.
    #[must_use]
    pub fn regeneration_rate(self) -> f32 {
        if self.enabled {
            self.regeneration_rate
        } else {
            0.0
        }
    }

    /// Return retail's unit ammunition ratio, or zero without a usable maximum.
    #[must_use]
    pub fn percentage(self) -> f32 {
        let maximum = self.maximum();
        if maximum >= f32::EPSILON {
            self.current / maximum
        } else {
            0.0
        }
    }

    /// Directly assign the raw amount, matching `BUnit::setAmmunition`.
    pub fn set_current(&mut self, current: f32) {
        self.current = current;
    }

    /// Add an amount and clamp only the lower boundary, like retail.
    pub fn adjust(&mut self, amount: f32) {
        self.current += amount;
        if self.current <= 0.0 {
            self.current = 0.0;
        }
    }

    pub(crate) fn configure(
        &mut self,
        maximum: f32,
        regeneration_rate: f32,
        start_at_maximum: bool,
    ) {
        self.maximum = finite_or_zero(maximum);
        self.regeneration_rate = finite_or_zero(regeneration_rate);
        self.enabled = self.maximum > 0.0;
        self.current = if self.enabled && start_at_maximum {
            self.maximum
        } else {
            0.0
        };
        self.regeneration_started = false;
    }

    pub(crate) fn reconcile_profile(&mut self, maximum: f32, regeneration_rate: f32) {
        let maximum = finite_or(maximum, self.maximum);
        let regeneration_rate = finite_or(regeneration_rate, self.regeneration_rate);
        if self.enabled && self.maximum != 0.0 {
            self.current *= maximum / self.maximum;
        }
        self.maximum = maximum;
        self.regeneration_rate = regeneration_rate;
    }

    pub(crate) fn has_full_attack(&self, attacks: u32, ammunition_per_attack: f32) -> bool {
        self.current
            >= f32::from(u16::try_from(attacks).unwrap_or(u16::MAX)) * ammunition_per_attack
    }

    pub(crate) fn spend_attack(&mut self, amount: f32) {
        self.adjust(-amount);
    }

    pub(crate) fn advance(&mut self, elapsed: f32) {
        if !self.enabled || !elapsed.is_finite() || elapsed <= 0.0 {
            return;
        }
        if !self.regeneration_started {
            self.regeneration_started = true;
            return;
        }
        if self.current < self.maximum {
            self.current = (self.current + self.regeneration_rate * elapsed).min(self.maximum);
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.enabled));
        checksum.hash_f32(self.current);
        checksum.hash_f32(self.maximum);
        checksum.hash_f32(self.regeneration_rate);
        checksum.hash_u32(u32::from(self.regeneration_started));
    }
}

fn finite_or_zero(value: f32) -> f32 {
    finite_or(value, 0.0)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regeneration_action_initializes_before_accumulating() {
        let mut ammunition = UnitAmmunition::default();
        ammunition.configure(200.0, 9.0, false);

        ammunition.advance(0.05);
        assert_close(ammunition.current(), 0.0);
        ammunition.advance(0.05);
        assert_close(ammunition.current(), 0.45);
    }

    #[test]
    fn start_at_maximum_and_live_maximum_scaling_match_retail() {
        let mut ammunition = UnitAmmunition::default();
        ammunition.configure(800.0, 40.0, true);
        ammunition.set_current(400.0);
        ammunition.reconcile_profile(1_000.0, 50.0);

        assert_close(ammunition.current(), 500.0);
        assert_close(ammunition.maximum(), 1_000.0);
        assert_close(ammunition.regeneration_rate(), 50.0);
        ammunition.adjust(-600.0);
        assert_close(ammunition.current(), 0.0);
    }

    #[test]
    fn technology_does_not_enable_ammunition_on_an_existing_unit() {
        let mut ammunition = UnitAmmunition::default();
        ammunition.configure(0.0, 0.0, false);
        ammunition.reconcile_profile(10.0, 2.0);

        assert!(!ammunition.is_enabled());
        assert_close(ammunition.maximum(), 0.0);
        ammunition.advance(1.0);
        assert_close(ammunition.current(), 0.0);
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
    }
}
