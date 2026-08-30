//! Associated parking-lot ownership and lifetime.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;

impl World {
    /// Associate a live building with the unit that performs its birth sequence.
    pub fn associate_parking_lot(
        &mut self,
        building_id: EntityId,
        parking_lot_id: EntityId,
    ) -> bool {
        if building_id == parking_lot_id
            || !self.get_building(building_id).is_some_and(Entity::is_alive)
            || !self
                .get_building(parking_lot_id)
                .is_some_and(Entity::is_alive)
        {
            return false;
        }
        let Some(building) = self.get_building_mut(building_id) else {
            return false;
        };
        building.associated_parking_lot_id = Some(parking_lot_id);
        true
    }

    pub(super) fn prepare_remove_unit_parking_lot(&mut self, unit_id: EntityId) {
        let owned = self.detach_unit_parking_lot_refs(unit_id);
        if let Some(parking_lot_id) = owned
            && self.get_unit(parking_lot_id).is_some()
        {
            let _removed = self.remove_unit(parking_lot_id);
        }
    }

    pub(super) fn prepare_kill_unit_parking_lot(&mut self, unit_id: EntityId) {
        let owned = self.detach_unit_parking_lot_refs(unit_id);
        if let Some(parking_lot_id) = owned {
            let _killed = self.kill_unit(parking_lot_id, false);
        }
    }

    fn detach_unit_parking_lot_refs(&mut self, unit_id: EntityId) -> Option<EntityId> {
        let owned = self
            .get_unit(unit_id)
            .and_then(|unit| unit.associated_parking_lot_id);
        if let Some(unit) = self.get_unit_mut(unit_id) {
            unit.associated_parking_lot_id = None;
        }
        for (_, unit) in self.units.iter_mut() {
            if unit.associated_parking_lot_id == Some(unit_id) {
                unit.associated_parking_lot_id = None;
            }
        }
        owned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_parking_lot_follows_building_lifetime() {
        let mut world = World::new();
        let building = world.create_building(1);
        let parking = world.create_building(1);
        assert!(world.associate_parking_lot(building, parking));
        assert_eq!(
            world
                .get_building(building)
                .unwrap()
                .associated_parking_lot(),
            Some(parking)
        );

        let _removed = world.remove_unit(building);
        assert!(world.get_unit(parking).is_none());
    }
}
