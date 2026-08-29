//! Authoritative runtime control flags shared by triggers and commands.

use super::World;
use crate::EntityId;
use glam::Vec3;

impl World {
    /// Return the synchronized runtime render override for any `BObject`-derived entity.
    #[must_use]
    pub fn entity_is_render_enabled(&self, entity_id: EntityId) -> Option<bool> {
        self.entity_object_state(entity_id)
            .map(crate::entities::ObjectState::is_render_enabled)
    }

    /// Change the synchronized runtime render override for any `BObject`-derived entity.
    pub fn set_entity_render_enabled(&mut self, entity_id: EntityId, enabled: bool) -> bool {
        let Some(state) = self.entity_object_state_mut(entity_id) else {
            return false;
        };
        state.set_render_enabled(enabled);
        true
    }

    /// Return the live selectable override for any represented entity class.
    #[must_use]
    pub fn entity_is_selectable(&self, entity_id: EntityId) -> Option<bool> {
        self.objects
            .get(entity_id)
            .map(|object| object.base.is_selectable())
            .or_else(|| {
                self.units
                    .get(entity_id)
                    .map(|unit| unit.base.is_selectable())
            })
            .or_else(|| {
                self.squads
                    .get(entity_id)
                    .map(|squad| squad.base.is_selectable())
            })
            .or_else(|| {
                self.projectiles
                    .get(entity_id)
                    .map(|projectile| projectile.base.is_selectable())
            })
    }

    /// Set the live selectable override for any represented entity class.
    pub fn set_entity_selectable(&mut self, entity_id: EntityId, selectable: bool) -> bool {
        if let Some(object) = self.objects.get_mut(entity_id) {
            object.base.set_selectable(selectable);
            return true;
        }
        if let Some(unit) = self.units.get_mut(entity_id) {
            unit.base.set_selectable(selectable);
            return true;
        }
        if let Some(squad) = self.squads.get_mut(entity_id) {
            squad.base.set_selectable(selectable);
            return true;
        }
        if let Some(projectile) = self.projectiles.get_mut(entity_id) {
            projectile.base.set_selectable(selectable);
            return true;
        }
        false
    }

    /// Return whether a squad's live mobility override permits movement.
    #[must_use]
    pub fn squad_is_mobile(&self, squad_id: EntityId) -> Option<bool> {
        self.squads
            .get(squad_id)
            .map(|squad| squad.base.is_mobile())
    }

    /// Apply retail squad mobility semantics.
    ///
    /// Immutable non-mobile prototypes reject the override. A persistent
    /// disable removes all orders, while a temporary disable preserves the
    /// logical order so it can resume when mobility is restored.
    pub fn set_squad_mobile(&mut self, squad_id: EntityId, mobile: bool, temporary: bool) -> bool {
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        if !squad.base.is_ever_mobile() {
            return false;
        }
        if !mobile && !temporary {
            squad.remove_all_orders();
        }
        if !squad.base.set_mobile(mobile) {
            return false;
        }
        if !mobile {
            squad.base.velocity = Vec3::ZERO;
        }
        let unit_ids = squad.unit_ids.clone();
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id)
                && unit.is_physics_driven()
            {
                unit.stop();
            }
        }
        true
    }

    /// Return whether automatic target acquisition may choose a unit.
    #[must_use]
    pub fn unit_is_auto_attackable(&self, unit_id: EntityId) -> Option<bool> {
        self.units
            .get(unit_id)
            .map(crate::entities::Unit::is_auto_attackable)
    }

    /// Set whether automatic target acquisition may choose a unit.
    pub fn set_unit_auto_attackable(&mut self, unit_id: EntityId, auto_attackable: bool) -> bool {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        unit.set_auto_attackable(auto_attackable);
        true
    }
}

#[cfg(test)]
mod tests;
