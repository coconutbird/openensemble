//! Base entity type with common fields.
//!
//! Based on `BEntity` from the original source.

use crate::entity_id::EntityId;
use crate::player::PlayerId;
use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EntityFlags(u8);

impl EntityFlags {
    const SELECTABLE: u8 = 1 << 0;
    const NON_MOBILE: u8 = 1 << 1;
    const PROTOTYPE_NON_MOBILE: u8 = 1 << 2;

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

impl Default for EntityFlags {
    fn default() -> Self {
        Self(Self::SELECTABLE)
    }
}

/// Base entity data shared by all entity types.
///
/// Based on `BEntity` fields from entity.h:
/// - mPosition (`BVector`) - 16 bytes
/// - mForward (`BVector`) - 16 bytes
/// - mVelocity (`BVector`) - 16 bytes
/// - mID (`BEntityID`) - 4 bytes
/// - mPlayerID (`BPlayerID`) - 4 bytes
#[derive(Debug, Clone)]
pub struct BaseEntity {
    /// Entity ID.
    pub id: EntityId,
    /// Owning player ID.
    pub player_id: PlayerId,
    /// Position in world space.
    pub position: Vec3,
    /// Forward direction (normalized).
    pub forward: Vec3,
    /// Current velocity.
    pub velocity: Vec3,
    /// Whether the entity is alive/valid.
    pub alive: bool,
    /// Retail selectable and mobility bit state.
    flags: EntityFlags,
}

impl Default for BaseEntity {
    fn default() -> Self {
        Self {
            id: EntityId::INVALID,
            player_id: 0,
            position: Vec3::ZERO,
            forward: Vec3::Z, // Default facing +Z
            velocity: Vec3::ZERO,
            alive: true,
            flags: EntityFlags::default(),
        }
    }
}

impl BaseEntity {
    /// Create a new base entity with the given ID and player.
    #[must_use]
    pub fn new(id: EntityId, player_id: PlayerId) -> Self {
        Self {
            id,
            player_id,
            ..Default::default()
        }
    }

    /// Set position.
    pub fn set_position(&mut self, pos: Vec3) {
        self.position = pos;
    }

    /// Set forward direction (will be normalized).
    pub fn set_forward(&mut self, forward: Vec3) {
        self.forward = forward.normalize_or_zero();
        if self.forward == Vec3::ZERO {
            self.forward = Vec3::Z;
        }
    }

    /// Look at a target position.
    pub fn look_at(&mut self, target: Vec3) {
        let dir = target - self.position;
        if dir.length_squared() > 0.0001 {
            self.forward = dir.normalize();
        }
    }

    /// Check if entity is alive.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Kill the entity.
    pub fn kill(&mut self) {
        self.alive = false;
    }

    /// Return the live retail selectable override.
    #[must_use]
    pub const fn is_selectable(&self) -> bool {
        self.flags.contains(EntityFlags::SELECTABLE)
    }

    /// Change the live retail selectable override.
    pub(crate) fn set_selectable(&mut self, selectable: bool) {
        self.flags.set(EntityFlags::SELECTABLE, selectable);
    }

    /// Return whether movement is currently enabled.
    #[must_use]
    pub const fn is_mobile(&self) -> bool {
        !self.flags.contains(EntityFlags::NON_MOBILE)
    }

    /// Return whether the immutable prototype permits mobility overrides.
    #[must_use]
    pub const fn is_ever_mobile(&self) -> bool {
        !self.flags.contains(EntityFlags::PROTOTYPE_NON_MOBILE)
    }

    /// Initialize live and immutable mobility from the selected prototype.
    pub(crate) fn configure_prototype_mobility(&mut self, non_mobile: bool) {
        self.flags
            .set(EntityFlags::PROTOTYPE_NON_MOBILE, non_mobile);
        self.flags.set(EntityFlags::NON_MOBILE, non_mobile);
    }

    /// Apply the live mobility override when the prototype permits it.
    pub(crate) fn set_mobile(&mut self, mobile: bool) -> bool {
        if !self.is_ever_mobile() {
            return false;
        }
        self.flags.set(EntityFlags::NON_MOBILE, !mobile);
        true
    }
}
