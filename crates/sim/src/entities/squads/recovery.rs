//! Squad recovery channels used by authored abilities and movement actions.

use crate::sync::SyncChecksum;

/// Retail recovery channel shared by actions that must not overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecoveryType {
    /// Movement recovery, such as a completed boost.
    Move = 0,
    /// Attack recovery, such as post-sprint weapon recovery.
    Attack = 1,
    /// The squad's command ability cooldown.
    Ability = 2,
}

/// Live recovery state on one squad.
#[derive(Debug, Clone, Default)]
pub struct SquadRecovery {
    recovery_type: Option<RecoveryType>,
    remaining: f32,
    ability_id: Option<u8>,
}

impl RecoveryType {
    pub(crate) fn from_authored(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("Move") {
            Some(Self::Move)
        } else if value.eq_ignore_ascii_case("Attack") {
            Some(Self::Attack)
        } else if value.eq_ignore_ascii_case("Ability") {
            Some(Self::Ability)
        } else {
            None
        }
    }
}

impl SquadRecovery {
    /// Return whether any recovery channel is active.
    #[must_use]
    pub const fn is_recovering(&self) -> bool {
        self.recovery_type.is_some()
    }

    /// Return the active retail recovery channel.
    #[must_use]
    pub const fn recovery_type(&self) -> Option<RecoveryType> {
        self.recovery_type
    }

    /// Return seconds remaining on the active recovery channel.
    #[must_use]
    pub const fn remaining(&self) -> f32 {
        self.remaining
    }

    /// Return the concrete ability whose execution started this recovery.
    #[must_use]
    pub const fn ability_id(&self) -> Option<u8> {
        self.ability_id
    }

    /// Return whether the active recovery blocks another action on `kind`.
    #[must_use]
    pub fn blocks(&self, kind: RecoveryType) -> bool {
        self.recovery_type == Some(kind)
    }

    pub(crate) fn start(&mut self, recovery_type: RecoveryType, time: f32, ability_id: Option<u8>) {
        if !time.is_finite() || time <= 0.0 {
            self.clear();
            return;
        }
        self.recovery_type = Some(recovery_type);
        self.remaining = time;
        self.ability_id = ability_id;
    }

    pub(crate) fn advance(&mut self, elapsed: f32) {
        if self.recovery_type.is_none() || !elapsed.is_finite() || elapsed <= 0.0 {
            return;
        }
        self.remaining -= elapsed;
        let expiry_tolerance = f32::EPSILON * elapsed.abs().max(1.0) * 8.0;
        if self.remaining <= expiry_tolerance {
            self.clear();
        }
    }

    pub(crate) fn clear(&mut self) {
        self.recovery_type = None;
        self.remaining = 0.0;
        self.ability_id = None;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.recovery_type.map_or(u32::MAX, |kind| kind as u32));
        checksum.hash_f32(self.remaining);
        checksum.hash_u32(self.ability_id.map_or(u32::MAX, u32::from));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_expires_on_its_authored_channel() {
        let mut recovery = SquadRecovery::default();
        recovery.start(RecoveryType::Ability, 2.0, Some(4));

        recovery.advance(0.75);
        assert!(recovery.blocks(RecoveryType::Ability));
        assert!((recovery.remaining() - 1.25).abs() < f32::EPSILON);
        assert_eq!(recovery.ability_id(), Some(4));

        recovery.advance(1.25);
        assert!(!recovery.is_recovering());
        assert!(recovery.remaining().abs() < f32::EPSILON);
    }
}
