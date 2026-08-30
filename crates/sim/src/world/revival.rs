//! Authoritative retail down, hibernation, and revival transitions.

use super::World;
use crate::entity::Entity;
use crate::entity_id::{EntityClass, EntityId};
use crate::gameplay::{GameplayCatalog, UnitRevivalProfile};

impl World {
    /// Configure persistent revival actions for every currently loaded unit.
    pub fn configure_unit_revivals(&mut self, gameplay: &GameplayCatalog) {
        let profiles = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                gameplay
                    .unit_revival_profile(&unit.proto_object_name)
                    .map(|profile| (unit_id, profile))
            })
            .collect::<Vec<_>>();
        for (unit_id, profile) in profiles {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.configure_revival(profile);
            }
        }
    }

    pub(crate) fn configure_unit_revival(
        &mut self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(profile) = self
            .units
            .get(unit_id)
            .and_then(|unit| gameplay.unit_revival_profile(&unit.proto_object_name))
        else {
            return false;
        };
        self.units.get_mut(unit_id).is_some_and(|unit| {
            unit.configure_revival(profile);
            true
        })
    }

    /// Return retail `BSquad::isDown`: true when any child unit is down.
    #[must_use]
    pub fn is_squad_down(&self, squad_id: EntityId) -> bool {
        self.squads.get(squad_id).is_some_and(|squad| {
            squad.unit_ids.iter().any(|unit_id| {
                self.units
                    .get(*unit_id)
                    .is_some_and(crate::entities::Unit::is_down)
            })
        })
    }

    /// Return retail `BSquad::isHibernating`: true for any hibernating child.
    #[must_use]
    pub fn is_squad_hibernating(&self, squad_id: EntityId) -> bool {
        self.squads.get(squad_id).is_some_and(|squad| {
            squad.unit_ids.iter().any(|unit_id| {
                self.units
                    .get(*unit_id)
                    .is_some_and(crate::entities::Unit::is_hibernating)
            })
        })
    }

    /// Return whether any child keeps this squad from accepting normal orders.
    #[must_use]
    pub fn is_squad_incapacitated(&self, squad_id: EntityId) -> bool {
        self.is_squad_down(squad_id)
            || self.is_squad_hibernating(squad_id)
            || self
                .squads
                .get(squad_id)
                .is_some_and(crate::entities::Squad::is_being_pulled)
    }

    /// Test the exact liveness contract shared by retail V3 conditions and filters.
    #[must_use]
    pub fn is_entity_trigger_alive(&self, entity_id: EntityId) -> bool {
        match entity_id.class() {
            Some(EntityClass::Object) => self.objects.get(entity_id).is_some_and(Entity::is_alive),
            Some(EntityClass::Unit) => self
                .units
                .get(entity_id)
                .is_some_and(|unit| unit.is_alive() && !unit.is_down() && !unit.is_hibernating()),
            Some(EntityClass::Squad) => self
                .squads
                .get(entity_id)
                .is_some_and(|squad| squad.is_alive() && !self.is_squad_incapacitated(entity_id)),
            Some(EntityClass::Projectile) => self
                .projectiles
                .get(entity_id)
                .is_some_and(Entity::is_alive),
            _ => false,
        }
    }

    pub(super) fn update_revivals(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.configure_unit_revivals(gameplay);
        let mut ready_heroes = Vec::new();
        let mut changed_bases = Vec::new();
        for (unit_id, unit) in self.units.iter_mut() {
            let was_alive = unit.is_alive();
            if was_alive && unit.advance_revival(dt) {
                ready_heroes.push(unit_id);
            }
            if was_alive
                && !unit.is_alive()
                && let Some(base_id) = unit.base_id
            {
                changed_bases.push(base_id);
            }
        }
        changed_bases.sort_unstable();
        changed_bases.dedup();
        for base_id in changed_bases {
            self.recompute_base_child_damage(base_id);
        }
        for unit_id in ready_heroes {
            if self.hero_has_revival_ally(unit_id)
                && let Some(unit) = self.units.get_mut(unit_id)
            {
                let _revived = unit.finish_hero_revival();
            }
        }
    }

    pub(super) fn cancel_incapacitated_squad_orders(&mut self, unit_id: EntityId) {
        let squad_id = self.units.get(unit_id).and_then(|unit| unit.squad_id);
        if let Some(squad_id) = squad_id
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.remove_all_orders();
        }
    }

    fn hero_has_revival_ally(&self, unit_id: EntityId) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let Some(UnitRevivalProfile::Hero(profile)) = unit.revival_profile() else {
            return false;
        };
        let Some(own_squad_id) = unit.squad_id else {
            return false;
        };
        let position = unit.base.position;
        let radius_squared = profile.revival_distance * profile.revival_distance;
        self.squads
            .iter()
            .filter(|(squad_id, squad)| {
                let offset = squad.base.position - position;
                *squad_id != own_squad_id
                    && squad.is_alive()
                    && !self.is_squad_down(*squad_id)
                    && self.players_are_allied(unit.base.player_id, squad.base.player_id)
                    && offset.x.mul_add(offset.x, offset.z * offset.z) <= radius_squared
            })
            .take(60)
            .next()
            .is_some()
    }
}

#[cfg(test)]
mod tests;
