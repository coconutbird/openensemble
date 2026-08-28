//! Shared retail-style entity kill and immediate-destroy operations.

use super::World;
use crate::entity_id::{EntityClass, EntityId};

impl World {
    /// Kill an entity, optionally removing it immediately.
    ///
    /// A regular kill leaves dead state observable until the next entity
    /// update. Immediate destruction invalidates the entity ID before this
    /// method returns, matching the distinction trigger DBIDs 37 and 38 make.
    pub fn kill_entity(&mut self, entity_id: EntityId, immediate: bool) -> bool {
        match entity_id.class() {
            Some(EntityClass::Unit) => self.kill_unit(entity_id, immediate),
            Some(EntityClass::Squad) => self.kill_squad(entity_id, immediate),
            _ => false,
        }
    }

    /// Kill or immediately destroy one mobile unit or building.
    pub fn kill_unit(&mut self, unit_id: EntityId, immediate: bool) -> bool {
        if immediate {
            return self.remove_unit(unit_id).is_some();
        }
        let Some(unit) = self.get_unit_mut(unit_id) else {
            return false;
        };
        unit.kill();
        true
    }

    /// Kill or immediately destroy a squad and each of its member units.
    pub fn kill_squad(&mut self, squad_id: EntityId, immediate: bool) -> bool {
        let Some(member_ids) = self.get_squad(squad_id).map(|squad| squad.unit_ids.clone()) else {
            return false;
        };
        if immediate {
            for unit_id in member_ids {
                let _removed = self.remove_unit(unit_id);
            }
            let _removed = self.remove_squad(squad_id);
            return true;
        }
        if let Some(squad) = self.get_squad_mut(squad_id) {
            squad.kill();
        }
        for unit_id in member_ids {
            if let Some(unit) = self.get_unit_mut(unit_id) {
                unit.kill();
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entity;

    #[test]
    fn regular_kill_is_observable_until_update_and_destroy_is_immediate() {
        let mut world = World::new();
        world.init_players(1);
        let killed = world.create_unit(1);
        let destroyed = world.create_unit(1);

        assert!(world.kill_entity(killed, false));
        assert!(world.get_unit(killed).is_some_and(|unit| !unit.is_alive()));
        assert!(world.kill_entity(destroyed, true));
        assert!(world.get_unit(destroyed).is_none());

        world.update_entities(0.05);
        assert!(world.get_unit(killed).is_none());
    }

    #[test]
    fn squad_lifecycle_cascades_to_members() {
        let mut world = World::new();
        world.init_players(1);
        let squad_id = world.create_squad(1);
        let unit_id = world.create_unit(1);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));

        assert!(world.kill_squad(squad_id, false));
        assert!(
            world
                .get_squad(squad_id)
                .is_some_and(|squad| !squad.is_alive())
        );
        assert!(world.get_unit(unit_id).is_some_and(|unit| !unit.is_alive()));

        world.update_entities(0.05);
        assert!(world.get_squad(squad_id).is_none());
        assert!(world.get_unit(unit_id).is_none());
    }
}
