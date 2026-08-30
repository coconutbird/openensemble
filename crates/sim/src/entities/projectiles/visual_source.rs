//! Sim-owned visual identity and inherited motion for launched source objects.

use super::{Projectile, ProjectileRuntimeFlags};
use crate::player::PlayerId;
use glam::Vec3;

impl Projectile {
    /// Prototype whose model represents this projectile in presentation.
    ///
    /// Most projectiles render their own logical prototype. Retail powers can
    /// instead retain the source object's model while still using the launched
    /// projectile's tactic, damage, collision, and lifetime.
    #[must_use]
    pub fn visual_proto_object_name(&self) -> &str {
        self.visual_proto_object_name
            .as_deref()
            .unwrap_or(&self.proto_object_name)
    }

    /// Projectile-local translation from its simulation center to its model root.
    #[must_use]
    pub const fn visual_center_offset(&self) -> Vec3 {
        self.visual_center_offset
    }

    pub(crate) fn inherit_source_visual(
        &mut self,
        proto_object_name: &str,
        variation_index: Option<usize>,
        visual_player_id: PlayerId,
        source_center_offset: Vec3,
    ) {
        let proto_object_name = proto_object_name.trim();
        self.visual_proto_object_name =
            (!proto_object_name.is_empty()).then(|| proto_object_name.to_owned());
        let variation_index = variation_index
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1);
        self.object_state
            .set_visual_variation_index(variation_index);
        self.base.player_id = visual_player_id;
        self.visual_center_offset = -finite_vec3_or_zero(source_center_offset);
    }

    /// Player credited for this projectile's collision and damage.
    #[must_use]
    pub const fn created_by_player_id(&self) -> PlayerId {
        self.created_by_player_id
    }

    pub(crate) fn inherit_source_motion(&mut self, velocity: Vec3, forward: Vec3, gravity: f32) {
        let velocity = finite_vec3_or_zero(velocity);
        self.base.velocity = velocity;
        self.base.set_forward(finite_vec3_or_forward(forward));
        self.current_speed = velocity.length();
        self.desired_speed = self.current_speed;
        self.acceleration = 0.0;
        self.fuel = 0.0;
        self.tracking = false;
        self.runtime_flags
            .set(ProjectileRuntimeFlags::TRACKING_PENDING, false);
        self.runtime_flags
            .set(ProjectileRuntimeFlags::TESTS_FUEL, false);
        self.affected_by_gravity = true;
        self.gravity = finite_nonnegative(gravity);
    }
}

fn finite_vec3_or_zero(value: Vec3) -> Vec3 {
    if value.is_finite() { value } else { Vec3::ZERO }
}

fn finite_vec3_or_forward(value: Vec3) -> Vec3 {
    if value.is_finite() { value } else { Vec3::Z }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::projectiles::ProjectileLaunch;
    use crate::gameplay::ProjectileProfile;
    use crate::gameplay::projectiles::{ProjectileBehavior, ProjectilePerturbanceProfile};
    use crate::{EntityClass, EntityId};

    #[test]
    fn inherited_source_visual_and_motion_replace_only_presentation_and_flight() {
        let id = EntityId::new(EntityClass::Projectile, 1);
        let profile = ProjectileProfile {
            proto_object_id: 7,
            proto_object_name: "logical_debris".to_owned(),
            speed: 10.0,
            starting_speed: 10.0,
            fuel: 2.0,
            acceleration: 5.0,
            max_projectile_height: 0.0,
            lifespan: 10.0,
            tracking_delay: 0.5,
            turn_rate_degrees: 90.0,
            perturbance: ProjectilePerturbanceProfile::default(),
            behavior: ProjectileBehavior::TRACKING,
        };
        let mut projectile = Projectile::new(
            id,
            1,
            ProjectileLaunch::test(Vec3::ZERO, Vec3::X * 10.0, 10.0),
            &profile,
        );

        projectile.inherit_source_visual(
            " captured_marine ",
            Some(3),
            2,
            Vec3::new(0.25, 1.0, -0.5),
        );
        projectile.inherit_source_motion(Vec3::new(4.0, 5.0, 6.0), -Vec3::X, 9.8);

        assert_eq!(projectile.proto_object_name, "logical_debris");
        assert_eq!(projectile.visual_proto_object_name(), "captured_marine");
        assert_eq!(projectile.object_state.visual_variation_index(), Some(3));
        assert_eq!(projectile.base.player_id, 2);
        assert_eq!(
            projectile.visual_center_offset(),
            Vec3::new(-0.25, -1.0, 0.5)
        );
        assert_eq!(projectile.created_by_player_id(), 1);
        assert_eq!(projectile.base.velocity, Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(projectile.base.forward, -Vec3::X);
        assert_eq!(projectile.gravity.to_bits(), 9.8_f32.to_bits());
        assert!(projectile.affected_by_gravity);
        assert!(!projectile.tracking);
        assert_eq!(projectile.acceleration.to_bits(), 0.0_f32.to_bits());
        assert_eq!(projectile.fuel.to_bits(), 0.0_f32.to_bits());
    }
}
