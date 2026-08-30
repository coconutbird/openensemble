//! Retail squad leash and aircraft collision-avoidance anchor state.

use super::Squad;
use crate::sync::SyncChecksum;
use glam::Vec3;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct SquadLeash {
    position: Vec3,
    anchor: Vec3,
    deadzone: f32,
    recall_delay_ms: u32,
}

impl Squad {
    /// Return the current AI leash position.
    #[must_use]
    pub const fn leash_position(&self) -> Vec3 {
        self.leash.position
    }

    /// Return the collision-avoidance anchor retained behind a dragged leash.
    #[must_use]
    pub const fn anchor_position(&self) -> Vec3 {
        self.leash.anchor
    }

    /// Return the authored leash deadzone.
    #[must_use]
    pub const fn leash_deadzone(&self) -> f32 {
        self.leash.deadzone
    }

    /// Return the authored idle recall delay in milliseconds.
    #[must_use]
    pub const fn leash_recall_delay_ms(&self) -> u32 {
        self.leash.recall_delay_ms
    }

    pub(crate) fn configure_leash_profile(&mut self, deadzone: f32, recall_delay_ms: u32) {
        self.leash.deadzone = deadzone.max(0.0);
        self.leash.recall_delay_ms = recall_delay_ms;
    }

    pub(crate) fn set_leash_position(&mut self, position: Vec3, set_anchor: bool) {
        if !position.is_finite() {
            return;
        }
        self.leash.position = position;
        if set_anchor {
            self.leash.anchor = position;
        }
    }

    pub(crate) fn initialize_air_anchor(&mut self) {
        if self.leash.anchor == Vec3::ZERO {
            self.leash.anchor = self.leash.position;
        }
    }

    pub(crate) fn hash_leash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_vec3(
            self.leash.position.x,
            self.leash.position.y,
            self.leash.position.z,
        );
        checksum.hash_vec3(
            self.leash.anchor.x,
            self.leash.anchor.y,
            self.leash.anchor.z,
        );
        checksum.hash_f32(self.leash.deadzone);
        checksum.hash_u32(self.leash.recall_delay_ms);
    }
}
