//! Immutable projectile movement definitions from the layered object database.

use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeMap;

/// Authored one-time velocity perturbation applied after clearing the launcher.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProjectileInitialPerturbance {
    /// Magnitude of the random perturbation vector.
    pub velocity: f32,
    /// Minimum duration in seconds.
    pub min_time: f32,
    /// Maximum duration in seconds.
    pub max_time: f32,
}

/// Authored recurring and one-time projectile perturbation values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProjectilePerturbanceProfile {
    /// Per-update chance of beginning a recurring perturbation.
    pub chance: f32,
    /// Recurring perturbation velocity at full projectile speed.
    pub velocity: f32,
    /// Minimum recurring duration in seconds.
    pub min_time: f32,
    /// Maximum recurring duration in seconds.
    pub max_time: f32,
    /// Optional perturbation that retail starts exactly once.
    pub initial: Option<ProjectileInitialPerturbance>,
}

/// Simulation-relevant values for a projectile proto object.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileProfile {
    /// Database ID, or the database index when no explicit ID is authored.
    pub proto_object_id: i32,
    /// Authored proto-object name.
    pub proto_object_name: String,
    /// Desired flight speed in world units per second.
    pub speed: f32,
    /// Initial speed for accelerating projectiles.
    pub starting_speed: f32,
    /// Authored fuel duration used by acceleration and tracking.
    pub fuel: f32,
    /// Forward acceleration while fuel remains.
    pub acceleration: f32,
    /// Maximum authored ballistic height at full weapon range.
    pub max_projectile_height: f32,
    /// Maximum lifetime in seconds.
    pub lifespan: f32,
    /// Delay before tracking begins, in seconds.
    pub tracking_delay: f32,
    /// Maximum authored turn rate in degrees per second.
    pub turn_rate_degrees: f32,
    /// Authored synchronized velocity perturbation behavior.
    pub perturbance: ProjectilePerturbanceProfile,
    pub(crate) behavior: ProjectileBehavior,
}

/// Compact authored projectile behavior flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ProjectileBehavior(u16);

impl ProjectileProfile {
    /// Return whether this projectile can begin tracking a live target.
    #[must_use]
    pub const fn tracks_target(&self) -> bool {
        self.behavior.tracking()
    }

    /// Return whether global projectile gravity affects this prototype.
    #[must_use]
    pub const fn is_affected_by_gravity(&self) -> bool {
        self.behavior.affected_by_gravity()
    }

    /// Return whether this projectile begins in retail's tumbling state.
    #[must_use]
    pub const fn tumbles(&self) -> bool {
        self.behavior.tumbles()
    }

    /// Return whether collision filtering permits the launching unit.
    #[must_use]
    pub const fn allows_self_damage(&self) -> bool {
        self.behavior.self_damage()
    }

    /// Return whether impact is deferred until the authored lifespan expires.
    #[must_use]
    pub const fn explodes_on_timer(&self) -> bool {
        self.behavior.explodes_on_timer()
    }

    /// Return whether the projectile lingers until its lifespan without exploding.
    #[must_use]
    pub const fn expires_on_timer(&self) -> bool {
        self.behavior.expires_on_timer()
    }

    /// Return whether the projectile attaches to a struck unit or rests on terrain.
    #[must_use]
    pub const fn is_sticky(&self) -> bool {
        self.behavior.sticky()
    }
}

impl ProjectileBehavior {
    pub(crate) const TRACKING: Self = Self(1 << 0);
    pub(crate) const AFFECTED_BY_GRAVITY: Self = Self(1 << 1);
    pub(crate) const TUMBLING: Self = Self(1 << 2);
    const SELF_DAMAGE: Self = Self(1 << 3);
    pub(crate) const EXPLODE_ON_TIMER: Self = Self(1 << 4);
    pub(crate) const EXPIRE_ON_TIMER: Self = Self(1 << 5);
    pub(crate) const STICKY: Self = Self(1 << 6);

    fn from_proto(object: &ProtoObject) -> Self {
        let mut flags = Self::default();
        flags.set(Self::TRACKING, has_flag(object, "Tracking"));
        flags.set(
            Self::AFFECTED_BY_GRAVITY,
            has_flag(object, "IsAffectedByGravity"),
        );
        flags.set(Self::TUMBLING, has_flag(object, "ProjectileTumbles"));
        flags.set(Self::SELF_DAMAGE, has_flag(object, "SelfDamage"));
        flags.set(Self::EXPLODE_ON_TIMER, has_flag(object, "ExplodeOnTimer"));
        flags.set(Self::EXPIRE_ON_TIMER, has_flag(object, "ExpireOnTimer"));
        flags.set(Self::STICKY, has_flag(object, "IsSticky"));
        flags
    }

    pub(crate) const fn tracking(self) -> bool {
        self.contains(Self::TRACKING)
    }

    pub(crate) const fn affected_by_gravity(self) -> bool {
        self.contains(Self::AFFECTED_BY_GRAVITY)
    }

    pub(crate) const fn tumbles(self) -> bool {
        self.contains(Self::TUMBLING)
    }

    pub(crate) const fn self_damage(self) -> bool {
        self.contains(Self::SELF_DAMAGE)
    }

    pub(crate) const fn explodes_on_timer(self) -> bool {
        self.contains(Self::EXPLODE_ON_TIMER)
    }

    pub(crate) const fn expires_on_timer(self) -> bool {
        self.contains(Self::EXPIRE_ON_TIMER)
    }

    pub(crate) const fn sticky(self) -> bool {
        self.contains(Self::STICKY)
    }

    #[cfg(test)]
    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    fn set(&mut self, flag: Self, enabled: bool) {
        if enabled {
            self.0 |= flag.0;
        }
    }
}

pub(super) fn collect_projectile_profiles(
    database: &Database,
) -> BTreeMap<String, ProjectileProfile> {
    database
        .objects
        .iter()
        .enumerate()
        .filter(|(_, object)| is_projectile(object))
        .map(|(index, object)| {
            let speed = positive_or(object.velocity, 1.0);
            let fuel = nonnegative_or_zero(object.fuel);
            let profile = ProjectileProfile {
                proto_object_id: object
                    .dbid
                    .unwrap_or_else(|| i32::try_from(index).unwrap_or(-1)),
                proto_object_name: object.name.clone(),
                speed,
                starting_speed: positive_or(object.starting_velocity, speed),
                fuel,
                acceleration: if fuel > f32::EPSILON {
                    nonnegative_or_zero(object.acceleration)
                } else {
                    0.0
                },
                max_projectile_height: nonnegative_or_zero(object.max_projectile_height),
                lifespan: positive_or(object.lifespan, 10.0),
                tracking_delay: nonnegative_or_zero(object.tracking_delay),
                turn_rate_degrees: nonnegative_or_zero(object.turn_rate),
                perturbance: perturbance_profile(object),
                behavior: ProjectileBehavior::from_proto(object),
            };
            (object.name.to_ascii_lowercase(), profile)
        })
        .collect()
}

fn perturbance_profile(object: &ProtoObject) -> ProjectilePerturbanceProfile {
    ProjectilePerturbanceProfile {
        chance: nonnegative_or_zero(object.perturbance_chance),
        velocity: nonnegative_or_zero(object.perturbance_velocity),
        min_time: nonnegative_or_zero(object.perturbance_min_time),
        max_time: nonnegative_or_zero(object.perturbance_max_time),
        initial: object.perturb_initial_velocity.as_ref().map(|initial| {
            ProjectileInitialPerturbance {
                velocity: finite_nonnegative(initial.velocity),
                min_time: nonnegative_or_zero(initial.min_time),
                max_time: nonnegative_or_zero(initial.max_time),
            }
        }),
    }
}

fn is_projectile(object: &ProtoObject) -> bool {
    object
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Projectile"))
}

fn has_flag(object: &ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(expected))
}

fn positive_or(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(fallback)
}

fn nonnegative_or_zero(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
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

    #[test]
    fn projectile_profiles_are_case_insensitive_and_preserve_motion_flags() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "Test_Rocket".to_owned(),
            dbid: Some(42),
            object_class: Some("Projectile".to_owned()),
            flags: vec![
                "tracking".to_owned(),
                "IsAffectedByGravity".to_owned(),
                "ExplodeOnTimer".to_owned(),
                "IsSticky".to_owned(),
            ],
            velocity: Some(65.0),
            starting_velocity: Some(20.0),
            fuel: Some(1.0),
            acceleration: Some(40.0),
            max_projectile_height: Some(8.0),
            lifespan: Some(2.0),
            tracking_delay: Some(0.3),
            turn_rate: Some(680.0),
            perturbance_chance: Some(0.15),
            perturbance_velocity: Some(15.0),
            perturbance_min_time: Some(0.01),
            perturbance_max_time: Some(0.05),
            ..ProtoObject::default()
        });

        let profiles = collect_projectile_profiles(&database);
        let profile = profiles.get("test_rocket").expect("projectile profile");
        assert_eq!(profile.proto_object_id, 42);
        assert!((profile.speed - 65.0).abs() < f32::EPSILON);
        assert!((profile.starting_speed - 20.0).abs() < f32::EPSILON);
        assert!((profile.fuel - 1.0).abs() < f32::EPSILON);
        assert!((profile.acceleration - 40.0).abs() < f32::EPSILON);
        assert!((profile.max_projectile_height - 8.0).abs() < f32::EPSILON);
        assert!(profile.behavior.tracking());
        assert!(profile.behavior.affected_by_gravity());
        assert!(profile.explodes_on_timer());
        assert!(!profile.expires_on_timer());
        assert!(profile.is_sticky());
        assert!((profile.perturbance.chance - 0.15).abs() < f32::EPSILON);
        assert!((profile.perturbance.velocity - 15.0).abs() < f32::EPSILON);
        assert!((profile.perturbance.min_time - 0.01).abs() < f32::EPSILON);
        assert!((profile.perturbance.max_time - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn initial_perturbance_presence_and_attributes_are_preserved() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "needler".to_owned(),
            object_class: Some("Projectile".to_owned()),
            perturb_initial_velocity: Some(
                pipeline::database::hw1::objects::PerturbInitialVelocity {
                    velocity: 20.0,
                    min_time: Some(0.02),
                    max_time: Some(0.08),
                },
            ),
            ..ProtoObject::default()
        });

        let initial = collect_projectile_profiles(&database)["needler"]
            .perturbance
            .initial
            .expect("authored one-time perturbance");
        assert!((initial.velocity - 20.0).abs() < f32::EPSILON);
        assert!((initial.min_time - 0.02).abs() < f32::EPSILON);
        assert!((initial.max_time - 0.08).abs() < f32::EPSILON);
    }

    #[test]
    fn projectile_acceleration_is_disabled_without_fuel() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "fuel_less".to_owned(),
            object_class: Some("Projectile".to_owned()),
            acceleration: Some(40.0),
            ..ProtoObject::default()
        });

        let profile = collect_projectile_profiles(&database)["fuel_less"].clone();
        assert!(profile.fuel.abs() < f32::EPSILON);
        assert!(profile.acceleration.abs() < f32::EPSILON);
    }
}
