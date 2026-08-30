//! Runtime state for retail's general thrown-unit action.

use super::Unit;
use crate::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitThrown {
    active: bool,
    thrower_id: EntityId,
    release_physics_on_landing: bool,
}

impl Default for UnitThrown {
    fn default() -> Self {
        Self {
            active: false,
            thrower_id: EntityId::INVALID,
            release_physics_on_landing: false,
        }
    }
}

impl UnitThrown {
    fn begin(
        &mut self,
        thrower_id: EntityId,
        release_physics_on_landing: bool,
        wait_for_landing: bool,
    ) {
        if !wait_for_landing {
            *self = Self::default();
            return;
        }
        self.active = true;
        self.thrower_id = thrower_id;
        self.release_physics_on_landing |= release_physics_on_landing;
    }

    fn finish_if_grounded(&mut self, grounded: bool) -> Option<bool> {
        if !self.active || !grounded {
            return None;
        }
        let release_physics = self.release_physics_on_landing;
        *self = Self::default();
        Some(release_physics)
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.active));
        checksum.hash_u32(self.thrower_id.as_u32());
        checksum.hash_u32(u32::from(self.release_physics_on_landing));
    }
}

impl Unit {
    pub(crate) fn begin_throw(
        &mut self,
        thrower_id: EntityId,
        velocity: Vec3,
        release_physics_on_landing: bool,
    ) -> bool {
        if !velocity.is_finite() || self.physics.is_none() {
            return false;
        }
        self.clear_attack_order();
        self.stop();
        if !self.set_physics_velocity(velocity) {
            return false;
        }
        let wait_for_landing = release_physics_on_landing || self.is_object_type("Infantry");
        self.thrown
            .begin(thrower_id, release_physics_on_landing, wait_for_landing);
        true
    }

    /// Return whether a thrown-unit action currently controls locomotion.
    #[must_use]
    pub const fn is_thrown(&self) -> bool {
        self.thrown.active
    }

    /// Entity whose action launched this unit.
    #[must_use]
    pub const fn thrown_by(&self) -> Option<EntityId> {
        if self.thrown.active {
            Some(self.thrown.thrower_id)
        } else {
            None
        }
    }

    pub(crate) fn finish_throw_if_grounded(&mut self) {
        let grounded = self
            .physics
            .as_ref()
            .is_some_and(crate::physics::PhysicsBody::is_grounded);
        match self.thrown.finish_if_grounded(grounded) {
            Some(true) => {
                self.physics = None;
                self.base.velocity = Vec3::ZERO;
            }
            Some(false) => {
                if let Some(body) = &mut self.physics {
                    let _linear = body.set_linear_velocity(&mut self.base, Vec3::ZERO);
                    let _angular = body.set_angular_velocity(Vec3::ZERO);
                }
            }
            None => {}
        }
    }

    pub(crate) fn hash_thrown_state(&self, checksum: &mut SyncChecksum) {
        self.thrown.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entity;
    use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
    use crate::{EntityClass, EntityId};

    #[test]
    fn temporary_throw_physics_is_released_after_ground_contact() {
        let id = EntityId::new(EntityClass::Unit, 1);
        let thrower = EntityId::new(EntityClass::Unit, 2);
        let mut unit = Unit::new(id, 1);
        unit.physics = Some(PhysicsBody::dynamic_replacement(
            PhysicsMaterial::default(),
            BoxCollider::new(Vec3::splat(0.5), Vec3::ZERO),
            0.0,
            0.0,
        ));

        assert!(unit.begin_throw(thrower, Vec3::ZERO, true));
        assert!(unit.is_thrown());
        assert_eq!(unit.thrown_by(), Some(thrower));
        unit.update(0.05);

        assert!(!unit.is_thrown());
        assert!(unit.physics.is_none());
        assert_eq!(unit.base.velocity, Vec3::ZERO);
    }

    #[test]
    fn existing_infantry_physics_stops_all_motion_on_landing() {
        let id = EntityId::new(EntityClass::Unit, 1);
        let thrower = EntityId::new(EntityClass::Unit, 2);
        let mut unit = Unit::new(id, 1);
        unit.object_types.push("Infantry".to_owned());
        unit.physics = Some(PhysicsBody::dynamic_replacement(
            PhysicsMaterial::default(),
            BoxCollider::new(Vec3::splat(0.5), Vec3::ZERO),
            0.0,
            0.0,
        ));
        assert!(unit.apply_impulse_at_point(Vec3::X, Vec3::Y));
        assert_ne!(
            unit.physics.as_ref().unwrap().angular_velocity(),
            Vec3::ZERO
        );

        assert!(unit.begin_throw(thrower, Vec3::X * 4.0, false));
        assert!(unit.is_thrown());
        unit.finish_throw_if_grounded();

        assert!(!unit.is_thrown());
        assert_eq!(unit.base.velocity, Vec3::ZERO);
        assert_eq!(
            unit.physics.as_ref().unwrap().angular_velocity(),
            Vec3::ZERO
        );
    }

    #[test]
    fn existing_non_infantry_physics_completes_throw_action_immediately() {
        let id = EntityId::new(EntityClass::Unit, 1);
        let thrower = EntityId::new(EntityClass::Unit, 2);
        let mut unit = Unit::new(id, 1);
        unit.physics = Some(PhysicsBody::dynamic_replacement(
            PhysicsMaterial::default(),
            BoxCollider::new(Vec3::splat(0.5), Vec3::ZERO),
            0.0,
            0.0,
        ));
        let velocity = Vec3::new(3.0, 4.0, 5.0);

        assert!(unit.begin_throw(thrower, velocity, false));

        assert!(!unit.is_thrown());
        assert_eq!(unit.thrown_by(), None);
        assert_eq!(unit.base.velocity, velocity);
    }
}
