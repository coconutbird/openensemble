//! Authoritative access to runtime `BObject` state.

use super::World;
use crate::entities::{DopplePolicy, ObjectState, TargetingSelection};
use crate::entity_id::{EntityClass, EntityId};

impl World {
    /// Return runtime object state for a live `BObject`-derived entity.
    #[must_use]
    pub fn entity_object_state(&self, entity_id: EntityId) -> Option<&ObjectState> {
        match entity_id.class()? {
            EntityClass::Object => self
                .objects
                .get(entity_id)
                .map(|object| &object.object_state),
            EntityClass::Unit => self.units.get(entity_id).map(|unit| &unit.object_state),
            EntityClass::Projectile => self
                .projectiles
                .get(entity_id)
                .map(|projectile| &projectile.object_state),
            _ => None,
        }
    }

    /// Return the active targeting-selection request for one live entity.
    #[must_use]
    pub fn entity_targeting_selection(&self, entity_id: EntityId) -> Option<TargetingSelection> {
        self.entity_object_state(entity_id)?.targeting_selection()
    }

    /// Return one live entity's fog-memory policy.
    #[must_use]
    pub fn entity_dopple_policy(&self, entity_id: EntityId) -> Option<DopplePolicy> {
        self.entity_object_state(entity_id)
            .map(ObjectState::dopple_policy)
    }

    /// Install or refresh retail's additive targeting-selection texture.
    pub fn flash_entity(
        &mut self,
        entity_id: EntityId,
        interval_ms: u32,
        duration_ms: u32,
        color: [u8; 4],
        intensity: f32,
    ) -> bool {
        let now_ms = self.game_time_ms;
        let Some(state) = self.entity_object_state_mut(entity_id) else {
            return false;
        };
        state.flash(now_ms, interval_ms, duration_ms, color, intensity);
        true
    }

    /// Change fog-memory policy and invalidate every existing team dopple.
    pub fn reset_entity_dopples(
        &mut self,
        entity_id: EntityId,
        gray_map_dopples: bool,
        dopples: bool,
    ) -> bool {
        let Some(state) = self.entity_object_state_mut(entity_id) else {
            return false;
        };
        state.reset_dopples(gray_map_dopples, dopples);
        true
    }

    pub(super) fn update_object_states(&mut self) {
        let now_ms = self.game_time_ms;
        for (_, object) in self.objects.iter_mut() {
            object.object_state.update(now_ms);
        }
        for (_, unit) in self.units.iter_mut() {
            unit.object_state.update(now_ms);
        }
        for (_, projectile) in self.projectiles.iter_mut() {
            projectile.object_state.update(now_ms);
        }
    }

    fn entity_object_state_mut(&mut self, entity_id: EntityId) -> Option<&mut ObjectState> {
        match entity_id.class()? {
            EntityClass::Object => self
                .objects
                .get_mut(entity_id)
                .map(|object| &mut object.object_state),
            EntityClass::Unit => self
                .units
                .get_mut(entity_id)
                .map(|unit| &mut unit.object_state),
            EntityClass::Projectile => self
                .projectiles
                .get_mut(entity_id)
                .map(|projectile| &mut projectile.object_state),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::World;

    #[test]
    fn flash_and_dopple_state_are_owned_and_expired_by_the_world() {
        let mut world = World::new();
        let unit_id = world.create_unit(1);
        assert!(world.flash_entity(unit_id, 500, 1_000, [255, 255, 0, 255], 20.0));
        assert!(world.reset_entity_dopples(unit_id, true, false));

        let policy = world.entity_dopple_policy(unit_id).unwrap();
        assert!(policy.gray_map_dopples());
        assert!(!policy.dopples());
        assert!(policy.visibility_update_pending());

        world.game_time_ms = 1_000;
        world.update_object_states();
        assert!(world.entity_targeting_selection(unit_id).is_some());
        assert!(
            !world
                .entity_dopple_policy(unit_id)
                .unwrap()
                .visibility_update_pending()
        );

        world.game_time_ms = 1_001;
        world.update_object_states();
        assert!(world.entity_targeting_selection(unit_id).is_none());
    }
}
