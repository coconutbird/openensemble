//! Persistent relationships created from a prototype's authored child objects.

use super::Unit;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// Relationship kind used by retail child-object entity references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthoredUnitChildKind {
    Object,
    Building,
    Unit,
    Foundation,
}

/// Parent-owned authored child references and their prototype lifetime rule.
#[derive(Debug, Clone, Default)]
pub(crate) struct UnitAuthoredChildren {
    object_ids: Vec<EntityId>,
    building_ids: Vec<EntityId>,
    unit_ids: Vec<EntityId>,
    foundation_ids: Vec<EntityId>,
    kill_on_death: bool,
}

impl UnitAuthoredChildren {
    fn ids(&self, kind: AuthoredUnitChildKind) -> &[EntityId] {
        match kind {
            AuthoredUnitChildKind::Object => &self.object_ids,
            AuthoredUnitChildKind::Building => &self.building_ids,
            AuthoredUnitChildKind::Unit => &self.unit_ids,
            AuthoredUnitChildKind::Foundation => &self.foundation_ids,
        }
    }

    fn ids_mut(&mut self, kind: AuthoredUnitChildKind) -> &mut Vec<EntityId> {
        match kind {
            AuthoredUnitChildKind::Object => &mut self.object_ids,
            AuthoredUnitChildKind::Building => &mut self.building_ids,
            AuthoredUnitChildKind::Unit => &mut self.unit_ids,
            AuthoredUnitChildKind::Foundation => &mut self.foundation_ids,
        }
    }

    fn add(&mut self, kind: AuthoredUnitChildKind, child_id: EntityId) -> bool {
        let ids = self.ids_mut(kind);
        if ids.contains(&child_id) {
            return false;
        }
        ids.push(child_id);
        true
    }

    fn remove(&mut self, child_id: EntityId) -> bool {
        let old_object_count = self.object_ids.len();
        let old_building_count = self.building_ids.len();
        let old_unit_count = self.unit_ids.len();
        let old_foundation_count = self.foundation_ids.len();
        self.object_ids.retain(|&candidate| candidate != child_id);
        self.building_ids.retain(|&candidate| candidate != child_id);
        self.unit_ids.retain(|&candidate| candidate != child_id);
        self.foundation_ids
            .retain(|&candidate| candidate != child_id);
        old_object_count != self.object_ids.len()
            || old_building_count != self.building_ids.len()
            || old_unit_count != self.unit_ids.len()
            || old_foundation_count != self.foundation_ids.len()
    }

    fn take_all(&mut self) -> Vec<EntityId> {
        let mut children = std::mem::take(&mut self.object_ids);
        children.append(&mut self.building_ids);
        children.append(&mut self.unit_ids);
        children.append(&mut self.foundation_ids);
        children
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.kill_on_death));
        hash_ids(checksum, &self.object_ids);
        hash_ids(checksum, &self.building_ids);
        hash_ids(checksum, &self.unit_ids);
        hash_ids(checksum, &self.foundation_ids);
    }
}

impl Unit {
    /// Return class-zero children linked through retail's `AssociatedObject` relationship.
    #[must_use]
    pub fn associated_child_objects(&self) -> &[EntityId] {
        self.authored_children.ids(AuthoredUnitChildKind::Object)
    }

    /// Return unit-pool children linked through retail's `AssociatedBuilding` relationship.
    #[must_use]
    pub fn associated_child_buildings(&self) -> &[EntityId] {
        self.authored_children.ids(AuthoredUnitChildKind::Building)
    }

    /// Return children linked through retail's `AssociatedUnit` relationship.
    #[must_use]
    pub fn associated_child_units(&self) -> &[EntityId] {
        self.authored_children.ids(AuthoredUnitChildKind::Unit)
    }

    /// Return children linked through retail's `AssociatedFoundation` relationship.
    #[must_use]
    pub fn associated_foundations(&self) -> &[EntityId] {
        self.authored_children
            .ids(AuthoredUnitChildKind::Foundation)
    }

    pub(crate) fn set_kill_authored_children_on_death(&mut self, enabled: bool) {
        self.authored_children.kill_on_death = enabled;
    }

    pub(crate) const fn kills_authored_children_on_death(&self) -> bool {
        self.authored_children.kill_on_death
    }

    pub(crate) fn add_authored_child(
        &mut self,
        kind: AuthoredUnitChildKind,
        child_id: EntityId,
    ) -> bool {
        self.authored_children.add(kind, child_id)
    }

    pub(crate) fn remove_authored_child(&mut self, child_id: EntityId) -> bool {
        self.authored_children.remove(child_id)
    }

    pub(crate) fn take_authored_children(&mut self) -> Vec<EntityId> {
        self.authored_children.take_all()
    }

    pub(crate) fn hash_authored_child_state(&self, checksum: &mut SyncChecksum) {
        self.authored_children.hash_state(checksum);
    }
}

fn hash_ids(checksum: &mut SyncChecksum, ids: &[EntityId]) {
    checksum.hash_u32(u32::try_from(ids.len()).unwrap_or(u32::MAX));
    for id in ids {
        checksum.hash_u32(id.as_u32());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relationship_kinds_preserve_insertion_order_and_remove_together() {
        let mut unit = Unit::default();
        let object = EntityId::new(crate::entity_id::EntityClass::Object, 2);
        let building = EntityId::new(crate::entity_id::EntityClass::Unit, 3);
        let first = EntityId::new(crate::entity_id::EntityClass::Unit, 4);
        let second = EntityId::new(crate::entity_id::EntityClass::Unit, 7);
        assert!(unit.add_authored_child(AuthoredUnitChildKind::Object, object));
        assert!(unit.add_authored_child(AuthoredUnitChildKind::Building, building));
        assert!(unit.add_authored_child(AuthoredUnitChildKind::Unit, first));
        assert!(unit.add_authored_child(AuthoredUnitChildKind::Foundation, second));
        assert!(!unit.add_authored_child(AuthoredUnitChildKind::Unit, first));
        assert_eq!(unit.associated_child_objects(), &[object]);
        assert_eq!(unit.associated_child_buildings(), &[building]);
        assert_eq!(unit.associated_child_units(), &[first]);
        assert_eq!(unit.associated_foundations(), &[second]);
        assert!(unit.remove_authored_child(first));
        assert!(unit.associated_child_units().is_empty());
    }
}
