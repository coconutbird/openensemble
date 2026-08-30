//! Unit targetability and simulation-bounds queries.

use super::Unit;
use crate::entity::Entity;
use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ExternalShieldState {
    #[default]
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum InvulnerabilityState {
    #[default]
    Vulnerable,
    Invulnerable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum BoardingState {
    #[default]
    Free,
    BeingBoarded,
}

impl Unit {
    /// Check whether this unit can perform completed-unit gameplay actions.
    #[must_use]
    pub fn is_operational(&self) -> bool {
        self.is_alive()
            && !self.is_incapacitated()
            && !self.is_garrisoned()
            && !self.is_thrown()
            && !self.is_undergoing_infection()
            && !self.is_crashing()
            && (!self.is_building() || self.built)
    }

    /// Return whether combat may currently target and damage this unit.
    #[must_use]
    pub fn is_attackable(&self) -> bool {
        self.is_alive()
            && !self.is_incapacitated()
            && !self.is_garrisoned()
            && !self.is_invulnerable()
            && !self.is_being_boarded()
            && !self.is_jump_pull_untargetable()
            && !self.is_jumping()
            && !self.is_air_crash_untargetable()
            && !self.is_undergoing_infection()
    }

    /// Return whether automatic combat acquisition may target this object.
    #[must_use]
    pub fn is_auto_attackable(&self) -> bool {
        self.auto_attackable && self.is_attackable()
    }

    /// Return whether this prototype owns a retail external-shield volume.
    #[must_use]
    pub const fn is_external_shield(&self) -> bool {
        matches!(self.external_shield, ExternalShieldState::Enabled)
    }

    /// Return retail's synchronized simulation-bounding-box center.
    #[must_use]
    pub(crate) fn simulation_center(&self) -> Vec3 {
        if self.flying {
            self.base.position
        } else {
            self.base.position + Vec3::Y * self.obstruction_half_extents.y.abs()
        }
    }

    /// Return the horizontal obstruction radius used by retail range math.
    #[must_use]
    pub(crate) fn obstruction_radius(&self) -> f32 {
        self.obstruction_half_extents
            .x
            .abs()
            .max(self.obstruction_half_extents.z.abs())
    }

    /// Return the authoritative axis-aligned bounds used by sim-space queries.
    pub(crate) fn simulation_bounds(&self) -> (Vec3, Vec3) {
        (
            self.simulation_center(),
            self.obstruction_half_extents.abs(),
        )
    }

    pub(crate) const fn auto_attackable_setting(&self) -> bool {
        self.auto_attackable
    }

    pub(crate) fn set_auto_attackable(&mut self, auto_attackable: bool) {
        self.auto_attackable = auto_attackable;
    }

    /// Return whether a persistent retail action currently prevents damage.
    #[must_use]
    pub const fn is_invulnerable(&self) -> bool {
        matches!(self.invulnerability, InvulnerabilityState::Invulnerable)
    }

    pub(crate) fn set_invulnerable(&mut self, invulnerable: bool) {
        self.invulnerability = if invulnerable {
            InvulnerabilityState::Invulnerable
        } else {
            InvulnerabilityState::Vulnerable
        };
    }

    /// Return whether a timed Board action currently owns this target.
    #[must_use]
    pub const fn is_being_boarded(&self) -> bool {
        matches!(self.boarding_state, BoardingState::BeingBoarded)
    }

    pub(crate) fn set_being_boarded(&mut self, being_boarded: bool) {
        self.boarding_state = if being_boarded {
            BoardingState::BeingBoarded
        } else {
            BoardingState::Free
        };
    }

    pub(crate) fn set_external_shield(&mut self, external_shield: bool) {
        self.external_shield = if external_shield {
            ExternalShieldState::Enabled
        } else {
            ExternalShieldState::Disabled
        };
    }
}
