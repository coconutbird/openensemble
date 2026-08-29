//! Authoritative entity transforms used by scripted world manipulation.

use super::World;
use crate::entities::squads::formation_offset_to_local;
use crate::entity::Entity;
use crate::entity_id::{EntityClass, EntityId};
use crate::player::PlayerId;
use glam::Vec3;

impl World {
    /// Read a live trigger-addressable entity's world position.
    #[must_use]
    pub fn entity_position(&self, entity_id: EntityId) -> Option<Vec3> {
        match entity_id.class()? {
            EntityClass::Object => self
                .objects
                .get(entity_id)
                .map(|object| object.base.position),
            EntityClass::Unit => self.units.get(entity_id).map(|unit| unit.base.position),
            EntityClass::Squad => self.squads.get(entity_id).map(|squad| squad.base.position),
            EntityClass::Projectile => self
                .projectiles
                .get(entity_id)
                .map(|projectile| projectile.base.position),
            _ => None,
        }
    }

    /// Read a live trigger-addressable entity's normalized forward vector.
    #[must_use]
    pub fn entity_forward(&self, entity_id: EntityId) -> Option<Vec3> {
        match entity_id.class()? {
            EntityClass::Object => self
                .objects
                .get(entity_id)
                .map(|object| object.base.forward.normalize_or_zero()),
            EntityClass::Unit => self
                .units
                .get(entity_id)
                .map(|unit| unit.base.forward.normalize_or_zero()),
            EntityClass::Squad => self
                .squads
                .get(entity_id)
                .map(|squad| squad.base.forward.normalize_or_zero()),
            EntityClass::Projectile => self
                .projectiles
                .get(entity_id)
                .map(|projectile| projectile.base.forward.normalize_or_zero()),
            _ => None,
        }
    }

    /// Read a live trigger-addressable entity's owner.
    #[must_use]
    pub fn entity_owner(&self, entity_id: EntityId) -> Option<PlayerId> {
        match entity_id.class()? {
            EntityClass::Object => self
                .objects
                .get(entity_id)
                .map(|object| object.base.player_id),
            EntityClass::Unit => self.units.get(entity_id).map(|unit| unit.base.player_id),
            EntityClass::Squad => self.squads.get(entity_id).map(|squad| squad.base.player_id),
            EntityClass::Projectile => self
                .projectiles
                .get(entity_id)
                .map(|projectile| projectile.base.player_id),
            _ => None,
        }
    }

    /// Set one live unit or squad forward vector.
    pub fn set_entity_forward(&mut self, entity_id: EntityId, forward: Vec3) -> bool {
        if !forward.is_finite() {
            return false;
        }
        match entity_id.class() {
            Some(EntityClass::Object) => self.objects.get_mut(entity_id).is_some_and(|object| {
                object.base.set_forward(forward);
                true
            }),
            Some(EntityClass::Unit) => self.units.get_mut(entity_id).is_some_and(|unit| {
                unit.base.set_forward(forward);
                true
            }),
            Some(EntityClass::Squad) => self.squads.get_mut(entity_id).is_some_and(|squad| {
                squad.base.set_forward(forward);
                true
            }),
            Some(EntityClass::Projectile) => {
                self.projectiles
                    .get_mut(entity_id)
                    .is_some_and(|projectile| {
                        projectile.base.set_forward(forward);
                        true
                    })
            }
            _ => false,
        }
    }

    /// Teleport a live squad and synchronize every member from formation data.
    pub fn teleport_squad(&mut self, squad_id: EntityId, position: Vec3) -> bool {
        if !position.is_finite() || !self.squads.get(squad_id).is_some_and(Entity::is_alive) {
            return false;
        }
        self.detach_passenger_refs(squad_id);
        let (forward, unit_ids) = {
            let Some(squad) = self.squads.get_mut(squad_id) else {
                return false;
            };
            squad.garrison.finish_action();
            squad.clear_attack_order();
            squad.stop();
            squad.base.position = position;
            squad.base.velocity = Vec3::ZERO;
            (squad.base.forward, squad.unit_ids.clone())
        };
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.clear_attack_order();
                unit.stop();
            }
        }
        self.place_squad_members(squad_id, position, forward, true);
        true
    }

    /// Teleport one live object without moving its parent squad anchor.
    pub fn teleport_object(&mut self, unit_id: EntityId, position: Vec3) -> bool {
        if !position.is_finite() {
            return false;
        }
        if let Some(object) = self
            .objects
            .get_mut(unit_id)
            .filter(|object| object.is_alive())
        {
            object.base.position = position;
            object.base.velocity = Vec3::ZERO;
            return true;
        }
        if let Some(projectile) = self
            .projectiles
            .get_mut(unit_id)
            .filter(|projectile| projectile.is_alive())
        {
            projectile.base.position = position;
            projectile.base.velocity = Vec3::ZERO;
            return true;
        }
        let squad_transform = self.units.get(unit_id).and_then(|unit| {
            unit.squad_id.and_then(|squad_id| {
                self.squads
                    .get(squad_id)
                    .map(|squad| (squad.base.position, squad.base.forward))
            })
        });
        let Some(unit) = self.units.get_mut(unit_id).filter(|unit| unit.is_alive()) else {
            return false;
        };
        unit.clear_attack_order();
        unit.stop();
        unit.base.position = position;
        unit.base.velocity = Vec3::ZERO;
        if let Some((squad_position, squad_forward)) = squad_transform {
            unit.formation_offset =
                formation_offset_to_local(squad_forward, position - squad_position);
        }
        true
    }
}
