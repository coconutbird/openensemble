//! Class-0 simulation objects.
//!
//! Retail stores invisible world-control objects such as revealers in the
//! `BObject` pool, separate from class-1 mobile units and buildings. Keeping
//! that pool distinct preserves trigger-visible object IDs without adding
//! presentation-only entities to the unit roster.

mod revealer;

pub use revealer::Revealer;

use super::{BaseEntity, ObjectState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use glam::Vec3;

/// Specialized behavior carried by a class-0 object.
#[derive(Debug, Clone)]
pub enum ObjectKind {
    /// A team-scoped fog-of-war revealer.
    Revealer(Revealer),
}

/// A trigger-addressable class-0 retail object.
#[derive(Debug, Clone)]
pub struct Object {
    /// Common entity state.
    pub base: BaseEntity,
    /// Runtime state inherited from retail `BObject`.
    pub object_state: ObjectState,
    /// Database proto-object ID, or `-1` when unresolved.
    pub proto_object_id: i32,
    /// Proto-object name retained for diagnostics and checksums.
    pub proto_object_name: String,
    /// Specialized object behavior.
    pub kind: ObjectKind,
}

impl Object {
    /// Construct an invisible revealer object.
    #[must_use]
    pub(crate) fn new_revealer(
        id: EntityId,
        owner: PlayerId,
        position: Vec3,
        proto_object_id: i32,
        proto_object_name: String,
        revealer: Revealer,
    ) -> Self {
        let mut base = BaseEntity::new(id, owner);
        base.set_position(position);
        base.set_selectable(false);
        base.configure_prototype_mobility(true);
        Self {
            base,
            object_state: ObjectState::default(),
            proto_object_id,
            proto_object_name,
            kind: ObjectKind::Revealer(revealer),
        }
    }

    /// Return revealer state when this object is a revealer.
    #[must_use]
    pub const fn revealer(&self) -> Option<&Revealer> {
        match &self.kind {
            ObjectKind::Revealer(revealer) => Some(revealer),
        }
    }
}

impl Entity for Object {
    fn id(&self) -> EntityId {
        self.base.id
    }

    fn update(&mut self, dt: f32) {
        match &mut self.kind {
            ObjectKind::Revealer(revealer) => revealer.update(dt),
        }
    }

    fn is_alive(&self) -> bool {
        self.base.is_alive()
    }
}

#[cfg(test)]
mod tests;
