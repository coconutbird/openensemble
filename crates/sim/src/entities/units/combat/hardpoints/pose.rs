//! Hardpoint orientation steps over a readable and writable pose.
//!
//! [`UnitHardpointState`] applies each step at once. [`PoseProbe`] keeps the
//! proposed transforms over a borrowed state, so a feasibility check sees the same
//! values as an update without a copy of the state.

use glam::{Mat4, Quat, Vec3};

use super::{
    DIRECTION_TOLERANCE, HardpointAim, ROTATION_EPSILON, UnitHardpointState, canonical_name,
    combined_target, direction_orientation, normalized_or_identity, rotation_of, single_bone_base,
    single_bone_name, target_in_base, turn_toward, valid_transform,
};
use crate::gameplay::{AttackAnimationAnchor, AttackHardpointProfile};
use crate::math;

/// Attachment and bone transforms that the orientation steps read and write.
pub(super) trait HardpointPose {
    fn attachment_transform(&self, name: &str) -> Option<Mat4>;

    fn bone_transform(&self, name: &str) -> Option<Mat4>;

    fn set_attachment_rotation(&mut self, name: &str, rotation: Quat);

    fn set_bone_rotation(&mut self, name: &str, rotation: Quat);

    fn attachment_rotation(&self, name: &str) -> Quat {
        rotation_of(self.attachment_transform(name).unwrap_or(Mat4::IDENTITY))
    }

    fn bone_rotation(&self, name: &str) -> Quat {
        rotation_of(self.bone_transform(name).unwrap_or(Mat4::IDENTITY))
    }

    fn orient_target(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> HardpointAim {
        if !target_world.is_finite() || !valid_transform(unit_world) {
            return HardpointAim::HANDLED;
        }
        if profile.uses_single_bone_ik() {
            return self.orient_single_bone(profile, anchor, unit_world, target_world, elapsed);
        }
        let Some(anchor) = anchor else {
            return if profile.yaw_attachment.is_none() && profile.pitch_attachment.is_none() {
                HardpointAim::HANDLED
            } else {
                HardpointAim::BODY_HANDLED
            };
        };
        if profile.is_combined() {
            self.orient_combined(profile, anchor, unit_world, target_world, elapsed)
        } else {
            self.orient_separate(profile, anchor, unit_world, target_world, elapsed)
        }
    }

    fn orient_separate(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: &AttackAnimationAnchor,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> HardpointAim {
        let yaw = profile
            .yaw_attachment
            .as_deref()
            .map_or(HardpointAim::HANDLED, |component| {
                self.orient_yaw(
                    profile,
                    anchor,
                    component,
                    unit_world,
                    target_world,
                    elapsed,
                )
            });
        let pitch =
            profile
                .pitch_attachment
                .as_deref()
                .map_or(HardpointAim::HANDLED, |component| {
                    self.orient_pitch(
                        profile,
                        anchor,
                        component,
                        unit_world,
                        target_world,
                        elapsed,
                    )
                });
        HardpointAim {
            can_face: yaw.can_face && pitch.can_face,
            oriented: if profile.yaw_attachment.is_some() {
                yaw.oriented
            } else {
                pitch.oriented
            },
        }
    }

    fn orient_yaw(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: &AttackAnimationAnchor,
        component: &str,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> HardpointAim {
        let Some(base) = self.component_base(anchor, component) else {
            return HardpointAim::BODY_HANDLED;
        };
        let Some(mut target) = target_in_base(unit_world * base, target_world) else {
            return HardpointAim::HANDLED;
        };
        target.y = 0.0;
        let Some(actual_direction) = target.try_normalize() else {
            return HardpointAim::HANDLED;
        };
        let desired_angle = math::atan2(actual_direction.x, actual_direction.z);
        let clamped_angle = desired_angle.clamp(profile.yaw_left, profile.yaw_right);
        let capped = (desired_angle - clamped_angle).abs() > ROTATION_EPSILON;
        let goal = math::quat_from_rotation_y(clamped_angle);
        let current = self.attachment_rotation(component);
        if (current * Vec3::Z).dot(actual_direction) >= DIRECTION_TOLERANCE {
            return HardpointAim::HANDLED;
        }
        let desired_direction = goal * Vec3::Z;
        let next = turn_toward(current, goal, desired_direction, profile.yaw_rate, elapsed);
        self.set_attachment_rotation(component, next);
        HardpointAim {
            can_face: !capped,
            oriented: (next * Vec3::Z).dot(actual_direction) >= DIRECTION_TOLERANCE,
        }
    }

    fn orient_pitch(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: &AttackAnimationAnchor,
        component: &str,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> HardpointAim {
        let Some(base) = self.component_base(anchor, component) else {
            return HardpointAim::BODY_HANDLED;
        };
        let Some(target) = target_in_base(unit_world * base, target_world) else {
            return HardpointAim::HANDLED;
        };
        let planar = math::hypot(target.x, target.z);
        let desired_angle = math::atan2(target.y, planar);
        let clamped_angle = desired_angle.clamp(profile.pitch_min, profile.pitch_max);
        let capped = (desired_angle - clamped_angle).abs() > ROTATION_EPSILON;
        let actual_direction = Vec3::new(0.0, target.y, planar)
            .try_normalize()
            .unwrap_or(Vec3::Z);
        let goal = math::quat_from_rotation_x(-clamped_angle);
        let current = self.attachment_rotation(component);
        if (current * Vec3::Z).dot(actual_direction) >= DIRECTION_TOLERANCE {
            return HardpointAim::HANDLED;
        }
        let desired_direction = goal * Vec3::Z;
        let next = turn_toward(
            current,
            goal,
            desired_direction,
            profile.pitch_rate,
            elapsed,
        );
        self.set_attachment_rotation(component, next);
        HardpointAim {
            can_face: !capped || !profile.has_hard_pitch_limits(),
            oriented: (next * Vec3::Z).dot(actual_direction) >= DIRECTION_TOLERANCE,
        }
    }

    fn orient_combined(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: &AttackAnimationAnchor,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> HardpointAim {
        let Some(component) = profile.yaw_attachment.as_deref() else {
            return HardpointAim::HANDLED;
        };
        let Some(base) = self.component_base(anchor, component) else {
            return HardpointAim::BODY_HANDLED;
        };
        let Some(target) = target_in_base(unit_world * base, target_world) else {
            return HardpointAim::HANDLED;
        };
        let Some(actual_direction) = target.try_normalize() else {
            return HardpointAim::HANDLED;
        };
        let (desired_direction, capped) = combined_target(profile, actual_direction);
        let goal = direction_orientation(desired_direction);
        let current = self.attachment_rotation(component);
        if (current * Vec3::Z).dot(actual_direction) >= DIRECTION_TOLERANCE {
            return HardpointAim::HANDLED;
        }
        let next = turn_toward(current, goal, desired_direction, profile.yaw_rate, elapsed);
        self.set_attachment_rotation(component, next);
        HardpointAim {
            can_face: !capped,
            oriented: (next * Vec3::Z).dot(actual_direction) >= DIRECTION_TOLERANCE,
        }
    }

    fn orient_single_bone(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> HardpointAim {
        let Some(bone) = single_bone_name(profile) else {
            return HardpointAim::HANDLED;
        };
        let base_world = unit_world * single_bone_base(profile, anchor);
        let Some(mut target) = target_in_base(base_world, target_world) else {
            return HardpointAim::HANDLED;
        };
        target.y = 0.0;
        let Some(actual_direction) = target.try_normalize() else {
            return HardpointAim::HANDLED;
        };
        let desired_angle = math::atan2(actual_direction.x, actual_direction.z);
        let clamped_angle = desired_angle.clamp(profile.yaw_left, profile.yaw_right);
        let capped = (desired_angle - clamped_angle).abs() > ROTATION_EPSILON;
        let goal = math::quat_from_rotation_y(clamped_angle);
        let current = self.bone_rotation(bone);
        if (current * Vec3::Z).dot(actual_direction) >= DIRECTION_TOLERANCE {
            return HardpointAim::HANDLED;
        }
        let next = turn_toward(current, goal, goal * Vec3::Z, profile.yaw_rate, elapsed);
        self.set_bone_rotation(bone, next);
        HardpointAim {
            can_face: !capped,
            oriented: (next * Vec3::Z).dot(actual_direction) >= DIRECTION_TOLERANCE,
        }
    }

    fn component_base(&self, anchor: &AttackAnimationAnchor, component: &str) -> Option<Mat4> {
        let mut parent = Mat4::IDENTITY;
        for link in &anchor.links {
            if link.child_component.eq_ignore_ascii_case(component) {
                return Some(parent * link.to_bone.unwrap_or(Mat4::IDENTITY));
            }
            parent *= motion::transformed_attachment_transform(
                link.to_bone,
                link.from_bone,
                self.attachment_transform(&link.child_component),
                link.disregard_orientation,
            );
        }
        None
    }
}

impl HardpointPose for UnitHardpointState {
    fn attachment_transform(&self, name: &str) -> Option<Mat4> {
        self.attachment_transforms
            .get(&canonical_name(name))
            .copied()
    }

    fn bone_transform(&self, name: &str) -> Option<Mat4> {
        self.bone_transforms.get(&canonical_name(name)).copied()
    }

    fn set_attachment_rotation(&mut self, name: &str, rotation: Quat) {
        self.attachment_transforms.insert(
            canonical_name(name),
            Mat4::from_quat(normalized_or_identity(rotation)),
        );
    }

    fn set_bone_rotation(&mut self, name: &str, rotation: Quat) {
        self.bone_transforms.insert(
            canonical_name(name),
            Mat4::from_quat(normalized_or_identity(rotation)),
        );
    }
}

/// Proposed transforms over a borrowed hardpoint state.
pub(super) struct PoseProbe<'a> {
    state: &'a UnitHardpointState,
    attachments: Vec<(String, Mat4)>,
    bones: Vec<(String, Mat4)>,
}

impl<'a> PoseProbe<'a> {
    pub(super) const fn new(state: &'a UnitHardpointState) -> Self {
        Self {
            state,
            attachments: Vec::new(),
            bones: Vec::new(),
        }
    }
}

fn proposed(proposals: &[(String, Mat4)], key: &str) -> Option<Mat4> {
    proposals
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, transform)| *transform)
}

fn propose(proposals: &mut Vec<(String, Mat4)>, key: String, rotation: Quat) {
    let transform = Mat4::from_quat(normalized_or_identity(rotation));
    if let Some(entry) = proposals.iter_mut().find(|(name, _)| *name == key) {
        entry.1 = transform;
    } else {
        proposals.push((key, transform));
    }
}

impl HardpointPose for PoseProbe<'_> {
    fn attachment_transform(&self, name: &str) -> Option<Mat4> {
        let key = canonical_name(name);
        proposed(&self.attachments, &key)
            .or_else(|| self.state.attachment_transforms.get(&key).copied())
    }

    fn bone_transform(&self, name: &str) -> Option<Mat4> {
        let key = canonical_name(name);
        proposed(&self.bones, &key).or_else(|| self.state.bone_transforms.get(&key).copied())
    }

    fn set_attachment_rotation(&mut self, name: &str, rotation: Quat) {
        propose(&mut self.attachments, canonical_name(name), rotation);
    }

    fn set_bone_rotation(&mut self, name: &str, rotation: Quat) {
        propose(&mut self.bones, canonical_name(name), rotation);
    }
}
