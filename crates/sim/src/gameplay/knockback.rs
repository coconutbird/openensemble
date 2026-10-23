//! The knockback values of a weapon with `ApplyKnockback` (`BWeapon` static data, read by
//! `BUnitActionRangedAttack::doKnockbackAttack`).

use pipeline::database::hw1::tactics::Weapon;

/// The physics values that a knockback attack draws its launch angle and force from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KnockbackProfile {
    /// `PhysicsLaunchAngleMin` in degrees.
    pub launch_angle_min: f32,
    /// `PhysicsLaunchAngleMax` in degrees.
    pub launch_angle_max: f32,
    /// `PhysicsForceMin`, per unit of target mass.
    pub force_min: f32,
    /// `PhysicsForceMax`, per unit of target mass.
    pub force_max: f32,
}

impl KnockbackProfile {
    /// The knockback values of a weapon, or `None` if the weapon has no `ApplyKnockback`. A
    /// missing value is 0, the default of `BWeaponStatic` (2008 `tactic.cpp:77-81`).
    #[must_use]
    pub fn from_weapon(weapon: &Weapon) -> Option<Self> {
        let value = |value: Option<f32>| value.filter(|value| value.is_finite()).unwrap_or(0.0);
        (weapon.apply_knockback == Some(true)).then(|| Self {
            launch_angle_min: value(weapon.physics_launch_angle_min),
            launch_angle_max: value(weapon.physics_launch_angle_max),
            force_min: value(weapon.physics_force_min),
            force_max: value(weapon.physics_force_max),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_knockback_weapon_has_a_profile() {
        let mut weapon = Weapon {
            physics_launch_angle_min: Some(10.0),
            physics_launch_angle_max: Some(20.0),
            physics_force_min: Some(3.0),
            physics_force_max: Some(f32::NAN),
            ..Weapon::default()
        };
        assert_eq!(KnockbackProfile::from_weapon(&weapon), None);
        weapon.apply_knockback = Some(true);
        assert_eq!(
            KnockbackProfile::from_weapon(&weapon),
            Some(KnockbackProfile {
                launch_angle_min: 10.0,
                launch_angle_max: 20.0,
                force_min: 3.0,
                force_max: 0.0,
            })
        );
    }
}
