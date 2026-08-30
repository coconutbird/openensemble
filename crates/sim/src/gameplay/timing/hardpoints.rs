//! Immutable hardpoint rules selected by one ranged weapon.

use std::f32::consts::PI;

use pipeline::database::hw1::ProtoObject;
use pipeline::database::hw1::objects::Hardpoint;
use pipeline::database::hw1::tactics::Weapon;

/// Retail hardpoint limits and component attachment names for one attack.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackHardpointProfile {
    pub(crate) name: String,
    pub(crate) yaw_attachment: Option<String>,
    pub(crate) pitch_attachment: Option<String>,
    pub(crate) yaw_rate: f32,
    pub(crate) pitch_rate: f32,
    pub(crate) yaw_left: f32,
    pub(crate) yaw_right: f32,
    pub(crate) pitch_min: f32,
    pub(crate) pitch_max: f32,
    pub(crate) single_bone: Option<String>,
    flags: HardpointFlags,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HardpointFlags(u8);

impl HardpointFlags {
    const COMBINED: u8 = 1 << 0;
    const HARD_PITCH_LIMITS: u8 = 1 << 1;
    const AUTO_CENTER: u8 = 1 << 2;
    const SINGLE_BONE_IK: u8 = 1 << 3;
    const RELATIVE_TO_UNIT: u8 = 1 << 4;
    const ANGLES_AS_TOLERANCE: u8 = 1 << 5;
    const PRESERVE_YAW: u8 = 1 << 6;

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

impl AttackHardpointProfile {
    pub(super) fn from_weapon(object: &ProtoObject, weapon: &Weapon) -> Option<Self> {
        let requested = nonempty(weapon.hardpoint.as_deref())?;
        let hardpoint = object
            .hardpoints
            .iter()
            .find(|hardpoint| hardpoint.name.eq_ignore_ascii_case(requested))?;
        Some(Self::from_authored(
            hardpoint,
            object.single_bone_ik.first().map(String::as_str),
        ))
    }

    fn from_authored(hardpoint: &Hardpoint, single_bone: Option<&str>) -> Self {
        let yaw_attachment = owned_nonempty(hardpoint.yaw_attachment.as_deref());
        let pitch_attachment = owned_nonempty(hardpoint.pitch_attachment.as_deref());
        let tolerance = hardpoint.use_yaw_and_pitch_as_tolerance.unwrap_or(false);
        let same_attachment = yaw_attachment.as_deref().is_some_and(|yaw| {
            pitch_attachment
                .as_deref()
                .is_some_and(|pitch| yaw.eq_ignore_ascii_case(pitch))
        });
        let yaw_max = degrees_or(hardpoint.yaw_max_angle, PI);
        let pitch_max = degrees_or(hardpoint.pitch_max_angle, PI);
        let mut flags = HardpointFlags::default();
        flags.set(
            HardpointFlags::COMBINED,
            hardpoint.combined.unwrap_or(false) || same_attachment && !tolerance,
        );
        flags.set(
            HardpointFlags::HARD_PITCH_LIMITS,
            hardpoint.hard_pitch_limits.unwrap_or(false),
        );
        flags.set(
            HardpointFlags::AUTO_CENTER,
            hardpoint.autocenter.unwrap_or(true),
        );
        flags.set(
            HardpointFlags::SINGLE_BONE_IK,
            hardpoint.single_bone_ik.unwrap_or(false),
        );
        flags.set(
            HardpointFlags::RELATIVE_TO_UNIT,
            hardpoint.relative_to_unit.unwrap_or(false),
        );
        flags.set(HardpointFlags::ANGLES_AS_TOLERANCE, tolerance);
        flags.set(
            HardpointFlags::PRESERVE_YAW,
            hardpoint.infinite_rate_when_has_target.unwrap_or(false),
        );
        Self {
            name: hardpoint.name.clone(),
            yaw_attachment,
            pitch_attachment,
            yaw_rate: degrees_or(hardpoint.yaw_rate, PI / 12.0),
            pitch_rate: degrees_or(hardpoint.pitch_rate, PI / 12.0),
            yaw_left: degrees_or(hardpoint.yaw_left_max_angle, -yaw_max),
            yaw_right: degrees_or(hardpoint.yaw_right_max_angle, yaw_max),
            pitch_min: degrees_or(hardpoint.pitch_min_angle, -pitch_max),
            pitch_max,
            single_bone: owned_nonempty(single_bone),
            flags,
        }
    }

    pub(crate) const fn is_combined(&self) -> bool {
        self.flags.contains(HardpointFlags::COMBINED)
    }

    pub(crate) const fn has_hard_pitch_limits(&self) -> bool {
        self.flags.contains(HardpointFlags::HARD_PITCH_LIMITS)
    }

    pub(crate) const fn auto_centers(&self) -> bool {
        self.flags.contains(HardpointFlags::AUTO_CENTER)
    }

    pub(crate) const fn uses_single_bone_ik(&self) -> bool {
        self.flags.contains(HardpointFlags::SINGLE_BONE_IK)
    }

    pub(crate) const fn is_relative_to_unit(&self) -> bool {
        self.flags.contains(HardpointFlags::RELATIVE_TO_UNIT)
    }

    pub(crate) const fn uses_angles_as_tolerance(&self) -> bool {
        self.flags.contains(HardpointFlags::ANGLES_AS_TOLERANCE)
    }

    pub(crate) const fn preserves_yaw_on_unit_turn(&self) -> bool {
        self.flags.contains(HardpointFlags::PRESERVE_YAW)
    }

    pub(crate) const fn uses_retail_fixed_yaw_tolerance(&self) -> bool {
        const FLOAT_COMPARE_EPSILON: f32 = 1.0e-6;
        self.yaw_left < FLOAT_COMPARE_EPSILON && self.yaw_right < FLOAT_COMPARE_EPSILON
    }

    #[cfg(test)]
    pub(crate) fn set_uses_angles_as_tolerance(&mut self, enabled: bool) {
        self.flags.set(HardpointFlags::ANGLES_AS_TOLERANCE, enabled);
    }

    #[cfg(test)]
    pub(crate) fn set_preserves_yaw_on_unit_turn(&mut self, enabled: bool) {
        self.flags.set(HardpointFlags::PRESERVE_YAW, enabled);
    }

    #[cfg(test)]
    pub(crate) fn set_uses_single_bone_ik(&mut self, enabled: bool) {
        self.flags.set(HardpointFlags::SINGLE_BONE_IK, enabled);
    }

    #[cfg(test)]
    pub(crate) fn for_test(yaw: Option<&str>, pitch: Option<&str>) -> Self {
        let mut flags = HardpointFlags::default();
        flags.set(HardpointFlags::AUTO_CENTER, true);
        Self {
            name: "test".to_owned(),
            yaw_attachment: yaw.map(str::to_owned),
            pitch_attachment: pitch.map(str::to_owned),
            yaw_rate: PI / 2.0,
            pitch_rate: PI / 2.0,
            yaw_left: -PI,
            yaw_right: PI,
            pitch_min: -PI,
            pitch_max: PI,
            single_bone: None,
            flags,
        }
    }
}

fn degrees_or(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite())
        .map_or(fallback, f32::to_radians)
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn owned_nonempty(value: Option<&str>) -> Option<String> {
    nonempty(value).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn authored_degrees_and_same_component_combination_match_retail_load() {
        let hardpoint = Hardpoint {
            name: "Turret".to_owned(),
            yaw_attachment: Some("gun".to_owned()),
            pitch_attachment: Some("GUN".to_owned()),
            yaw_rate: Some(180.0),
            yaw_max_angle: Some(90.0),
            pitch_min_angle: Some(-30.0),
            pitch_max_angle: Some(60.0),
            ..Hardpoint::default()
        };

        let profile = AttackHardpointProfile::from_authored(&hardpoint, Some("Bip01 Spine"));

        assert!((profile.yaw_rate - PI).abs() <= f32::EPSILON);
        assert!((profile.yaw_left + FRAC_PI_2).abs() <= f32::EPSILON);
        assert!((profile.pitch_min + PI / 6.0).abs() <= f32::EPSILON);
        assert!((profile.pitch_max - PI / 3.0).abs() <= f32::EPSILON);
        assert!(profile.is_combined());
        assert!(profile.auto_centers());
        assert_eq!(profile.single_bone.as_deref(), Some("Bip01 Spine"));
        assert!(!profile.preserves_yaw_on_unit_turn());
    }
}
