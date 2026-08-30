//! Retail ranged-action flags and global attack-facing tolerances.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_3};

use pipeline::database::hw1::tactics::Action;
use pipeline::database::hw1::{Database, ProtoObject};

use super::AttackProfile;

/// Authored switches controlling hardpoint and owner orientation for one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackOrientationProfile {
    flags: AttackOrientationFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AttackOrientationFlags(u8);

impl AttackOrientationFlags {
    const DONT_CHECK: u8 = 1 << 0;
    const CAN_ORIENT_OWNER: u8 = 1 << 1;
    const STATIONARY: u8 = 1 << 2;
    const OWNER_CAN_ROTATE: u8 = 1 << 3;

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

impl Default for AttackOrientationProfile {
    fn default() -> Self {
        Self {
            flags: AttackOrientationFlags(
                AttackOrientationFlags::CAN_ORIENT_OWNER | AttackOrientationFlags::OWNER_CAN_ROTATE,
            ),
        }
    }
}

impl AttackOrientationProfile {
    pub(super) fn from_action(object: &ProtoObject, action: &Action) -> Self {
        let mut profile = Self::default();
        profile.flags.set(
            AttackOrientationFlags::DONT_CHECK,
            action.dont_check_orient_tolerance.unwrap_or(false),
        );
        profile.flags.set(
            AttackOrientationFlags::CAN_ORIENT_OWNER,
            action.can_orient_owner.unwrap_or(true),
        );
        profile.flags.set(
            AttackOrientationFlags::STATIONARY,
            action.stationary.unwrap_or(false),
        );
        profile.flags.set(
            AttackOrientationFlags::OWNER_CAN_ROTATE,
            !has_flag(object, "NonRotatable"),
        );
        profile
    }

    /// Return whether retail bypasses both orientation updates and hit checks.
    #[must_use]
    pub const fn skips_orientation_check(self) -> bool {
        self.flags.contains(AttackOrientationFlags::DONT_CHECK)
    }

    /// Return whether a capped hardpoint may rotate the owning unit.
    #[must_use]
    pub const fn can_orient_owner(self) -> bool {
        self.flags
            .contains(AttackOrientationFlags::CAN_ORIENT_OWNER)
    }

    /// Return whether orientation freezes after the attack enters Working state.
    #[must_use]
    pub const fn is_stationary(self) -> bool {
        self.flags.contains(AttackOrientationFlags::STATIONARY)
    }

    /// Return whether the proto object permits body rotation.
    #[must_use]
    pub const fn owner_can_rotate(self) -> bool {
        self.flags
            .contains(AttackOrientationFlags::OWNER_CAN_ROTATE)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct AttackOrientationTolerances {
    stationary: f32,
    moving: f32,
    tracking_moving: f32,
}

impl AttackOrientationTolerances {
    pub(crate) fn from_database(database: &Database) -> Self {
        let game_data = database.game_data.as_ref();
        Self {
            stationary: finite(
                game_data.and_then(|data| data.stationary_target_attack_tolerance_angle),
            ),
            moving: finite(game_data.and_then(|data| data.moving_target_attack_tolerance_angle)),
            tracking_moving: finite(
                game_data.and_then(|data| data.moving_target_tracking_attack_tolerance_angle),
            ),
        }
    }

    pub(crate) fn dot_tolerance(
        self,
        profile: &AttackProfile,
        target_is_moving: bool,
        ability_target: bool,
    ) -> f32 {
        if ability_target {
            return FRAC_PI_3.cos();
        }
        let has_projectile = profile.projectile.is_some();
        if profile
            .hardpoint
            .as_ref()
            .is_some_and(super::AttackHardpointProfile::uses_retail_fixed_yaw_tolerance)
        {
            let degrees = if target_is_moving && has_projectile {
                self.tracking_moving
            } else {
                self.moving * 0.5
            };
            return degrees.to_radians().cos();
        }
        if !has_projectile {
            return FRAC_PI_2.cos();
        }
        let degrees = if target_is_moving {
            self.tracking_moving
        } else {
            self.stationary
        };
        (degrees * 0.5).to_radians().cos()
    }
}

fn finite(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

fn has_flag(object: &ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::{
        AttackAccuracyProfile, AttackAmmunition, AttackHardpointProfile, ProjectileReactionFlags,
    };

    #[test]
    fn retail_action_defaults_allow_owner_rotation_and_keep_checks_enabled() {
        let profile =
            AttackOrientationProfile::from_action(&ProtoObject::default(), &Action::default());

        assert!(!profile.skips_orientation_check());
        assert!(profile.can_orient_owner());
        assert!(!profile.is_stationary());
        assert!(profile.owner_can_rotate());
    }

    #[test]
    fn explicit_action_and_object_flags_override_retail_defaults() {
        let mut object = ProtoObject::default();
        object.flags.push("NonRotatable".to_owned());
        let action = Action {
            dont_check_orient_tolerance: Some(true),
            can_orient_owner: Some(false),
            stationary: Some(true),
            ..Action::default()
        };
        let profile = AttackOrientationProfile::from_action(&object, &action);

        assert!(profile.skips_orientation_check());
        assert!(!profile.can_orient_owner());
        assert!(profile.is_stationary());
        assert!(!profile.owner_can_rotate());
    }

    #[test]
    fn projectile_melee_and_ability_tolerances_match_retail_angles() {
        let tolerances = tolerances();
        let projectile = attack_profile(Some("bullet"), None);
        let melee = attack_profile(None, None);

        assert_close(
            tolerances.dot_tolerance(&projectile, false, false),
            5.0_f32.to_radians().cos(),
        );
        assert_close(
            tolerances.dot_tolerance(&projectile, true, false),
            15.0_f32.to_radians().cos(),
        );
        assert_close(
            tolerances.dot_tolerance(&melee, false, false),
            FRAC_PI_2.cos(),
        );
        assert_close(
            tolerances.dot_tolerance(&projectile, false, true),
            FRAC_PI_3.cos(),
        );
    }

    #[test]
    fn signed_zero_right_yaw_uses_retails_special_moving_tolerance_branch() {
        let tolerances = tolerances();
        let mut hardpoint = AttackHardpointProfile::for_test(Some("turret"), None);
        hardpoint.yaw_left = -45.0_f32.to_radians();
        hardpoint.yaw_right = 0.0;
        let projectile = attack_profile(Some("bullet"), Some(hardpoint));

        assert_close(
            tolerances.dot_tolerance(&projectile, false, false),
            10.0_f32.to_radians().cos(),
        );
        assert_close(
            tolerances.dot_tolerance(&projectile, true, false),
            30.0_f32.to_radians().cos(),
        );
    }

    fn tolerances() -> AttackOrientationTolerances {
        AttackOrientationTolerances {
            stationary: 10.0,
            moving: 20.0,
            tracking_moving: 30.0,
        }
    }

    fn attack_profile(
        projectile: Option<&str>,
        hardpoint: Option<AttackHardpointProfile>,
    ) -> AttackProfile {
        AttackProfile {
            action_name: "Attack".to_owned(),
            animation_type: "Attack".to_owned(),
            weapon_name: "Weapon".to_owned(),
            weapon_type: None,
            projectile: projectile.map(str::to_owned),
            impact_effect: None,
            area_damage: None,
            pull: None,
            hardpoint,
            orientation: AttackOrientationProfile::default(),
            charged_animation: None,
            friendly_fire: false,
            targets_foot_of_unit: false,
            projectile_reactions: ProjectileReactionFlags::default(),
            max_range: 10.0,
            max_velocity_lead: 0.0,
            accuracy: AttackAccuracyProfile::default(),
            damage_per_attack: 1.0,
            ammunition: AttackAmmunition::None,
            animations: Vec::new(),
            pre_attack_cooldown: [0.0, 0.0],
            post_attack_cooldown: [0.0, 0.0],
            reload_duration: 0.0,
            visual_ammo: 0,
            uses_height_bonus_damage: false,
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= 1.0e-6);
    }
}
