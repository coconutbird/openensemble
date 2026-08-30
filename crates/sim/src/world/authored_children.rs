//! Retail authored-object relationships and prototype-controlled lifetime.

use super::World;
use crate::entities::units::AuthoredUnitChildKind;
use crate::entity::Entity;
use crate::entity_id::{EntityClass, EntityId};

impl World {
    pub(crate) fn associate_authored_child(
        &mut self,
        parent_id: EntityId,
        child_id: EntityId,
        kind: AuthoredUnitChildKind,
    ) -> bool {
        if parent_id == child_id
            || !self.get_unit(parent_id).is_some_and(Entity::is_alive)
            || !self.authored_child_is_alive(child_id, kind)
        {
            return false;
        }
        let old_parent_id = self.authored_child_parent(child_id);
        if let Some(old_parent_id) = old_parent_id
            && old_parent_id != parent_id
            && let Some(old_parent) = self.get_unit_mut(old_parent_id)
        {
            let _removed = old_parent.remove_authored_child(child_id);
        }
        let Some(parent) = self.get_unit_mut(parent_id) else {
            return false;
        };
        let _removed = parent.remove_authored_child(child_id);
        let _added = parent.add_authored_child(kind, child_id);
        self.set_authored_child_parent(child_id, Some(parent_id))
    }

    pub(super) fn prepare_kill_unit_authored_children(&mut self, unit_id: EntityId) {
        let children = self.detach_authored_child_relationships(unit_id);
        for child_id in children {
            let _killed = self.kill_entity(child_id, false);
        }
    }

    pub(super) fn prepare_remove_unit_authored_children(&mut self, unit_id: EntityId) {
        let children = self.detach_authored_child_relationships(unit_id);
        for child_id in children {
            let _removed = self.kill_entity(child_id, true);
        }
    }

    pub(super) fn prepare_remove_object_authored_relationships(&mut self, object_id: EntityId) {
        if let Some(object) = self.get_object_mut(object_id) {
            object.built_by = None;
        }
        for (_, unit) in self.units.iter_mut() {
            let _removed = unit.remove_authored_child(object_id);
        }
    }

    fn detach_authored_child_relationships(&mut self, unit_id: EntityId) -> Vec<EntityId> {
        let Some((kill_children, child_ids)) = self.get_unit_mut(unit_id).map(|unit| {
            unit.built_by = None;
            (
                unit.kills_authored_children_on_death(),
                unit.take_authored_children(),
            )
        }) else {
            return Vec::new();
        };
        for (_, unit) in self.units.iter_mut() {
            let _removed = unit.remove_authored_child(unit_id);
        }
        child_ids
            .into_iter()
            .filter(|&child_id| {
                let was_built_by_parent = self.authored_child_parent(child_id) == Some(unit_id);
                if was_built_by_parent {
                    let _cleared = self.set_authored_child_parent(child_id, None);
                }
                kill_children && was_built_by_parent
            })
            .collect()
    }

    fn authored_child_is_alive(&self, child_id: EntityId, kind: AuthoredUnitChildKind) -> bool {
        match kind {
            AuthoredUnitChildKind::Object => {
                self.get_object(child_id).is_some_and(Entity::is_alive)
            }
            AuthoredUnitChildKind::Building
            | AuthoredUnitChildKind::Unit
            | AuthoredUnitChildKind::Foundation => {
                self.get_unit(child_id).is_some_and(Entity::is_alive)
            }
        }
    }

    fn authored_child_parent(&self, child_id: EntityId) -> Option<EntityId> {
        match child_id.class() {
            Some(EntityClass::Object) => {
                self.get_object(child_id).and_then(|object| object.built_by)
            }
            Some(EntityClass::Unit) => self.get_unit(child_id).and_then(|unit| unit.built_by),
            _ => None,
        }
    }

    fn set_authored_child_parent(
        &mut self,
        child_id: EntityId,
        parent_id: Option<EntityId>,
    ) -> bool {
        match child_id.class() {
            Some(EntityClass::Object) => self.get_object_mut(child_id).is_some_and(|object| {
                object.built_by = parent_id;
                true
            }),
            Some(EntityClass::Unit) => self.get_unit_mut(child_id).is_some_and(|unit| {
                unit.built_by = parent_id;
                true
            }),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_flag_cascades_and_breaks_relationships_immediately() {
        let mut world = World::new();
        let parent_id = world.create_building(1);
        let child_id = world.create_building(1);
        world
            .get_unit_mut(parent_id)
            .unwrap()
            .set_kill_authored_children_on_death(true);
        assert!(world.associate_authored_child(
            parent_id,
            child_id,
            AuthoredUnitChildKind::Foundation,
        ));

        assert!(world.kill_unit(parent_id, false));

        assert!(
            world
                .get_unit(child_id)
                .is_some_and(|child| !child.is_alive())
        );
        assert_eq!(world.get_unit(child_id).unwrap().built_by, None);
        assert!(
            world
                .get_unit(parent_id)
                .unwrap()
                .associated_foundations()
                .is_empty()
        );
    }

    #[test]
    fn child_survives_when_parent_prototype_does_not_request_cascade() {
        let mut world = World::new();
        let parent_id = world.create_building(1);
        let child_id = world.create_building(1);
        assert!(world.associate_authored_child(parent_id, child_id, AuthoredUnitChildKind::Unit,));

        assert!(world.kill_unit(parent_id, false));

        assert!(world.get_unit(child_id).is_some_and(Entity::is_alive));
        assert_eq!(world.get_unit(child_id).unwrap().built_by, None);
    }

    #[test]
    fn removing_a_child_cleans_the_parent_reference() {
        let mut world = World::new();
        let parent_id = world.create_building(1);
        let child_id = world.create_building(1);
        assert!(world.associate_authored_child(parent_id, child_id, AuthoredUnitChildKind::Unit,));

        let _removed = world.remove_unit(child_id);

        assert!(
            world
                .get_unit(parent_id)
                .unwrap()
                .associated_child_units()
                .is_empty()
        );
    }
}
