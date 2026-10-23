//! The data of the physics deaths: the throw values of a weapon (the `IDamageInfo` getters of
//! `BProtoAction`, 2008 `tactic.cpp:2660-2760`), the proto objects with `RocketOnDeath` and the
//! rocket death chance of `deathmanager.xml`.

use super::GameplayCatalog;
use pipeline::database::hw1::tactics::Weapon;
use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeSet;

/// The physics throw values of the weapon of a damage (`BWeapon` static data).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DeathThrowProfile {
    /// `ThrowUnits`: a kill throws the physics replacement of the target.
    pub throw_units: bool,
    /// `PhysicsForceMin`.
    pub force_min: f32,
    /// `PhysicsForceMax`.
    pub force_max: f32,
    /// `PhysicsLaunchAngleMin` in degrees.
    pub launch_angle_min: f32,
    /// `PhysicsLaunchAngleMax` in degrees.
    pub launch_angle_max: f32,
    /// `PhysicsForceMaxAngle` in radians: a larger angle between the damage direction and the
    /// direction to the target lowers the force. 0 turns the rule off.
    pub force_max_angle: f32,
    /// `PhysicsLaunchAxial`: the throw follows the velocity of the attacker.
    pub launch_axial: bool,
    /// `FlailThrownUnits`: a thrown clamshell flails until its first contact.
    pub flail_thrown_units: bool,
}

impl DeathThrowProfile {
    /// The throw values of a weapon. A missing value is 0 or `false`, the defaults of
    /// `BWeaponStatic` (2008 `tactic.cpp:77-81`).
    #[must_use]
    pub fn from_weapon(weapon: &Weapon) -> Self {
        let value = |value: Option<f32>| value.filter(|value| value.is_finite()).unwrap_or(0.0);
        Self {
            throw_units: weapon.throw_units == Some(true),
            force_min: value(weapon.physics_force_min),
            force_max: value(weapon.physics_force_max),
            launch_angle_min: value(weapon.physics_launch_angle_min),
            launch_angle_max: value(weapon.physics_launch_angle_max),
            force_max_angle: value(weapon.physics_force_max_angle),
            launch_axial: weapon.physics_launch_axial == Some(true),
            flail_thrown_units: weapon.flail_thrown_units == Some(true),
        }
    }
}

/// One `<DeathSpecial>` chance (`BDeathSpecial`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DeathChance {
    /// `mMaxChance`: the start chance and the upper limit.
    pub max_chance: f32,
    /// `mMinChance`: the chance after a success.
    pub min_chance: f32,
    /// `mChanceGrowth`: the chance that a failed roll adds.
    pub chance_growth: f32,
}

/// The physics death data of the catalog.
#[derive(Clone, Debug, Default)]
pub(super) struct DeathThrowCatalog {
    /// The lowercase names of the proto objects with the `RocketOnDeath` flag.
    rocket_on_death: BTreeSet<String>,
    /// The first death special (`cDeathSpecialRocket` = 0 indexes the list).
    rocket_death: Option<DeathChance>,
}

impl DeathThrowCatalog {
    pub(super) fn from_database(database: &Database) -> Self {
        Self {
            rocket_on_death: database
                .objects
                .iter()
                .filter(|object| has_flag(object, "RocketOnDeath"))
                .map(|object| object.name.to_ascii_lowercase())
                .collect(),
            rocket_death: database.death_specials.first().map(|special| DeathChance {
                max_chance: special.max_chance,
                min_chance: special.min_chance,
                chance_growth: special.chance_growth,
            }),
        }
    }
}

impl GameplayCatalog {
    /// `true` for a proto object with the `RocketOnDeath` flag.
    #[must_use]
    pub fn rocket_on_death(&self, proto_object_name: &str) -> bool {
        self.death_throws
            .rocket_on_death
            .contains(&proto_object_name.to_ascii_lowercase())
    }

    /// The rocket death chance of `deathmanager.xml`, or `None` without the file.
    #[must_use]
    pub const fn rocket_death_chance(&self) -> Option<DeathChance> {
        self.death_throws.rocket_death
    }

    #[cfg(test)]
    pub(crate) fn set_test_rocket_death(
        &mut self,
        proto_object_name: &str,
        chance: Option<DeathChance>,
    ) {
        self.death_throws
            .rocket_on_death
            .insert(proto_object_name.to_ascii_lowercase());
        self.death_throws.rocket_death = chance;
    }
}

fn has_flag(object: &ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}
