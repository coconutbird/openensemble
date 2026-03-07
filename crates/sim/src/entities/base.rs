//! Base entity type with common fields.
//!
//! Based on BEntity from the original source.

use crate::entity_id::EntityId;
use crate::player::PlayerId;
use glam::Vec3;

/// Base entity data shared by all entity types.
///
/// Based on BEntity fields from entity.h:
/// - mPosition (BVector) - 16 bytes
/// - mForward (BVector) - 16 bytes
/// - mVelocity (BVector) - 16 bytes
/// - mID (BEntityID) - 4 bytes
/// - mPlayerID (BPlayerID) - 4 bytes
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
        }
    }
}

impl BaseEntity {
    /// Create a new base entity with the given ID and player.
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
    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Kill the entity.
    pub fn kill(&mut self) {
        self.alive = false;
    }
}
