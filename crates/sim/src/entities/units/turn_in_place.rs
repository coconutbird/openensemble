//! Retail turn-in-place flags shared by sim orientation and animation projection.

use super::Unit;
use crate::sync::SyncChecksum;

/// Direction selected for an active retail turn-in-place animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnInPlaceDirection {
    /// Counter-clockwise turn animation.
    Left,
    /// Clockwise turn animation.
    Right,
}

/// `mFlagTurning` plus the persistent `mFlagTurningRight` bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum UnitTurnInPlace {
    #[default]
    Disabled,
    IdleLeft,
    IdleRight,
    TurningLeft,
    TurningRight,
}

impl UnitTurnInPlace {
    fn configure(&mut self, enabled: bool) {
        *self = if enabled {
            Self::IdleLeft
        } else {
            Self::Disabled
        };
    }

    fn set_turning(&mut self, turning: bool) {
        *self = match (*self, turning) {
            (Self::Disabled, _) => Self::Disabled,
            (Self::IdleRight | Self::TurningRight, true) => Self::TurningRight,
            (Self::IdleRight | Self::TurningRight, false) => Self::IdleRight,
            (Self::IdleLeft | Self::TurningLeft, true) => Self::TurningLeft,
            (Self::IdleLeft | Self::TurningLeft, false) => Self::IdleLeft,
        };
    }

    fn set_turning_right(&mut self, right: bool) {
        *self = match (*self, right) {
            (Self::Disabled, _) => Self::Disabled,
            (Self::TurningLeft | Self::TurningRight, true) => Self::TurningRight,
            (Self::TurningLeft | Self::TurningRight, false) => Self::TurningLeft,
            (Self::IdleLeft | Self::IdleRight, true) => Self::IdleRight,
            (Self::IdleLeft | Self::IdleRight, false) => Self::IdleLeft,
        };
    }

    const fn direction(self) -> Option<TurnInPlaceDirection> {
        match self {
            Self::TurningLeft => Some(TurnInPlaceDirection::Left),
            Self::TurningRight => Some(TurnInPlaceDirection::Right),
            Self::Disabled | Self::IdleLeft | Self::IdleRight => None,
        }
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self as u32);
    }
}

impl Unit {
    pub(crate) fn configure_turn_in_place(&mut self, enabled: bool) {
        self.turn_in_place.configure(enabled);
    }

    pub(crate) fn set_turning_in_place(&mut self, turning: bool) {
        self.turn_in_place.set_turning(turning);
    }

    pub(crate) fn set_turning_in_place_right(&mut self, right: bool) {
        self.turn_in_place.set_turning_right(right);
    }

    /// Return the active retail turn-in-place animation direction.
    #[must_use]
    pub const fn turn_in_place_direction(&self) -> Option<TurnInPlaceDirection> {
        self.turn_in_place.direction()
    }

    pub(crate) fn hash_turn_in_place_state(&self, checksum: &mut SyncChecksum) {
        self.turn_in_place.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_retains_the_source_turning_right_bit() {
        let mut unit = Unit::default();
        unit.configure_turn_in_place(true);
        unit.set_turning_in_place_right(true);
        unit.set_turning_in_place(true);
        assert_eq!(
            unit.turn_in_place_direction(),
            Some(TurnInPlaceDirection::Right)
        );

        unit.set_turning_in_place(false);
        unit.set_turning_in_place(true);
        assert_eq!(
            unit.turn_in_place_direction(),
            Some(TurnInPlaceDirection::Right)
        );
    }
}
