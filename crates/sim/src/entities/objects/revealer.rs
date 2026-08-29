//! Authoritative state for retail fog-of-war revealers.

use crate::player::TeamId;
use glam::Vec3;

const REVEAL_FRACTION_PER_SECOND: f32 = 3.33;

/// Team-scoped reveal behavior carried by a class-0 object.
#[derive(Debug, Clone)]
pub struct Revealer {
    team_id: TeamId,
    los_scalar: f32,
    prototype_line_of_sight: f32,
    reveal_fraction: f32,
    lifespan_expiration_ms: Option<u32>,
}

impl Revealer {
    /// Retail sentinel for a revealer that exposes the entire map.
    pub const GLOBAL_LINE_OF_SIGHT: f32 = -1.0;

    #[must_use]
    pub(crate) const fn new(
        team_id: TeamId,
        los_scalar: f32,
        prototype_line_of_sight: f32,
        lifespan_expiration_ms: Option<u32>,
    ) -> Self {
        Self {
            team_id,
            los_scalar,
            prototype_line_of_sight,
            reveal_fraction: 0.0,
            lifespan_expiration_ms,
        }
    }

    /// Team whose current visibility map this object contributes to.
    #[must_use]
    pub const fn team_id(&self) -> TeamId {
        self.team_id
    }

    /// Trigger-authored LOS scalar after the retail minimum-size clamp.
    #[must_use]
    pub const fn line_of_sight_scalar(&self) -> f32 {
        self.los_scalar
    }

    /// Current gameplay LOS radius, or `-1` for the global revealer.
    #[must_use]
    pub fn line_of_sight(&self) -> f32 {
        if self.los_scalar == Self::GLOBAL_LINE_OF_SIGHT {
            Self::GLOBAL_LINE_OF_SIGHT
        } else {
            self.prototype_line_of_sight * self.los_scalar
        }
    }

    /// Renderer-facing minimap expansion fraction.
    ///
    /// Retail gameplay LOS is active immediately; only presentation expands
    /// over roughly 0.3 seconds.
    #[must_use]
    pub const fn reveal_fraction(&self) -> f32 {
        self.reveal_fraction
    }

    /// Absolute world time at which this object expires, when timed.
    #[must_use]
    pub const fn lifespan_expiration_ms(&self) -> Option<u32> {
        self.lifespan_expiration_ms
    }

    #[must_use]
    pub(crate) fn covers(&self, origin: Vec3, position: Vec3) -> bool {
        let line_of_sight = self.line_of_sight();
        if line_of_sight == Self::GLOBAL_LINE_OF_SIGHT {
            return true;
        }
        if !line_of_sight.is_finite() || line_of_sight <= 0.0 {
            return line_of_sight.is_infinite() && line_of_sight.is_sign_positive();
        }
        let delta = position - origin;
        delta.x.mul_add(delta.x, delta.z * delta.z) <= line_of_sight * line_of_sight
    }

    #[must_use]
    pub(crate) fn is_expired(&self, game_time_ms: u32) -> bool {
        self.lifespan_expiration_ms
            .is_some_and(|expiration| game_time_ms >= expiration)
    }

    pub(crate) fn update(&mut self, dt: f32) {
        if dt.is_finite() && dt > 0.0 {
            self.reveal_fraction =
                (self.reveal_fraction + dt * REVEAL_FRACTION_PER_SECOND).min(1.0);
        }
    }
}
