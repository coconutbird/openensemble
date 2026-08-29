//! Projectile entities owned and advanced by the authoritative simulation.

mod flight;
mod perturbance;
mod sticky;

use crate::entities::{BaseEntity, ObjectState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AreaDamageProfile, ProjectileProfile};
use crate::player::PlayerId;
use crate::random::SimRandom;
use crate::sync::SyncChecksum;
use glam::Vec3;
use perturbance::ProjectilePerturbance;
use sticky::ProjectileMotionState;

pub(crate) use flight::launch_target_position;

const MIN_COLLISION_RADIUS: f32 = 0.25;
const DIRECTION_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ProjectileRuntimeFlags(u16);

impl ProjectileRuntimeFlags {
    const FRIENDLY_FIRE: Self = Self(1 << 0);
    const COLLIDES_WITH_ALL_UNITS: Self = Self(1 << 1);
    const SELF_DAMAGE: Self = Self(1 << 2);
    const TUMBLING: Self = Self(1 << 3);
    const TRACKING_PENDING: Self = Self(1 << 4);
    const TESTS_FUEL: Self = Self(1 << 5);
    const CLEARED_LAUNCHER: Self = Self(1 << 6);
    const INTERCEPT_DISTANCE: Self = Self(1 << 7);
    const EXPLODE_ON_TIMER: Self = Self(1 << 8);
    const EXPIRE_ON_TIMER: Self = Self(1 << 9);
    const STICKY: Self = Self(1 << 10);

    fn from_launch(launch: &ProjectileLaunch, profile: &ProjectileProfile) -> Self {
        let mut flags = Self::default();
        flags.set(Self::FRIENDLY_FIRE, launch.friendly_fire);
        flags.set(
            Self::COLLIDES_WITH_ALL_UNITS,
            launch.collides_with_all_units,
        );
        flags.set(Self::SELF_DAMAGE, profile.behavior.self_damage());
        flags.set(Self::TUMBLING, profile.behavior.tumbles());
        flags.set(Self::TRACKING_PENDING, profile.behavior.tracking());
        flags.set(Self::TESTS_FUEL, profile.behavior.tracking());
        flags.set(Self::EXPLODE_ON_TIMER, profile.behavior.explodes_on_timer());
        flags.set(Self::EXPIRE_ON_TIMER, profile.behavior.expires_on_timer());
        flags.set(Self::STICKY, profile.behavior.sticky());
        flags
    }

    const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    fn set(&mut self, flag: Self, enabled: bool) {
        if enabled {
            self.0 |= flag.0;
        } else {
            self.0 &= !flag.0;
        }
    }
}

/// Result of advancing one projectile through an authoritative substep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectileStep {
    /// The projectile remains active.
    Flying,
    /// The projectile reached its intended target location.
    Impact,
    /// The projectile exhausted its authored lifespan.
    Expired,
    /// The projectile reached an authored timer explosion.
    Detonate,
}

/// Launch-time values computed by the firing action.
#[derive(Debug, Clone)]
pub(crate) struct ProjectileLaunch {
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub source_position: Vec3,
    pub target_position: Vec3,
    pub target_entity_position: Vec3,
    pub target_offset: Vec3,
    pub target_radius: f32,
    pub max_range: f32,
    pub damage: f32,
    pub weapon_type: Option<String>,
    pub area_damage: Option<AreaDamageProfile>,
    pub friendly_fire: bool,
    pub collides_with_all_units: bool,
}

#[cfg(test)]
impl ProjectileLaunch {
    fn test(source_position: Vec3, target_position: Vec3, max_range: f32) -> Self {
        Self {
            source_id: EntityId::INVALID,
            target_id: EntityId::INVALID,
            source_position,
            target_position,
            target_entity_position: target_position,
            target_offset: Vec3::ZERO,
            target_radius: 0.0,
            max_range,
            damage: 0.0,
            weapon_type: None,
            area_damage: None,
            friendly_fire: false,
            collides_with_all_units: true,
        }
    }
}

/// Live motion of the concrete entity targeted by a projectile.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectileTargetMotion {
    pub position: Vec3,
    pub velocity: Vec3,
    pub forward: Vec3,
    pub flying: bool,
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
    /// Launch-time sim-center and miss offset retained as the target entity moves.
    target_offset: Vec3,
    /// Damage after attacker and launch-time height modifiers.
    pub damage: f32,
    /// Weapon type applied against the target's damage type on impact.
    pub weapon_type: Option<String>,
    /// Launch-time area-damage values applied at the impact position.
    pub area_damage: Option<AreaDamageProfile>,
    /// Desired flight speed.
    pub desired_speed: f32,
    /// Current scalar speed before gravity is applied.
    pub current_speed: f32,
    /// Forward acceleration.
    pub acceleration: f32,
    /// Remaining authored tracking/acceleration fuel in seconds.
    pub fuel: f32,
    /// Current lifetime in seconds.
    pub age: f32,
    /// Maximum authored lifetime in seconds.
    pub lifespan: f32,
    /// Whether steering toward the live target is currently active.
    pub tracking: bool,
    /// Delay before tracking begins.
    pub tracking_delay: f32,
    /// Maximum steering rate in radians per second.
    pub turn_rate_radians: f32,
    /// Whether global projectile gravity affects flight.
    pub affected_by_gravity: bool,
    /// Per-shot downward acceleration computed by retail's launch solver.
    pub gravity: f32,
    /// Clearance above the simulation terrain retained from the launch point.
    follow_ground_height: f32,
    perturbance: Option<ProjectilePerturbance>,
    motion_state: ProjectileMotionState,
    initial_position: Vec3,
    target_radius: f32,
    runtime_flags: ProjectileRuntimeFlags,
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
        let motion = flight::launch_motion(&launch, profile);
        let forward = motion.velocity.normalize_or(Vec3::Z);
        let current_speed = motion.velocity.length();
        let mut base = BaseEntity::new(id, player_id);
        base.position = launch.source_position;
        base.set_forward(forward);
        base.velocity = motion.velocity;
        let runtime_flags = ProjectileRuntimeFlags::from_launch(&launch, profile);
        Self {
            base,
            object_state: ObjectState::default(),
            proto_object_id: profile.proto_object_id,
            proto_object_name: profile.proto_object_name.clone(),
            source_id: launch.source_id,
            target_id: launch.target_id,
            target_position: launch.target_position,
            target_offset: launch.target_offset,
            damage: launch.damage,
            weapon_type: launch.weapon_type,
            area_damage: launch.area_damage,
            desired_speed: profile.speed,
            current_speed,
            acceleration: profile.acceleration,
            fuel: profile.fuel,
            age: 0.0,
            lifespan: profile.lifespan,
            tracking: false,
            tracking_delay: profile.tracking_delay,
            turn_rate_radians: profile.turn_rate_degrees.to_radians(),
            affected_by_gravity: motion.affected_by_gravity,
            gravity: motion.gravity,
            follow_ground_height: 5.0,
            perturbance: ProjectilePerturbance::from_profile(profile.perturbance),
            motion_state: ProjectileMotionState::Flying,
            initial_position: launch.source_position,
            target_radius: sanitize_radius(launch.target_radius),
            runtime_flags,
            first_update: true,
        }
    }

    /// Advance flight using an optional current target position.
    pub(crate) fn advance(
        &mut self,
        dt: f32,
        live_target: Option<ProjectileTargetMotion>,
        intercept_distance: f32,
    ) -> ProjectileStep {
        self.advance_inner(dt, live_target, intercept_distance, false, None)
    }

    pub(crate) fn advance_authoritative(
        &mut self,
        dt: f32,
        live_target: Option<ProjectileTargetMotion>,
        intercept_distance: f32,
        clear_of_launcher: bool,
        rng: &mut SimRandom,
    ) -> ProjectileStep {
        self.advance_inner(
            dt,
            live_target,
            intercept_distance,
            clear_of_launcher,
            Some(rng),
        )
    }

    fn advance_inner(
        &mut self,
        dt: f32,
        live_target: Option<ProjectileTargetMotion>,
        intercept_distance: f32,
        clear_of_launcher: bool,
        rng: Option<&mut SimRandom>,
    ) -> ProjectileStep {
        if !dt.is_finite() || dt <= 0.0 || !self.base.alive {
            return ProjectileStep::Flying;
        }
        let remaining_life = (self.lifespan - self.age).max(0.0);
        if remaining_life <= f32::EPSILON {
            return self.expire_if_needed();
        }
        let elapsed = dt.min(remaining_life);
        self.age += elapsed;

        // Retail initializes projectile velocity and presents the launch point on
        // its first world update before beginning flight on the following one.
        if self.first_update {
            self.first_update = false;
            return self.expire_if_needed();
        }

        let live_target = live_target.filter(|target| {
            target.position.is_finite() && target.velocity.is_finite() && target.forward.is_finite()
        });
        if !self.is_flying() {
            self.update_non_flying_motion(live_target);
            return self.expire_if_needed();
        }
        self.update_tracking(live_target, intercept_distance);
        self.accelerate(elapsed);
        self.consume_fuel(elapsed);
        if clear_of_launcher {
            self.runtime_flags
                .set(ProjectileRuntimeFlags::CLEARED_LAUNCHER, true);
        }
        let perturbance = self.update_perturbance(elapsed, rng);

        let previous = self.base.position;
        self.move_projectile(elapsed, perturbance);

        if segment_reaches_target(
            previous,
            self.base.position,
            self.target_position,
            self.target_radius,
        ) {
            self.base.position = self.target_position;
            if !self.has_timed_lifecycle() {
                self.base.kill();
            }
            return ProjectileStep::Impact;
        }
        self.expire_if_needed()
    }

    fn accelerate(&mut self, dt: f32) {
        let current_speed = self.base.velocity.length();
        if self.acceleration > 0.0 && current_speed < self.desired_speed {
            let fueled_time = dt.min(self.fuel);
            let next_speed =
                (current_speed + self.acceleration * fueled_time).min(self.desired_speed);
            self.base.velocity = self.base.velocity.normalize_or(self.base.forward) * next_speed;
            self.current_speed = next_speed;
        } else if current_speed >= self.desired_speed {
            self.acceleration = 0.0;
        }
    }

    fn update_tracking(
        &mut self,
        live_target: Option<ProjectileTargetMotion>,
        intercept_distance: f32,
    ) {
        if self
            .runtime_flags
            .contains(ProjectileRuntimeFlags::TRACKING_PENDING)
        {
            let starts_early = live_target.is_some_and(|target| {
                target.position.distance(self.base.position) < 0.4 * self.base.velocity.length()
            });
            if self.age >= self.tracking_delay || starts_early {
                self.runtime_flags
                    .set(ProjectileRuntimeFlags::TRACKING_PENDING, false);
                self.tracking = true;
            }
        }
        if let Some(target) = live_target {
            let target_position = target.position + self.target_offset;
            self.target_position = if self.tracking {
                let tracking_target = flight::tracking_target(
                    self.base.position,
                    target_position,
                    target.velocity,
                    self.base.velocity.length(),
                    intercept_distance,
                );
                if tracking_target.intercepting {
                    self.runtime_flags
                        .set(ProjectileRuntimeFlags::INTERCEPT_DISTANCE, true);
                }
                tracking_target.position
            } else {
                target_position
            };
        } else {
            self.tracking = false;
        }
    }

    fn consume_fuel(&mut self, dt: f32) {
        if !self
            .runtime_flags
            .contains(ProjectileRuntimeFlags::TESTS_FUEL)
        {
            return;
        }
        self.fuel = (self.fuel - dt).max(0.0);
        if self.fuel <= f32::EPSILON {
            self.fuel = 0.0;
            self.tracking = false;
            self.runtime_flags
                .set(ProjectileRuntimeFlags::TRACKING_PENDING, false);
            self.runtime_flags
                .set(ProjectileRuntimeFlags::TESTS_FUEL, false);
            self.acceleration = 0.0;
            self.affected_by_gravity = true;
            self.runtime_flags
                .set(ProjectileRuntimeFlags::TUMBLING, true);
            if let Some(perturbance) = &mut self.perturbance {
                perturbance.disable_after_fuel();
            }
        }
    }

    fn update_perturbance(&mut self, elapsed: f32, rng: Option<&mut SimRandom>) -> Vec3 {
        if !self
            .runtime_flags
            .contains(ProjectileRuntimeFlags::CLEARED_LAUNCHER)
            || self
                .runtime_flags
                .contains(ProjectileRuntimeFlags::INTERCEPT_DISTANCE)
        {
            return Vec3::ZERO;
        }
        if let Some(state) = &mut self.perturbance
            && state.is_active()
        {
            return state.update(elapsed);
        }
        let Some(rng) = rng else {
            return Vec3::ZERO;
        };
        let roll = rng.distribution();
        if let Some(state) = &mut self.perturbance {
            let velocity_ratio = if self.desired_speed > DIRECTION_EPSILON {
                self.base.velocity.length() / self.desired_speed
            } else {
                0.0
            };
            state.attempt_start(roll, velocity_ratio, rng);
        }
        Vec3::ZERO
    }

    fn move_projectile(&mut self, dt: f32, perturbance: Vec3) {
        let facing = if self.tracking {
            let motion = flight::tracking_motion(
                self.base.velocity,
                self.target_position - self.base.position,
                perturbance,
                self.turn_rate_radians,
                dt,
            );
            self.base.velocity = motion.velocity;
            self.base.position += motion.step;
            motion.facing
        } else if self.affected_by_gravity {
            let previous_velocity = self.base.velocity;
            self.base.velocity.y -= self.gravity * dt;
            self.base.position += (previous_velocity + self.base.velocity) * (0.5 * dt);
            self.base.velocity
        } else {
            let step = self.base.velocity * dt;
            self.base.position += step;
            step
        };
        self.current_speed = self.base.velocity.length();
        if !self.is_tumbling() && facing.length_squared() > DIRECTION_EPSILON {
            self.base.set_forward(facing);
        }
    }

    fn expire_if_needed(&mut self) -> ProjectileStep {
        if self.age + f32::EPSILON >= self.lifespan {
            self.base.kill();
            if self.explodes_on_timer() {
                ProjectileStep::Detonate
            } else {
                ProjectileStep::Expired
            }
        } else {
            ProjectileStep::Flying
        }
    }

    pub(crate) fn is_close_to_target(&self) -> bool {
        self.initial_position.distance_squared(self.base.position)
            >= self.initial_position.distance_squared(self.target_position) * 0.95
    }

    pub(crate) const fn initial_position(&self) -> Vec3 {
        self.initial_position
    }

    pub(crate) const fn friendly_fire(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::FRIENDLY_FIRE)
    }

    pub(crate) const fn collides_with_all_units(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::COLLIDES_WITH_ALL_UNITS)
    }

    pub(crate) const fn self_damage(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::SELF_DAMAGE)
    }

    pub(crate) const fn is_tumbling(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::TUMBLING)
    }

    pub(crate) const fn has_cleared_launcher(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::CLEARED_LAUNCHER)
    }

    pub(crate) fn initialize_follow_ground_height(&mut self, terrain_height: Option<f32>) {
        if !self.first_update {
            return;
        }
        self.follow_ground_height = terrain_height
            .filter(|height| height.is_finite())
            .map_or(5.0, |height| {
                (self.base.position.y - height).clamp(0.0, 5.0)
            });
    }

    pub(crate) fn apply_tracking_ground_avoidance(
        &mut self,
        terrain_height: Option<f32>,
        target_flying: bool,
    ) -> bool {
        if !self.tracking || target_flying {
            return false;
        }
        let Some(terrain_height) = terrain_height.filter(|height| height.is_finite()) else {
            return false;
        };
        let fixed_height = terrain_height + self.follow_ground_height;
        if fixed_height <= self.base.position.y {
            return false;
        }
        let offset = self.target_position - self.base.position;
        let xz_distance = offset.x.hypot(offset.z);
        let turn_scale = self.turn_rate_radians.to_degrees().clamp(90.0, 360.0) / 360.0;
        let fixed_height_distance = self.base.velocity.length() * turn_scale;
        if fixed_height_distance > xz_distance {
            return false;
        }
        self.base.position.y = fixed_height;
        true
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
        checksum.hash_vec3(
            self.target_offset.x,
            self.target_offset.y,
            self.target_offset.z,
        );
        checksum.hash_f32(self.damage);
        hash_optional_string(checksum, self.weapon_type.as_deref());
        hash_area_damage(checksum, self.area_damage);
        checksum.hash_u32(u32::from(self.runtime_flags.0));
        checksum.hash_f32(self.desired_speed);
        checksum.hash_f32(self.current_speed);
        checksum.hash_f32(self.acceleration);
        checksum.hash_f32(self.fuel);
        checksum.hash_f32(self.age);
        checksum.hash_f32(self.lifespan);
        checksum.hash_u32(u32::from(self.tracking));
        checksum.hash_f32(self.tracking_delay);
        checksum.hash_f32(self.turn_rate_radians);
        checksum.hash_u32(u32::from(self.affected_by_gravity));
        checksum.hash_f32(self.gravity);
        checksum.hash_f32(self.follow_ground_height);
        self.hash_motion_state(checksum);
        if let Some(perturbance) = &self.perturbance {
            checksum.hash_u32(1);
            perturbance.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_vec3(
            self.initial_position.x,
            self.initial_position.y,
            self.initial_position.z,
        );
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

fn hash_optional_string(checksum: &mut SyncChecksum, value: Option<&str>) {
    if let Some(value) = value {
        checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(value.as_bytes());
    } else {
        checksum.hash_u32(u32::MAX);
    }
}

fn hash_area_damage(checksum: &mut SyncChecksum, profile: Option<AreaDamageProfile>) {
    let Some(profile) = profile else {
        checksum.hash_u32(0);
        return;
    };
    checksum.hash_u32(1);
    checksum.hash_f32(profile.radius);
    checksum.hash_f32(profile.primary_target_factor);
    checksum.hash_f32(profile.distance_factor);
    checksum.hash_f32(profile.damage_factor);
    checksum.hash_u32(u32::from(profile.linear_damage));
    checksum.hash_u32(u32::from(profile.ignores_y_axis));
    checksum.hash_u32(u32::from(profile.friendly_fire));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;
    use crate::gameplay::projectiles::{ProjectileBehavior, ProjectilePerturbanceProfile};

    fn profile() -> ProjectileProfile {
        ProjectileProfile {
            proto_object_id: 17,
            proto_object_name: "test_bullet".to_owned(),
            speed: 10.0,
            starting_speed: 10.0,
            fuel: 0.0,
            acceleration: 0.0,
            max_projectile_height: 0.0,
            lifespan: 2.0,
            tracking_delay: 0.0,
            turn_rate_degrees: 0.0,
            perturbance: ProjectilePerturbanceProfile::default(),
            behavior: ProjectileBehavior::default(),
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
                target_entity_position: Vec3::new(1.0, 0.0, 0.0),
                target_offset: Vec3::ZERO,
                target_radius: 0.1,
                max_range: 10.0,
                damage: 5.0,
                weapon_type: None,
                area_damage: None,
                friendly_fire: false,
                collides_with_all_units: true,
            },
            &profile(),
        );

        assert_eq!(
            projectile.advance(0.05, Some(target_motion(Vec3::X)), 0.0),
            ProjectileStep::Flying
        );
        assert_eq!(projectile.base.position, Vec3::ZERO);
        assert_eq!(
            projectile.advance(0.05, Some(target_motion(Vec3::X)), 0.0),
            ProjectileStep::Flying
        );
        assert_eq!(
            projectile.advance(0.05, Some(target_motion(Vec3::X)), 0.0),
            ProjectileStep::Impact
        );
        assert_eq!(projectile.base.position, Vec3::X);
    }

    #[test]
    fn ballistic_motion_preserves_velocity_and_reaches_the_authored_apex() {
        let mut ballistic = profile();
        ballistic.max_projectile_height = 5.0;
        ballistic.behavior = ProjectileBehavior::AFFECTED_BY_GRAVITY;
        let launch = ProjectileLaunch::test(Vec3::ZERO, Vec3::X * 10.0, 10.0);
        let mut projectile = Projectile::new(
            EntityId::new(EntityClass::Projectile, 0),
            1,
            launch,
            &ballistic,
        );

        let _first = projectile.advance(0.25, None, 0.0);
        let _ascending = projectile.advance(0.25, None, 0.0);
        let _apex = projectile.advance(0.25, None, 0.0);
        assert!((projectile.base.position.x - 5.0).abs() < 0.000_1);
        assert!((projectile.base.position.y - 5.0).abs() < 0.000_1);
        assert!(projectile.base.velocity.y.abs() < 0.000_1);
    }

    #[test]
    fn tracking_starts_early_then_exhausts_fuel_into_tumbling_gravity() {
        let id = EntityId::new(EntityClass::Projectile, 0);
        let source = EntityId::new(EntityClass::Unit, 0);
        let target = EntityId::new(EntityClass::Unit, 1);
        let mut tracking_profile = profile();
        tracking_profile.fuel = 0.2;
        tracking_profile.tracking_delay = 1.0;
        tracking_profile.behavior = ProjectileBehavior::TRACKING;
        let launch = ProjectileLaunch {
            source_id: source,
            target_id: target,
            source_position: Vec3::ZERO,
            target_position: Vec3::X * 10.0,
            target_entity_position: Vec3::X * 10.0,
            target_offset: Vec3::ZERO,
            target_radius: 0.1,
            max_range: 10.0,
            damage: 5.0,
            weapon_type: None,
            area_damage: None,
            friendly_fire: false,
            collides_with_all_units: true,
        };
        let mut projectile = Projectile::new(id, 1, launch, &tracking_profile);

        assert_eq!(
            projectile.advance(0.05, Some(target_motion(Vec3::X * 3.0)), 25.0),
            ProjectileStep::Flying
        );
        assert_eq!(
            projectile.advance(0.05, Some(target_motion(Vec3::X * 3.0)), 25.0),
            ProjectileStep::Flying
        );
        assert!(projectile.tracking);
        assert_eq!(projectile.target_position, Vec3::X * 3.0);

        let _step = projectile.advance(0.2, Some(target_motion(Vec3::X * 3.0)), 25.0);
        assert!(!projectile.tracking);
        assert!(projectile.affected_by_gravity);
        assert!(projectile.is_tumbling());
        assert!(projectile.fuel.abs() < f32::EPSILON);
    }

    #[test]
    fn live_target_updates_preserve_the_launch_deviation_offset() {
        let retained_offset = Vec3::new(0.0, 3.0, 2.0);
        let mut launch = ProjectileLaunch::test(Vec3::ZERO, Vec3::X * 10.0 + retained_offset, 10.0);
        launch.target_offset = retained_offset;
        let mut projectile = Projectile::new(
            EntityId::new(EntityClass::Projectile, 0),
            1,
            launch,
            &profile(),
        );

        let _launch = projectile.advance(0.05, Some(target_motion(Vec3::X * 3.0)), 0.0);
        let _flight = projectile.advance(0.05, Some(target_motion(Vec3::X * 3.0)), 0.0);

        assert_eq!(projectile.target_position, Vec3::X * 3.0 + retained_offset);
    }

    #[test]
    fn cleared_projectile_without_data_still_consumes_the_attempt_roll() {
        let mut projectile = Projectile::new(
            EntityId::new(EntityClass::Projectile, 0),
            1,
            ProjectileLaunch::test(Vec3::ZERO, Vec3::X * 100.0, 100.0),
            &profile(),
        );
        let mut actual = SimRandom::new();
        let mut expected = SimRandom::new();

        let _first = projectile.advance_authoritative(0.1, None, 0.0, true, &mut actual);
        assert_eq!(actual.seed(), expected.seed());
        let _flight = projectile.advance_authoritative(0.1, None, 0.0, true, &mut actual);
        let _attempt = expected.distribution();
        assert_eq!(actual.seed(), expected.seed());
    }

    #[test]
    fn recurring_perturbance_uses_five_start_draws_then_modifies_tracking_step() {
        let mut tracking_profile = profile();
        tracking_profile.fuel = 2.0;
        tracking_profile.tracking_delay = 0.0;
        tracking_profile.turn_rate_degrees = 0.0;
        tracking_profile.behavior = ProjectileBehavior::TRACKING;
        tracking_profile.perturbance = ProjectilePerturbanceProfile {
            chance: 1.0,
            velocity: 10.0,
            min_time: 1.0,
            max_time: 1.0,
            initial: None,
        };
        let mut projectile = Projectile::new(
            EntityId::new(EntityClass::Projectile, 0),
            1,
            ProjectileLaunch::test(Vec3::ZERO, Vec3::X * 100.0, 100.0),
            &tracking_profile,
        );
        let target = Some(target_motion(Vec3::X * 100.0));
        let mut actual = SimRandom::new();
        let mut expected = SimRandom::new();

        let _first = projectile.advance_authoritative(0.1, target, 0.0, true, &mut actual);
        let _start = projectile.advance_authoritative(0.1, target, 0.0, true, &mut actual);
        for _ in 0..5 {
            let _draw = expected.distribution();
        }
        assert_eq!(actual.seed(), expected.seed());
        assert!(projectile.perturbance.as_ref().unwrap().is_active());
        let start = projectile.base.position;
        let rng_after_start = actual.seed();

        let _perturbed = projectile.advance_authoritative(0.5, target, 0.0, true, &mut actual);
        let step = projectile.base.position - start;
        assert!(step.distance(Vec3::X * 5.0) > 0.01);
        assert!((projectile.base.velocity.length() - 10.0).abs() < 0.000_1);
        assert_eq!(actual.seed(), rng_after_start);
    }

    #[test]
    fn intercept_distance_suppresses_perturbance_and_remains_sticky() {
        let mut tracking_profile = profile();
        tracking_profile.fuel = 2.0;
        tracking_profile.behavior = ProjectileBehavior::TRACKING;
        tracking_profile.perturbance = ProjectilePerturbanceProfile {
            chance: 1.0,
            velocity: 10.0,
            min_time: 1.0,
            max_time: 1.0,
            initial: None,
        };
        let mut projectile = Projectile::new(
            EntityId::new(EntityClass::Projectile, 0),
            1,
            ProjectileLaunch::test(Vec3::ZERO, Vec3::X * 100.0, 100.0),
            &tracking_profile,
        );
        let mut rng = SimRandom::new();
        let seed = rng.seed();

        let _first = projectile.advance_authoritative(
            0.1,
            Some(target_motion(Vec3::X * 2.0)),
            5.0,
            true,
            &mut rng,
        );
        let _near = projectile.advance_authoritative(
            0.1,
            Some(target_motion(Vec3::X * 2.0)),
            5.0,
            true,
            &mut rng,
        );
        let _far = projectile.advance_authoritative(
            0.1,
            Some(target_motion(Vec3::X * 100.0)),
            5.0,
            true,
            &mut rng,
        );

        assert_eq!(rng.seed(), seed);
        assert!(
            projectile
                .runtime_flags
                .contains(ProjectileRuntimeFlags::INTERCEPT_DISTANCE)
        );
        assert!(!projectile.perturbance.as_ref().unwrap().is_active());
    }

    #[test]
    fn tracking_ground_avoidance_preserves_launch_clearance_until_turning_distance() {
        let mut tracking_profile = profile();
        tracking_profile.turn_rate_degrees = 180.0;
        tracking_profile.behavior = ProjectileBehavior::TRACKING;
        let mut launch =
            ProjectileLaunch::test(Vec3::new(0.0, 2.0, 0.0), Vec3::new(100.0, 2.0, 0.0), 100.0);
        launch.target_entity_position = launch.target_position;
        let mut projectile = Projectile::new(
            EntityId::new(EntityClass::Projectile, 0),
            1,
            launch,
            &tracking_profile,
        );
        projectile.initialize_follow_ground_height(Some(0.0));
        projectile.tracking = true;
        projectile.base.position = Vec3::new(10.0, 1.0, 0.0);

        assert!(projectile.apply_tracking_ground_avoidance(Some(3.0), false));
        assert!((projectile.base.position.y - 5.0).abs() < f32::EPSILON);

        projectile.base.position = Vec3::new(99.0, 1.0, 0.0);
        assert!(!projectile.apply_tracking_ground_avoidance(Some(3.0), false));
        assert!((projectile.base.position.y - 1.0).abs() < f32::EPSILON);
        projectile.base.position = Vec3::new(10.0, 1.0, 0.0);
        assert!(!projectile.apply_tracking_ground_avoidance(Some(3.0), true));
    }

    fn target_motion(position: Vec3) -> ProjectileTargetMotion {
        ProjectileTargetMotion {
            position,
            velocity: Vec3::ZERO,
            forward: Vec3::Z,
            flying: false,
        }
    }
}
