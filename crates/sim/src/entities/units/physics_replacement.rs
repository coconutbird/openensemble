//! Runtime identity and construction for retail physics death replacements.

use super::Unit;
use crate::entities::BaseEntity;
use crate::entity_id::EntityId;
use crate::gameplay::PhysicsReplacementProfile;
use crate::physics::PhysicsBody;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
enum PhysicsReplacementPhase {
    #[default]
    Inactive = 0,
    Active = 1,
    Detonated = 2,
    CleanupReady = 3,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct UnitPhysicsReplacement {
    phase: PhysicsReplacementPhase,
}

impl UnitPhysicsReplacement {
    fn mark_active(&mut self) {
        self.phase = PhysicsReplacementPhase::Active;
    }

    fn mark_detonated(&mut self) -> bool {
        if self.phase != PhysicsReplacementPhase::Active {
            return false;
        }
        self.phase = PhysicsReplacementPhase::Detonated;
        true
    }

    fn advance_cleanup(&mut self, resting: bool) -> bool {
        if !resting {
            return false;
        }
        match self.phase {
            PhysicsReplacementPhase::Detonated => {
                self.phase = PhysicsReplacementPhase::CleanupReady;
                false
            }
            PhysicsReplacementPhase::CleanupReady => true,
            PhysicsReplacementPhase::Inactive | PhysicsReplacementPhase::Active => false,
        }
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
    }
}

impl Unit {
    pub(crate) fn create_physics_replacement(
        &self,
        id: EntityId,
        profile: &PhysicsReplacementProfile,
        ground_height: f32,
    ) -> Self {
        let mut replacement = Self::new(id, self.base.player_id);
        replacement.base = replacement_base(self, id);
        replacement.object_state = self.object_state.clone();
        replacement.kind = self.kind;
        replacement.archetype = self.archetype;
        replacement.proto_object_id = self.proto_object_id;
        replacement
            .proto_object_name
            .clone_from(&self.proto_object_name);
        replacement.object_types.clone_from(&self.object_types);
        replacement.hitpoints = 1.0;
        replacement.max_hitpoints = 1.0;
        replacement.damage_multiplier = self.damage_multiplier;
        replacement.damage_taken_multiplier = self.damage_taken_multiplier;
        replacement.accuracy_scalar = self.accuracy_scalar;
        replacement.dodge_scalar = self.dodge_scalar;
        replacement.work_rate_scalar = self.work_rate_scalar;
        replacement.line_of_sight_scalar = self.line_of_sight_scalar;
        replacement.velocity_scalar = self.velocity_scalar;
        replacement.weapon_range_scalar = self.weapon_range_scalar;
        replacement.obstruction_half_extents = profile.collider().half_extents;
        replacement.physics = Some(PhysicsBody::dynamic_replacement(
            profile.material(),
            profile.collider(),
            ground_height,
            replacement.base.position.y,
        ));
        replacement.actions = self.actions.clone();
        replacement.base.set_selectable(false);
        replacement.set_auto_attackable(false);
        replacement.physics_replacement.mark_active();
        replacement
    }

    /// Return whether this live unit is a visual/physics death replacement.
    #[must_use]
    pub const fn is_physics_replacement(&self) -> bool {
        !matches!(
            self.physics_replacement.phase,
            PhysicsReplacementPhase::Inactive
        )
    }

    pub(crate) fn mark_physics_replacement_detonated(&mut self) -> bool {
        self.physics_replacement.mark_detonated()
    }

    pub(crate) const fn physics_replacement_is_detonated(&self) -> bool {
        matches!(
            self.physics_replacement.phase,
            PhysicsReplacementPhase::Detonated | PhysicsReplacementPhase::CleanupReady
        )
    }

    pub(crate) fn advance_physics_replacement_cleanup(&mut self) -> bool {
        let resting = self
            .physics
            .as_ref()
            .is_some_and(|body| body.is_grounded() && self.base.velocity.length() < 0.1);
        self.physics_replacement.advance_cleanup(resting)
    }

    pub(crate) fn hash_physics_replacement_state(&self, checksum: &mut SyncChecksum) {
        self.physics_replacement.hash_state(checksum);
    }
}

fn replacement_base(source: &Unit, id: EntityId) -> BaseEntity {
    let mut base = BaseEntity::new(id, source.base.player_id);
    base.position = source.base.position;
    base.set_forward(source.base.forward);
    base.velocity = source.base.velocity;
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;
    use crate::physics::{BoxCollider, PhysicsMaterial};
    use glam::Vec3;

    #[test]
    fn replacement_copies_identity_without_squad_or_population_ownership() {
        let source_id = EntityId::new(EntityClass::Unit, 1);
        let replacement_id = EntityId::new(EntityClass::Unit, 2);
        let mut source = Unit::new(source_id, 3);
        source.proto_object_id = 42;
        source.proto_object_name = "tank".to_owned();
        source.base.position = Vec3::new(1.0, 2.0, 3.0);
        source.base.velocity = Vec3::X * 4.0;
        source.squad_id = Some(EntityId::new(EntityClass::Squad, 1));
        source.population_costs.push(crate::player::PopulationCost {
            population_type: 0,
            amount: 2.0,
        });
        let profile = PhysicsReplacementProfile::new(
            "wreck",
            PhysicsMaterial::default(),
            BoxCollider::new(Vec3::splat(0.5), Vec3::ZERO),
        );

        let replacement = source.create_physics_replacement(replacement_id, &profile, 0.0);
        assert_eq!(replacement.base.id, replacement_id);
        assert_eq!(replacement.proto_object_id, 42);
        assert_eq!(replacement.proto_object_name, "tank");
        assert_eq!(replacement.base.position, source.base.position);
        assert_eq!(replacement.base.velocity, source.base.velocity);
        assert_eq!(replacement.hitpoints.to_bits(), 1.0_f32.to_bits());
        assert!(replacement.squad_id.is_none());
        assert!(replacement.population_costs.is_empty());
        assert!(replacement.is_physics_replacement());
        assert!(!replacement.base.is_selectable());
    }
}
