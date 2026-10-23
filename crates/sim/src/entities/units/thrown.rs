//! Runtime state for retail's general thrown-unit action (`BUnitActionThrown`).

use super::Unit;
use crate::EntityId;
use crate::sync::SyncChecksum;
use crate::world::World;
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

    fn finish_if_landed(&mut self, landed: bool) -> Option<bool> {
        if !self.active || !landed {
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

    pub(crate) fn hash_thrown_state(&self, checksum: &mut SyncChecksum) {
        self.thrown.hash_state(checksum);
    }
}

impl World {
    /// Throws a unit with a body: the body takes `velocity`. An infantry unit, or a unit whose
    /// body exists only for the throw, then waits for its landing. Returns `false` if the unit
    /// has no dynamic body or the velocity is not finite.
    pub(crate) fn begin_unit_throw(
        &mut self,
        unit_id: EntityId,
        thrower_id: EntityId,
        velocity: Vec3,
        release_physics_on_landing: bool,
    ) -> bool {
        if !velocity.is_finite() || !self.set_unit_body_linear_velocity(unit_id, velocity) {
            return false;
        }
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        unit.clear_attack_order();
        unit.stop();
        unit.base.velocity = velocity;
        let wait_for_landing = release_physics_on_landing || unit.is_object_type("Infantry");
        unit.thrown
            .begin(thrower_id, release_physics_on_landing, wait_for_landing);
        true
    }

    /// Ends the throw of each thrown unit whose body landed in the last physics step (a new
    /// terrain contact point while the body moved down, `BUnitActionThrown`). A body that exists
    /// only for the throw goes; another body stops.
    pub(crate) fn finish_unit_throws(&mut self) {
        let landed: Vec<(EntityId, bool)> = self
            .units
            .iter_mut()
            .filter_map(|(unit_id, unit)| {
                let landed = unit
                    .physics_object
                    .as_ref()
                    .is_some_and(|state| state.contacts.landed);
                unit.thrown
                    .finish_if_landed(landed)
                    .map(|release| (unit_id, release))
            })
            .collect();
        for (unit_id, release_physics) in landed {
            if release_physics {
                self.release_physics_object(unit_id);
            } else {
                let _linear = self.set_unit_body_linear_velocity(unit_id, Vec3::ZERO);
                let _angular = self.set_unit_body_angular_velocity(unit_id, Vec3::ZERO);
            }
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.base.velocity = Vec3::ZERO;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityClass;
    use crate::physics::test_support::{
        TERRAIN_HEIGHT, box_template, spawn_body_unit, world_with_flat_terrain,
    };

    /// Steps the physics and the throw landing test until the throw ends. Returns the steps.
    fn steps_until_landed(world: &mut World, unit_id: EntityId) -> u32 {
        for step in 1..=100 {
            world.step_physics(0.05);
            world.finish_unit_throws();
            if !world.get_unit(unit_id).unwrap().is_thrown() {
                return step;
            }
        }
        panic!("the thrown unit never landed");
    }

    fn thrown_world(infantry: bool) -> (World, EntityId) {
        let mut world = world_with_flat_terrain();
        let template = box_template("crate", Vec3::splat(0.5), 10.0, Vec3::Y * 0.5);
        let position = Vec3::new(50.0, TERRAIN_HEIGHT + 4.0, 50.0);
        let unit_id = spawn_body_unit(&mut world, position, &template);
        if infantry {
            let unit = world.get_unit_mut(unit_id).unwrap();
            unit.object_types.push("Infantry".to_owned());
        }
        (world, unit_id)
    }

    #[test]
    fn temporary_throw_physics_is_released_after_the_landing() {
        let (mut world, unit_id) = thrown_world(false);
        let thrower = EntityId::new(EntityClass::Unit, 99);

        assert!(world.begin_unit_throw(unit_id, thrower, Vec3::new(2.0, 3.0, 0.0), true));
        let unit = world.get_unit(unit_id).unwrap();
        assert!(unit.is_thrown());
        assert_eq!(unit.thrown_by(), Some(thrower));

        assert!(steps_until_landed(&mut world, unit_id) > 1);
        let unit = world.get_unit(unit_id).unwrap();
        assert!(unit.physics_object.is_none());
        assert_eq!(unit.base.velocity, Vec3::ZERO);
    }

    #[test]
    fn existing_infantry_physics_stops_all_motion_on_landing() {
        let (mut world, unit_id) = thrown_world(true);
        let thrower = EntityId::new(EntityClass::Unit, 99);
        assert!(world.apply_unit_angular_impulse(unit_id, Vec3::X * 5.0));
        assert_ne!(world.unit_body_angular_velocity(unit_id), Some(Vec3::ZERO));

        assert!(world.begin_unit_throw(unit_id, thrower, Vec3::X * 4.0, false));
        assert!(world.get_unit(unit_id).unwrap().is_thrown());
        let _steps = steps_until_landed(&mut world, unit_id);

        assert!(world.unit_body(unit_id).is_some());
        assert_eq!(world.get_unit(unit_id).unwrap().base.velocity, Vec3::ZERO);
        assert_eq!(world.unit_body_linear_velocity(unit_id), Some(Vec3::ZERO));
        assert_eq!(world.unit_body_angular_velocity(unit_id), Some(Vec3::ZERO));
    }

    #[test]
    fn existing_non_infantry_physics_completes_throw_action_immediately() {
        let (mut world, unit_id) = thrown_world(false);
        let thrower = EntityId::new(EntityClass::Unit, 99);
        let velocity = Vec3::new(3.0, 4.0, 5.0);

        assert!(world.begin_unit_throw(unit_id, thrower, velocity, false));

        let unit = world.get_unit(unit_id).unwrap();
        assert!(!unit.is_thrown());
        assert_eq!(unit.thrown_by(), None);
        assert_eq!(unit.base.velocity, velocity);
        assert_eq!(world.unit_body_linear_velocity(unit_id), Some(velocity));
    }

    #[test]
    fn a_unit_without_a_body_cannot_be_thrown() {
        let mut world = world_with_flat_terrain();
        let unit_id = world.create_unit_at(1, Vec3::new(50.0, TERRAIN_HEIGHT, 50.0));
        let thrower = EntityId::new(EntityClass::Unit, 99);

        assert!(!world.begin_unit_throw(unit_id, thrower, Vec3::Y, true));
        assert!(!world.get_unit(unit_id).unwrap().is_thrown());
    }
}
