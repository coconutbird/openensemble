//! Squad-level shield recharge scheduling.

use crate::sync::SyncChecksum;

/// Damage timer shared by all shield-bearing members of a squad.
#[derive(Debug, Clone, Copy, Default)]
pub struct SquadShields {
    recharge_requested: bool,
    seconds_since_damage: Option<f32>,
}

impl SquadShields {
    /// Request birth/technology recharge without resetting the damage clock.
    pub(crate) fn request_recharge(&mut self) {
        self.recharge_requested = true;
    }

    pub(crate) fn clear_recharge_request(&mut self) {
        self.recharge_requested = false;
    }

    /// Record retail's squad `Damaged` notification.
    pub(crate) fn notify_damaged(&mut self) {
        self.recharge_requested = true;
        self.seconds_since_damage = Some(0.0);
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

    pub(crate) fn advance_damage_clock(&mut self, dt: f32) {
        if let Some(elapsed) = &mut self.seconds_since_damage {
            *elapsed += dt;
        }
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.recharge_requested));
        checksum.hash_f32(self.seconds_since_damage.unwrap_or(-1.0));
    }
}
