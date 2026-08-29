//! Creation and visibility of authoritative class-zero icon objects.

use super::World;
use crate::entities::objects::is_icon_prototype;
use crate::entities::{IconObject, Object};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use crate::spawn::find_object_by_id;
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

pub(super) struct IconObjectSpawn {
    pub(super) player_id: PlayerId,
    pub(super) prototype_id: i32,
    pub(super) position: Vec3,
    pub(super) forward: Vec3,
    pub(super) color_override: Option<[u8; 3]>,
    pub(super) force_visible_to_all: bool,
}

impl World {
    /// Create a retail class-zero icon from the scenario-layered database.
    ///
    /// `color_override` mirrors `BObject::setIconColor`, which changes RGB but
    /// leaves alpha and the prototype's minimap icon artwork untouched.
    pub fn create_icon_object(
        &mut self,
        database: &Database,
        player_id: PlayerId,
        prototype_id: i32,
        position: Vec3,
        color_override: Option<[u8; 3]>,
        force_visible_to_all: bool,
    ) -> Option<EntityId> {
        if self.get_player(player_id).is_none() || !position.is_finite() {
            return None;
        }
        let prototype = find_object_by_id(database, prototype_id)?;
        if !is_icon_prototype(prototype) {
            return None;
        }
        Some(self.insert_icon_object(
            prototype,
            &IconObjectSpawn {
                player_id,
                prototype_id,
                position,
                forward: Vec3::Z,
                color_override,
                force_visible_to_all,
            },
        ))
    }

    /// Iterate active icon objects in deterministic class-zero pool order.
    pub fn icon_objects(&self) -> impl Iterator<Item = (EntityId, &IconObject)> {
        self.objects
            .iter()
            .filter_map(|(id, object)| object.icon().map(|icon| (id, icon)))
    }

    /// Return whether retail minimap rules expose one icon to a local player.
    #[must_use]
    pub fn is_icon_visible_to_player(&self, icon_id: EntityId, viewer_id: PlayerId) -> bool {
        let Some(object) = self.get_object(icon_id).filter(|object| object.is_alive()) else {
            return false;
        };
        let Some(icon) = object.icon().copied() else {
            return false;
        };
        let Some(viewer_team) = self.get_player(viewer_id).map(|player| player.team_id) else {
            return false;
        };
        if icon.always_visible_on_minimap() {
            return true;
        }
        let same_team = self
            .get_player(object.base.player_id)
            .is_some_and(|owner| owner.team_id == viewer_team);
        if icon.visible_for_owner_only() || icon.visible_for_team_only() {
            return same_team;
        }
        icon.visible_to_all() || self.is_entity_visible_to_team(viewer_team, icon_id)
    }

    pub(super) fn insert_icon_object(
        &mut self,
        prototype: &ProtoObject,
        spawn: &IconObjectSpawn,
    ) -> EntityId {
        let icon =
            IconObject::from_prototype(prototype, spawn.color_override, spawn.force_visible_to_all);
        let id = self.objects.allocate_id();
        let object = Object::new_icon(
            id,
            spawn.player_id,
            spawn.position,
            spawn.forward,
            spawn.prototype_id,
            prototype.name.clone(),
            icon,
        );
        self.objects.insert(id, object);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon_database() -> Database {
        Database {
            objects: vec![ProtoObject {
                name: "sys_icon_test".to_owned(),
                dbid: Some(42),
                object_class: Some("Object".to_owned()),
                object_types: vec!["Icon".to_owned()],
                flags: vec!["VisibleForOwnerOnly".to_owned()],
                ..ProtoObject::default()
            }],
            ..Database::default()
        }
    }

    #[test]
    fn icon_creation_retains_runtime_color_and_retail_visibility() {
        let database = icon_database();
        let mut world = World::new();
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;

        let icon_id = world
            .create_icon_object(
                &database,
                1,
                42,
                Vec3::new(10.0, 0.0, 20.0),
                Some([255, 128, 0]),
                false,
            )
            .unwrap();
        let icon = world.get_object(icon_id).unwrap().icon().copied().unwrap();
        assert_eq!(icon.color_override(), Some([255, 128, 0]));
        assert!(world.is_icon_visible_to_player(icon_id, 1));
        assert!(!world.is_icon_visible_to_player(icon_id, 2));

        let public_id = world
            .create_icon_object(&database, 1, 42, Vec3::ZERO, None, true)
            .unwrap();
        assert!(world.is_icon_visible_to_player(public_id, 2));
    }
}
