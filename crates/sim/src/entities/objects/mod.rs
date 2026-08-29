//! Class-0 simulation objects.
//!
//! Retail stores invisible world-control objects such as revealers in the
//! `BObject` pool, separate from class-1 mobile units and buildings. Keeping
//! that pool distinct preserves trigger-visible object IDs without adding
//! presentation-only entities to the unit roster.

mod icon;
mod revealer;

pub use icon::IconObject;
pub(crate) use icon::is_icon_prototype;
pub use revealer::Revealer;

use super::{BaseEntity, ObjectState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use glam::Vec3;

/// Specialized behavior carried by a class-0 object.
#[derive(Debug, Clone)]
pub enum ObjectKind {
    /// A placed class-0 object with an authored visual representation.
    Visual,
    /// A minimap icon whose visibility and color are owned by the simulation.
    Icon(IconObject),
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
    /// Construct a placed visual object from the scenario database.
    #[must_use]
    pub(crate) fn new_visual(
        id: EntityId,
        owner: PlayerId,
        position: Vec3,
        forward: Vec3,
        proto_object_id: i32,
        proto_object_name: String,
    ) -> Self {
        let mut base = BaseEntity::new(id, owner);
        base.set_position(position);
        base.set_forward(forward);
        base.set_selectable(false);
        base.configure_prototype_mobility(false);
        Self {
            base,
            object_state: ObjectState::default(),
            proto_object_id,
            proto_object_name,
            kind: ObjectKind::Visual,
        }
    }

    /// Construct a class-zero minimap icon object.
    #[must_use]
    pub(crate) fn new_icon(
        id: EntityId,
        owner: PlayerId,
        position: Vec3,
        forward: Vec3,
        proto_object_id: i32,
        proto_object_name: String,
        icon: IconObject,
    ) -> Self {
        let mut base = BaseEntity::new(id, owner);
        base.set_position(position);
        base.set_forward(forward);
        base.set_selectable(false);
        base.configure_prototype_mobility(false);
        Self {
            base,
            object_state: ObjectState::default(),
            proto_object_id,
            proto_object_name,
            kind: ObjectKind::Icon(icon),
        }
    }

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
            ObjectKind::Visual | ObjectKind::Icon(_) => None,
            ObjectKind::Revealer(revealer) => Some(revealer),
        }
    }

    /// Return icon-specific runtime state when this object is a minimap icon.
    #[must_use]
    pub const fn icon(&self) -> Option<&IconObject> {
        match &self.kind {
            ObjectKind::Icon(icon) => Some(icon),
            ObjectKind::Visual | ObjectKind::Revealer(_) => None,
        }
    }

    /// Whether this class-0 object has a renderer-facing database visual.
    #[must_use]
    pub const fn is_visual(&self) -> bool {
        matches!(self.kind, ObjectKind::Visual)
    }
}

impl Entity for Object {
    fn id(&self) -> EntityId {
        self.base.id
    }

    fn update(&mut self, dt: f32) {
        match &mut self.kind {
            ObjectKind::Visual | ObjectKind::Icon(_) => {}
            ObjectKind::Revealer(revealer) => revealer.update(dt),
        }
    }

    fn is_alive(&self) -> bool {
        self.base.is_alive()
    }
}

#[cfg(test)]
mod tests;
