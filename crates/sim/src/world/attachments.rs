//! Retail object attachments and their authoritative parent transforms.

use super::World;
use super::icons::IconObjectSpawn;
use crate::entities::Object;
use crate::entities::objects::is_icon_prototype;
use crate::entity_id::{EntityClass, EntityId};
use crate::spawn::find_object_by_id;
use glam::Vec3;
use pipeline::database::hw1::Database;

impl World {
    /// Create a prototype object and attach it to one live unit.
    ///
    /// Trigger effect 74 supplies no bone handles, so retail follows the
    /// parent's root transform. The returned child remains a normal class-zero
    /// entity and is therefore trigger-addressable.
    pub fn add_prototype_attachment_to_unit(
        &mut self,
        database: &Database,
        unit_id: EntityId,
        prototype_id: i32,
    ) -> Option<EntityId> {
        let (owner, position, forward) = self
            .units
            .get(unit_id)
            .map(|unit| (unit.base.player_id, unit.base.position, unit.base.forward))?;
        let prototype = find_object_by_id(database, prototype_id)?;
        if !prototype
            .object_class
            .as_deref()
            .is_none_or(|class| class.trim().eq_ignore_ascii_case("Object"))
        {
            return None;
        }
        let attachment_id = if is_icon_prototype(prototype) {
            self.insert_icon_object(
                prototype,
                &IconObjectSpawn {
                    player_id: owner,
                    prototype_id,
                    position,
                    forward,
                    color_override: None,
                    force_visible_to_all: false,
                },
            )
        } else {
            self.insert_visual_attachment(
                owner,
                prototype_id,
                prototype.name.clone(),
                position,
                forward,
            )
        };
        self.entity_object_state_mut(attachment_id)?
            .set_attached_to(Some(unit_id));
        self.entity_object_state_mut(unit_id)?
            .add_attachment(attachment_id);
        Some(attachment_id)
    }

    pub(crate) fn add_visual_attachment_to_unit(
        &mut self,
        unit_id: EntityId,
        prototype_id: i32,
        prototype_name: &str,
    ) -> Option<EntityId> {
        let prototype_name = prototype_name.trim();
        if prototype_name.is_empty() {
            return None;
        }
        let (owner, position, forward) = self
            .units
            .get(unit_id)
            .map(|unit| (unit.base.player_id, unit.base.position, unit.base.forward))?;
        let attachment_id = self.insert_visual_attachment(
            owner,
            prototype_id,
            prototype_name.to_owned(),
            position,
            forward,
        );
        self.entity_object_state_mut(attachment_id)?
            .set_attached_to(Some(unit_id));
        self.entity_object_state_mut(unit_id)?
            .add_attachment(attachment_id);
        Some(attachment_id)
    }

    /// Synchronize child root transforms after all parent motion for a substep.
    pub(super) fn synchronize_attachments(&mut self) {
        let snapshots = self.attachment_snapshots();
        for (parent_id, child_id, position, forward) in snapshots {
            if !self.set_attachment_transform(child_id, position, forward)
                && let Some(parent) = self.entity_object_state_mut(parent_id)
            {
                parent.remove_attachment(child_id);
            }
        }
    }

    pub(super) fn remove_owned_attachments(&mut self, parent_id: EntityId) {
        let attachments = self
            .entity_object_state_mut(parent_id)
            .map_or_else(Vec::new, crate::entities::ObjectState::take_attachments);
        for attachment_id in attachments {
            if let Some(attachment) = self.entity_object_state_mut(attachment_id) {
                attachment.set_attached_to(None);
            }
            let _removed = self.kill_entity(attachment_id, true);
        }
    }

    pub(super) fn detach_attachment_from_parent(&mut self, attachment_id: EntityId) {
        let parent_id = self
            .entity_object_state(attachment_id)
            .and_then(crate::entities::ObjectState::attached_to);
        if let Some(attachment) = self.entity_object_state_mut(attachment_id) {
            attachment.set_attached_to(None);
        }
        if let Some(parent_id) = parent_id
            && let Some(parent) = self.entity_object_state_mut(parent_id)
        {
            parent.remove_attachment(attachment_id);
        }
    }

    fn insert_visual_attachment(
        &mut self,
        owner: u8,
        prototype_id: i32,
        prototype_name: String,
        position: Vec3,
        forward: Vec3,
    ) -> EntityId {
        let id = self.objects.allocate_id();
        let object = Object::new_visual(id, owner, position, forward, prototype_id, prototype_name);
        self.objects.insert(id, object);
        id
    }

    fn attachment_snapshots(&self) -> Vec<(EntityId, EntityId, Vec3, Vec3)> {
        self.objects
            .iter()
            .map(|(id, object)| (id, &object.base, &object.object_state))
            .chain(
                self.units
                    .iter()
                    .map(|(id, unit)| (id, &unit.base, &unit.object_state)),
            )
            .chain(
                self.projectiles
                    .iter()
                    .map(|(id, projectile)| (id, &projectile.base, &projectile.object_state)),
            )
            .flat_map(|(parent_id, base, state)| {
                state
                    .attachments()
                    .iter()
                    .copied()
                    .map(move |child_id| (parent_id, child_id, base.position, base.forward))
            })
            .collect()
    }

    fn set_attachment_transform(
        &mut self,
        attachment_id: EntityId,
        position: Vec3,
        forward: Vec3,
    ) -> bool {
        match attachment_id.class() {
            Some(EntityClass::Object) => {
                self.objects.get_mut(attachment_id).is_some_and(|object| {
                    object.base.position = position;
                    object.base.set_forward(forward);
                    true
                })
            }
            Some(EntityClass::Unit) => self.units.get_mut(attachment_id).is_some_and(|unit| {
                unit.base.position = position;
                unit.base.set_forward(forward);
                true
            }),
            Some(EntityClass::Projectile) => {
                self.projectiles
                    .get_mut(attachment_id)
                    .is_some_and(|projectile| {
                        projectile.base.position = position;
                        projectile.base.set_forward(forward);
                        true
                    })
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::ProtoObject;

    fn database() -> Database {
        Database {
            objects: vec![ProtoObject {
                name: "sys_icon_27_01".to_owned(),
                dbid: Some(27),
                object_types: vec!["Icon".to_owned()],
                ..ProtoObject::default()
            }],
            ..Database::default()
        }
    }

    #[test]
    fn attachment_is_a_class_zero_child_that_follows_and_dies_with_parent() {
        let database = database();
        let mut world = World::new();
        world.init_players(1);
        let unit_id = world.create_unit_at(1, Vec3::new(1.0, 2.0, 3.0));
        let attachment_id = world
            .add_prototype_attachment_to_unit(&database, unit_id, 27)
            .unwrap();

        assert_eq!(attachment_id.class(), Some(EntityClass::Object));
        assert_eq!(
            world.entity_object_state(unit_id).unwrap().attachments(),
            &[attachment_id]
        );
        assert_eq!(
            world
                .entity_object_state(attachment_id)
                .unwrap()
                .attached_to(),
            Some(unit_id)
        );

        assert!(world.teleport_object(unit_id, Vec3::new(9.0, 8.0, 7.0)));
        world.synchronize_attachments();
        assert_eq!(
            world.get_object(attachment_id).unwrap().base.position,
            Vec3::new(9.0, 8.0, 7.0)
        );

        assert!(world.kill_unit(unit_id, true));
        assert!(world.get_object(attachment_id).is_none());
    }
}
