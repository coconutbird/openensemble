//! Projectile entities owned and advanced by the authoritative simulation.

use crate::entities::{BaseEntity, ObjectState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::ProjectileProfile;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;
use glam::{Quat, Vec3};

const MIN_COLLISION_RADIUS: f32 = 0.25;
const DIRECTION_EPSILON: f32 = 0.000_001;

/// Result of advancing one projectile through an authoritative substep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectileStep {
    /// The projectile remains active.
    Flying,
    /// The projectile reached its intended target location.
    Impact,
    /// The projectile exhausted its authored lifespan.
    Expired,
}

/// Launch-time values computed by the firing action.
#[derive(Debug, Clone)]
pub(crate) struct ProjectileLaunch {
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub source_position: Vec3,
    pub target_position: Vec3,
    pub target_radius: f32,
    pub damage: f32,
    pub weapon_type: Option<String>,
}

/// A launched projectile carrying immutable hit data and live flight state.
#[derive(Debug, Clone)]
pub struct Projectile {
    /// Common entity state used directly by presentation systems.
    pub base: BaseEntity,
    /// Runtime state inherited from retail `BObject`.
    pub object_state: ObjectState,
    /// Database proto-object ID, or `-1` when unresolved.
    pub proto_object_id: i32,
    /// Proto-object name retained for rendering and checksums.
    pub proto_object_name: String,
    /// Unit that launched this projectile.
    pub source_id: EntityId,
    /// Concrete target unit selected when the shot was launched.
    pub target_id: EntityId,
    /// Last known destination, updated by tracking projectiles.
    pub target_position: Vec3,
    /// Damage after attacker and launch-time height modifiers.
    pub damage: f32,
    /// Weapon type applied against the target's damage type on impact.
    pub weapon_type: Option<String>,
    /// Desired flight speed.
    pub desired_speed: f32,
    /// Current scalar speed before gravity is applied.
    pub current_speed: f32,
    /// Forward acceleration.
    pub acceleration: f32,
    /// Current lifetime in seconds.
    pub age: f32,
    /// Maximum authored lifetime in seconds.
    pub lifespan: f32,
    /// Whether the destination follows a live target.
    pub tracking: bool,
    /// Delay before tracking begins.
    pub tracking_delay: f32,
    /// Maximum steering rate in radians per second.
    pub turn_rate_radians: f32,
    /// Whether global projectile gravity affects flight.
    pub affected_by_gravity: bool,
    target_radius: f32,
    first_update: bool,
}

impl Projectile {
    /// Create a launched projectile from immutable scenario-layered data.
    #[must_use]
    pub(crate) fn new(
        id: EntityId,
        player_id: PlayerId,
        launch: ProjectileLaunch,
        profile: &ProjectileProfile,
    ) -> Self {
        let forward = (launch.target_position - launch.source_position).normalize_or(Vec3::Z);
        let current_speed = if profile.acceleration > 0.0 {
            profile.starting_speed
        } else {
            profile.speed
        };
        let mut base = BaseEntity::new(id, player_id);
        base.position = launch.source_position;
        base.set_forward(forward);
        base.velocity = forward * current_speed;
        Self {
            base,
            object_state: ObjectState::default(),
            proto_object_id: profile.proto_object_id,
            proto_object_name: profile.proto_object_name.clone(),
            source_id: launch.source_id,
            target_id: launch.target_id,
            target_position: launch.target_position,
            damage: launch.damage,
            weapon_type: launch.weapon_type,
            desired_speed: profile.speed,
            current_speed,
            acceleration: profile.acceleration,
            age: 0.0,
            lifespan: profile.lifespan,
            tracking: profile.tracking,
            tracking_delay: profile.tracking_delay,
            turn_rate_radians: profile.turn_rate_degrees.to_radians(),
            affected_by_gravity: profile.affected_by_gravity,
            target_radius: sanitize_radius(launch.target_radius),
            first_update: true,
        }
    }

    /// Advance flight using an optional current target position.
    pub(crate) fn advance(
        &mut self,
        dt: f32,
        live_target_position: Option<Vec3>,
        gravity: f32,
    ) -> ProjectileStep {
        if !dt.is_finite() || dt <= 0.0 || !self.base.alive {
            return ProjectileStep::Flying;
        }
        let remaining_life = (self.lifespan - self.age).max(0.0);
        if remaining_life <= f32::EPSILON {
            self.base.kill();
            return ProjectileStep::Expired;
        }
        let elapsed = dt.min(remaining_life);
        self.age += elapsed;

        // Retail initializes projectile velocity and presents the launch point on
        // its first world update before beginning flight on the following one.
        if self.first_update {
            self.first_update = false;
            return self.expire_if_needed();
        }

        if self.tracking
            && self.age >= self.tracking_delay
            && let Some(position) = live_target_position.filter(|position| position.is_finite())
        {
            self.target_position = position;
        }
        self.accelerate(elapsed);
        self.steer(elapsed);

        let previous = self.base.position;
        let mut velocity = self.base.forward * self.current_speed;
        if self.affected_by_gravity && gravity.is_finite() && gravity > 0.0 {
            velocity.y -= gravity * elapsed;
        }
        self.base.velocity = velocity;
        self.base.position += velocity * elapsed;
        if velocity.length_squared() > DIRECTION_EPSILON {
            self.base.set_forward(velocity);
        }

        if segment_reaches_target(
            previous,
            self.base.position,
            self.target_position,
            self.target_radius,
        ) {
            self.base.position = self.target_position;
            self.base.kill();
            return ProjectileStep::Impact;
        }
        self.expire_if_needed()
    }

    fn accelerate(&mut self, dt: f32) {
        if self.acceleration > 0.0 && self.current_speed < self.desired_speed {
            self.current_speed =
                (self.current_speed + self.acceleration * dt).min(self.desired_speed);
        }
    }

    fn steer(&mut self, dt: f32) {
        if !self.tracking || self.age < self.tracking_delay {
            return;
        }
        let Some(desired) = (self.target_position - self.base.position).try_normalize() else {
            return;
        };
        self.base.forward = turn_toward(self.base.forward, desired, self.turn_rate_radians * dt);
    }

    fn expire_if_needed(&mut self) -> ProjectileStep {
        if self.age + f32::EPSILON >= self.lifespan {
            self.base.kill();
            ProjectileStep::Expired
        } else {
            ProjectileStep::Flying
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.proto_object_id);
        checksum.hash_u32(u32::try_from(self.proto_object_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.proto_object_name.as_bytes());
        checksum.hash_u32(self.source_id.as_u32());
        checksum.hash_u32(self.target_id.as_u32());
        checksum.hash_vec3(
            self.target_position.x,
            self.target_position.y,
            self.target_position.z,
        );
        checksum.hash_f32(self.damage);
        hash_optional_string(checksum, self.weapon_type.as_deref());
        checksum.hash_f32(self.desired_speed);
        checksum.hash_f32(self.current_speed);
        checksum.hash_f32(self.acceleration);
        checksum.hash_f32(self.age);
        checksum.hash_f32(self.lifespan);
        checksum.hash_u32(u32::from(self.tracking));
        checksum.hash_f32(self.tracking_delay);
        checksum.hash_f32(self.turn_rate_radians);
        checksum.hash_u32(u32::from(self.affected_by_gravity));
        checksum.hash_f32(self.target_radius);
        checksum.hash_u32(u32::from(self.first_update));
    }
}

impl Entity for Projectile {
    fn id(&self) -> EntityId {
        self.base.id
    }

    fn update(&mut self, dt: f32) {
        let _step = self.advance(dt, None, 0.0);
    }

    fn is_alive(&self) -> bool {
        self.base.is_alive()
    }
}

fn sanitize_radius(radius: f32) -> f32 {
    if radius.is_finite() && radius > 0.0 {
        radius
    } else {
        MIN_COLLISION_RADIUS
    }
}

fn segment_reaches_target(start: Vec3, end: Vec3, target: Vec3, radius: f32) -> bool {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= DIRECTION_EPSILON {
        return start.distance_squared(target) <= radius * radius;
    }
    let fraction = ((target - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    let closest = start + segment * fraction;
    closest.distance_squared(target) <= radius * radius
}

fn turn_toward(current: Vec3, desired: Vec3, maximum_angle: f32) -> Vec3 {
    let current = current.normalize_or(desired);
    if !maximum_angle.is_finite() || maximum_angle <= 0.0 {
        return desired;
    }
    let angle = current.dot(desired).clamp(-1.0, 1.0).acos();
    if angle <= maximum_angle {
        return desired;
    }
    let axis = current.cross(desired).try_normalize().unwrap_or_else(|| {
        let fallback = if current.x.abs() < 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        current.cross(fallback).normalize_or(Vec3::Z)
    });
    (Quat::from_axis_angle(axis, maximum_angle) * current).normalize_or(desired)
}

fn hash_optional_string(checksum: &mut SyncChecksum, value: Option<&str>) {
    if let Some(value) = value {
        checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(value.as_bytes());
    } else {
        checksum.hash_u32(u32::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    fn profile() -> ProjectileProfile {
        ProjectileProfile {
            proto_object_id: 17,
            proto_object_name: "test_bullet".to_owned(),
            speed: 10.0,
            starting_speed: 10.0,
            acceleration: 0.0,
            lifespan: 2.0,
            tracking: false,
            tracking_delay: 0.0,
            turn_rate_degrees: 0.0,
            affected_by_gravity: false,
        }
    }

    #[test]
    fn straight_projectile_presents_launch_point_then_impacts() {
        let id = EntityId::new(EntityClass::Projectile, 0);
        let source = EntityId::new(EntityClass::Unit, 0);
        let target = EntityId::new(EntityClass::Unit, 1);
        let mut projectile = Projectile::new(
            id,
            1,
            ProjectileLaunch {
                source_id: source,
                target_id: target,
                source_position: Vec3::ZERO,
                target_position: Vec3::new(1.0, 0.0, 0.0),
                target_radius: 0.1,
                damage: 5.0,
                weapon_type: None,
            },
            &profile(),
        );

        assert_eq!(
            projectile.advance(0.05, Some(Vec3::X), 0.0),
            ProjectileStep::Flying
        );
        assert_eq!(projectile.base.position, Vec3::ZERO);
        assert_eq!(
            projectile.advance(0.05, Some(Vec3::X), 0.0),
            ProjectileStep::Flying
        );
        assert_eq!(
            projectile.advance(0.05, Some(Vec3::X), 0.0),
            ProjectileStep::Impact
        );
        assert_eq!(projectile.base.position, Vec3::X);
    }

    #[test]
    fn steering_respects_the_authored_turn_limit() {
        let turned = turn_toward(Vec3::X, Vec3::Z, 45.0_f32.to_radians());
        assert!((turned.angle_between(Vec3::X).to_degrees() - 45.0).abs() < 0.001);
    }
}
