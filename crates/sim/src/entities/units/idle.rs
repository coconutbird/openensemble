//! Retail per-unit idle action state.

use super::{Unit, UnitState};
use crate::entity::Entity;

impl Unit {
    /// Return whether the retail idle action currently exists.
    #[must_use]
    pub fn has_idle_action(&self) -> bool {
        self.idle.is_active()
    }

    /// Return the elapsed duration of the current idle action in milliseconds.
    #[must_use]
    pub fn idle_duration(&self) -> u32 {
        self.idle.duration_ms()
    }

    pub(crate) fn reconcile_idle_action(&mut self, elapsed_ms: u32, parent_is_idle: bool) {
        let should_be_idle = self.is_alive()
            && !self.is_incapacitated()
            && self.state == UnitState::Idle
            && parent_is_idle;
        self.idle.reconcile(should_be_idle, elapsed_ms);
    }

    pub(crate) fn cancel_idle_action(&mut self) {
        self.idle.cancel();
    }
}
