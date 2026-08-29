//! Projectile-pool access and lifecycle integration.

mod collision;

use super::World;
use crate::entities::Projectile;
use crate::entity_id::EntityId;

impl World {
    /// Get a live projectile by its current generational ID.
    #[must_use]
    pub fn get_projectile(&self, id: EntityId) -> Option<&Projectile> {
        self.projectiles.get(id)
    }

    /// Mutably get a live projectile.
    pub fn get_projectile_mut(&mut self, id: EntityId) -> Option<&mut Projectile> {
        self.projectiles.get_mut(id)
    }

    /// Remove a projectile and invalidate its entity ID.
    pub fn remove_projectile(&mut self, id: EntityId) -> Option<Projectile> {
        self.projectiles.get(id)?;
        self.remove_owned_attachments(id);
        self.detach_attachment_from_parent(id);
        self.projectiles.remove(id)
    }
}
