//! Projectile-side state transitions for persistent Dodge and Deflect actions.

use super::{Projectile, ProjectileRuntimeFlags};
use crate::gameplay::ProjectileReactionFlags;
use crate::random::SimRandom;
use glam::Vec3;

const DEFLECT_YAW_LIMIT: f32 = std::f32::consts::FRAC_PI_8;
const DIRECTION_EPSILON: f32 = 0.000_001;

impl Projectile {
    pub(crate) fn configure_reactions(&mut self, reactions: ProjectileReactionFlags) {
        self.runtime_flags
            .set(ProjectileRuntimeFlags::DODGEABLE, reactions.dodgeable());
        self.runtime_flags
            .set(ProjectileRuntimeFlags::DEFLECTABLE, reactions.deflectable());
        self.runtime_flags.set(
            ProjectileRuntimeFlags::SMALL_ARMS_DEFLECTABLE,
            reactions.small_arms_deflectable(),
        );
    }

    pub(crate) const fn is_dodgeable(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::DODGEABLE)
    }

    pub(crate) const fn is_deflectable(&self, small_arms: bool) -> bool {
        let flag = if small_arms {
            ProjectileRuntimeFlags::SMALL_ARMS_DEFLECTABLE
        } else {
            ProjectileRuntimeFlags::DEFLECTABLE
        };
        self.runtime_flags.contains(flag)
    }

    pub(crate) const fn checked_for_defense(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::CHECKED_FOR_DEFENSE)
    }

    pub(crate) fn mark_checked_for_defense(&mut self) {
        self.runtime_flags
            .set(ProjectileRuntimeFlags::CHECKED_FOR_DEFENSE, true);
    }

    pub(crate) const fn abandoned_target(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::IGNORE_TARGET_COLLISIONS)
    }

    pub(crate) const fn was_deflected(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::DEFLECTED)
    }

    pub(crate) fn dodge_target(&mut self, flying_target: bool) {
        self.base.alive = true;
        self.runtime_flags
            .set(ProjectileRuntimeFlags::IGNORE_TARGET_COLLISIONS, true);
        if flying_target {
            self.disable_tracking();
        }
    }

    pub(crate) fn deflect_from(
        &mut self,
        previous: Vec3,
        impact_position: Vec3,
        unit_center: Vec3,
        rng: &mut SimRandom,
    ) {
        let speed = self.base.velocity.length();
        let fallback = -horizontal(self.base.velocity).normalize_or(Vec3::Z);
        let away = horizontal(impact_position - unit_center).normalize_or(fallback);
        let yaw = rng.range_float(-DEFLECT_YAW_LIMIT, DEFLECT_YAW_LIMIT);
        let (sin, cos) = yaw.sin_cos();
        let mut direction = Vec3::new(
            away.x.mul_add(cos, away.z * sin),
            rng.range_float(-1.0, 1.0),
            away.z.mul_add(cos, -away.x * sin),
        )
        .normalize_or(fallback);
        if direction.length_squared() <= DIRECTION_EPSILON {
            direction = fallback;
        }
        self.base.position = previous;
        self.base.velocity = direction * speed;
        self.base.set_forward(direction);
        self.base.alive = true;
        self.current_speed = speed;
        self.fuel = 0.0;
        self.acceleration = 0.0;
        self.perturbance = None;
        self.disable_tracking();
        self.runtime_flags
            .set(ProjectileRuntimeFlags::DEFLECTED, true);
        self.runtime_flags
            .set(ProjectileRuntimeFlags::IGNORE_TARGET_COLLISIONS, true);
    }

    fn disable_tracking(&mut self) {
        self.tracking = false;
        self.runtime_flags
            .set(ProjectileRuntimeFlags::TRACKING_PENDING, false);
        self.runtime_flags
            .set(ProjectileRuntimeFlags::TESTS_FUEL, false);
        self.runtime_flags
            .set(ProjectileRuntimeFlags::INTERCEPT_DISTANCE, false);
    }
}

fn horizontal(vector: Vec3) -> Vec3 {
    Vec3::new(vector.x, 0.0, vector.z)
}
