//! Authoritative terrain and scenario-playable world bounds.

use super::World;
use glam::Vec3;

/// Axis-aligned bounds in the simulation's horizontal X/Z plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBounds {
    min_x: f32,
    min_z: f32,
    max_x: f32,
    max_z: f32,
}

impl WorldBounds {
    /// Build normalized horizontal bounds from two world-space corners.
    #[must_use]
    pub fn from_corners(first: Vec3, second: Vec3) -> Option<Self> {
        if !first.is_finite() || !second.is_finite() {
            return None;
        }
        Some(Self {
            min_x: first.x.min(second.x),
            min_z: first.z.min(second.z),
            max_x: first.x.max(second.x),
            max_z: first.z.max(second.z),
        })
    }

    /// Minimum world X coordinate, inclusive.
    #[must_use]
    pub const fn min_x(self) -> f32 {
        self.min_x
    }

    /// Minimum world Z coordinate, inclusive.
    #[must_use]
    pub const fn min_z(self) -> f32 {
        self.min_z
    }

    /// Maximum world X coordinate, inclusive.
    #[must_use]
    pub const fn max_x(self) -> f32 {
        self.max_x
    }

    /// Maximum world Z coordinate, inclusive.
    #[must_use]
    pub const fn max_z(self) -> f32 {
        self.max_z
    }

    /// Return whether a finite world point lies inside these inclusive bounds.
    #[must_use]
    pub fn contains(self, position: Vec3) -> bool {
        position.is_finite()
            && position.x >= self.min_x
            && position.z >= self.min_z
            && position.x <= self.max_x
            && position.z <= self.max_z
    }

    pub(super) fn clamp_to(self, limits: Self) -> Self {
        Self {
            min_x: self.min_x.clamp(limits.min_x, limits.max_x),
            min_z: self.min_z.clamp(limits.min_z, limits.max_z),
            max_x: self.max_x.clamp(limits.min_x, limits.max_x),
            max_z: self.max_z.clamp(limits.min_z, limits.max_z),
        }
    }

    pub(super) fn hash_state(self, checksum: &mut crate::sync::SyncChecksum) {
        checksum.hash_f32(self.min_x);
        checksum.hash_f32(self.min_z);
        checksum.hash_f32(self.max_x);
        checksum.hash_f32(self.max_z);
    }
}

impl World {
    /// Configure the full terrain extent loaded for the current scenario.
    ///
    /// Any playable bounds authored before the terrain was available are
    /// clamped into this extent, matching retail `BWorld::setSimBounds`.
    pub fn configure_terrain_bounds(&mut self, first: Vec3, second: Vec3) -> bool {
        let Some(terrain_bounds) = WorldBounds::from_corners(first, second) else {
            return false;
        };
        self.terrain_bounds = Some(terrain_bounds);
        self.playable_bounds = self
            .playable_bounds
            .map(|bounds| bounds.clamp_to(terrain_bounds))
            .filter(|bounds| *bounds != terrain_bounds);
        true
    }

    /// Return the full terrain extent supplied by the active scenario XTD.
    #[must_use]
    pub const fn terrain_bounds(&self) -> Option<WorldBounds> {
        self.terrain_bounds
    }

    /// Return the active scenario-playable subset of the terrain.
    ///
    /// `None` means gameplay uses the full terrain extent.
    #[must_use]
    pub const fn playable_bounds(&self) -> Option<WorldBounds> {
        self.playable_bounds
    }

    /// Return the effective bounds consumed by renderer visibility projection.
    #[must_use]
    pub const fn effective_playable_bounds(&self) -> Option<WorldBounds> {
        match self.playable_bounds {
            Some(bounds) => Some(bounds),
            None => self.terrain_bounds,
        }
    }

    /// Set scenario-playable bounds from two unordered world-space corners.
    pub fn set_playable_bounds(&mut self, first: Vec3, second: Vec3) -> bool {
        let Some(mut bounds) = WorldBounds::from_corners(first, second) else {
            return false;
        };
        if let Some(terrain_bounds) = self.terrain_bounds {
            bounds = bounds.clamp_to(terrain_bounds);
            self.playable_bounds = (bounds != terrain_bounds).then_some(bounds);
        } else {
            self.playable_bounds = Some(bounds);
        }
        true
    }

    /// Match retail's playable-bounds query for one world position.
    ///
    /// When no scenario subset is active, full terrain boundaries are checked
    /// only when `force_terrain_boundaries` is true.
    #[must_use]
    pub fn is_outside_playable_bounds(
        &self,
        position: Vec3,
        force_terrain_boundaries: bool,
    ) -> bool {
        if let Some(bounds) = self.playable_bounds {
            return !bounds.contains(position);
        }
        force_terrain_boundaries
            && self
                .terrain_bounds
                .is_some_and(|bounds| !bounds.contains(position))
    }
}

#[cfg(test)]
mod tests;
