//! Compact authored switches and their public queries.

use super::{ParticleEmitterTiming, ParticleForceDefinition};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ParticleTimingFlags(u16);

impl ParticleTimingFlags {
    pub(super) const TIED_TO_EMITTER: u16 = 1 << 0;
    pub(super) const IGNORE_ROTATION: u16 = 1 << 1;
    pub(super) const LOOPING: u16 = 1 << 2;
    pub(super) const ALWAYS_ACTIVE: u16 = 1 << 3;
    pub(super) const ALWAYS_RENDER: u16 = 1 << 4;
    pub(super) const KILL_IMMEDIATELY_ON_RELEASE: u16 = 1 << 5;
    pub(super) const BEAM_COLOR_BY_LENGTH: u16 = 1 << 6;
    pub(super) const BEAM_OPACITY_BY_LENGTH: u16 = 1 << 7;
    pub(super) const BEAM_INTENSITY_BY_LENGTH: u16 = 1 << 8;
    pub(super) const COLLISION_DETECTION_TERRAIN: u16 = 1 << 9;
    pub(super) const FILL_OPTIMIZED: u16 = 1 << 10;

    pub(super) fn has(self, flag: u16) -> bool {
        self.0 & flag != 0
    }

    pub(super) fn set(&mut self, flag: u16, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

impl ParticleEmitterTiming {
    /// Returns whether particles remain in emitter-local space.
    #[must_use]
    pub fn tied_to_emitter(&self) -> bool {
        self.flags.has(ParticleTimingFlags::TIED_TO_EMITTER)
    }

    /// Returns whether the emitter strips authored transform rotation.
    #[must_use]
    pub fn ignore_rotation(&self) -> bool {
        self.flags.has(ParticleTimingFlags::IGNORE_ROTATION)
    }

    /// Returns whether active and dormant intervals repeat.
    #[must_use]
    pub fn looping(&self) -> bool {
        self.flags.has(ParticleTimingFlags::LOOPING)
    }

    /// Returns whether visibility culling may pause updates.
    #[must_use]
    pub fn always_active(&self) -> bool {
        self.flags.has(ParticleTimingFlags::ALWAYS_ACTIVE)
    }

    /// Returns whether visibility culling may suppress drawing.
    #[must_use]
    pub fn always_render(&self) -> bool {
        self.flags.has(ParticleTimingFlags::ALWAYS_RENDER)
    }

    /// Returns whether releasing the parent immediately kills nested effects.
    #[must_use]
    pub fn kill_immediately_on_release(&self) -> bool {
        self.flags
            .has(ParticleTimingFlags::KILL_IMMEDIATELY_ON_RELEASE)
    }

    /// Returns whether beam color progression uses distance.
    #[must_use]
    pub fn beam_color_by_length(&self) -> bool {
        self.flags.has(ParticleTimingFlags::BEAM_COLOR_BY_LENGTH)
    }

    /// Returns whether beam opacity progression uses distance.
    #[must_use]
    pub fn beam_opacity_by_length(&self) -> bool {
        self.flags.has(ParticleTimingFlags::BEAM_OPACITY_BY_LENGTH)
    }

    /// Returns whether beam intensity progression uses distance.
    #[must_use]
    pub fn beam_intensity_by_length(&self) -> bool {
        self.flags
            .has(ParticleTimingFlags::BEAM_INTENSITY_BY_LENGTH)
    }

    /// Returns whether particles collide with the terrain heightfield.
    #[must_use]
    pub fn collision_detection_terrain(&self) -> bool {
        self.flags
            .has(ParticleTimingFlags::COLLISION_DETECTION_TERRAIN)
    }

    /// Returns whether the authored half-resolution fill path is requested.
    #[must_use]
    pub fn fill_optimized(&self) -> bool {
        self.flags.has(ParticleTimingFlags::FILL_OPTIMIZED)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ParticleForceFlags(u8);

impl ParticleForceFlags {
    pub(super) const RANDOM_ORIENTATION: u8 = 1 << 0;
    pub(super) const TUMBLE: u8 = 1 << 1;
    pub(super) const TUMBLE_BOTH_DIRECTIONS: u8 = 1 << 2;
    pub(super) const USE_INTERNAL_GRAVITY: u8 = 1 << 3;
    pub(super) const USE_INTERNAL_WIND: u8 = 1 << 4;

    pub(super) fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    pub(super) fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

impl ParticleForceDefinition {
    /// Returns whether initial billboard rotation is randomized.
    #[must_use]
    pub fn random_orientation(&self) -> bool {
        self.flags.has(ParticleForceFlags::RANDOM_ORIENTATION)
    }

    /// Returns whether angular tumble is advanced each update.
    #[must_use]
    pub fn tumble(&self) -> bool {
        self.flags.has(ParticleForceFlags::TUMBLE)
    }

    /// Returns whether tumble may choose either rotation direction.
    #[must_use]
    pub fn tumble_both_directions(&self) -> bool {
        self.flags.has(ParticleForceFlags::TUMBLE_BOTH_DIRECTIONS)
    }

    /// Returns whether internal vertical gravity is active.
    #[must_use]
    pub fn use_internal_gravity(&self) -> bool {
        self.flags.has(ParticleForceFlags::USE_INTERNAL_GRAVITY)
    }

    /// Returns whether authored internal wind is active.
    #[must_use]
    pub fn use_internal_wind(&self) -> bool {
        self.flags.has(ParticleForceFlags::USE_INTERNAL_WIND)
    }
}
