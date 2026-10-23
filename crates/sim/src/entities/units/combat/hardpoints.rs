//! Deterministic mutable hardpoint orientation shared by attack simulation and presentation.

use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat3, Mat4, Quat, Vec3};

use crate::gameplay::{AttackAnimationAnchor, AttackHardpointProfile};
use crate::math;
use crate::sync::SyncChecksum;

mod pose;

use pose::{HardpointPose, PoseProbe};

const AUTO_CENTER_DELAY_SECONDS: f32 = 5.0;
const DIRECTION_TOLERANCE: f32 = 0.9999;
const ROTATION_EPSILON: f32 = 1.0e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HardpointAim {
    pub(crate) can_face: bool,
    pub(crate) oriented: bool,
}

impl HardpointAim {
    const HANDLED: Self = Self {
        can_face: true,
        oriented: true,
    };

    const BODY_HANDLED: Self = Self {
        can_face: false,
        oriented: true,
    };
}

#[derive(Debug, Clone)]
struct HardpointController {
    profile: AttackHardpointProfile,
    anchor: Option<AttackAnimationAnchor>,
    auto_center_delay: f32,
    centered: bool,
}

impl HardpointController {
    fn active(profile: &AttackHardpointProfile, anchor: Option<&AttackAnimationAnchor>) -> Self {
        Self {
            profile: profile.clone(),
            anchor: anchor.cloned(),
            auto_center_delay: 0.0,
            centered: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct UnitHardpointState {
    attachment_transforms: BTreeMap<String, Mat4>,
    bone_transforms: BTreeMap<String, Mat4>,
    controllers: BTreeMap<String, HardpointController>,
    active_controllers: BTreeSet<String>,
}

impl UnitHardpointState {
    pub(super) fn claim(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
    ) {
        self.activate(profile, anchor);
    }

    pub(super) fn orient(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> HardpointAim {
        self.activate(profile, anchor);
        if profile.uses_angles_as_tolerance() {
            return self.tolerance_coverage(profile, anchor, unit_world, target_world);
        }
        self.orient_target(profile, anchor, unit_world, target_world, elapsed)
    }

    pub(super) fn is_oriented(
        &self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
        tolerance: f32,
    ) -> bool {
        if profile.uses_angles_as_tolerance() {
            return self
                .tolerance_coverage(profile, anchor, unit_world, target_world)
                .oriented;
        }
        if !target_world.is_finite() || !valid_transform(unit_world) {
            return true;
        }
        if profile.uses_single_bone_ik() {
            let Some(bone) = single_bone_name(profile) else {
                return true;
            };
            return planar_alignment(
                unit_world * single_bone_base(profile, anchor),
                self.bone_rotation(bone),
                target_world,
                tolerance,
            );
        }
        let Some(anchor) = anchor else {
            return true;
        };
        let component = profile
            .yaw_attachment
            .as_deref()
            .or(profile.pitch_attachment.as_deref());
        let Some(component) = component else {
            return true;
        };
        let Some(base) = self.component_base(anchor, component) else {
            return true;
        };
        planar_alignment(
            unit_world * base,
            self.attachment_rotation(component),
            target_world,
            tolerance,
        )
    }

    pub(super) fn can_orient(
        &self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
    ) -> bool {
        if profile.uses_angles_as_tolerance() {
            return self
                .tolerance_coverage(profile, anchor, unit_world, target_world)
                .oriented;
        }
        // The probe records the proposed transforms over the unchanged state, so a
        // later step reads them as `orient` does, without a copy of the state.
        PoseProbe::new(self)
            .orient_target(profile, anchor, unit_world, target_world, 0.0)
            .can_face
    }

    pub(super) fn orient_pitch_only(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> bool {
        self.activate(profile, anchor);
        if profile.uses_single_bone_ik() || profile.is_combined() {
            return true;
        }
        let Some(component) = profile.pitch_attachment.as_deref() else {
            return true;
        };
        let Some(anchor) = anchor else {
            return true;
        };
        self.orient_pitch(
            profile,
            anchor,
            component,
            unit_world,
            target_world,
            elapsed,
        )
        .can_face
    }

    pub(super) fn yaw_forward(
        &self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
    ) -> Option<Vec3> {
        if !valid_transform(unit_world) {
            return None;
        }
        let (base, rotation) = if profile.uses_single_bone_ik() {
            let bone = single_bone_name(profile)?;
            (single_bone_base(profile, anchor), self.bone_rotation(bone))
        } else {
            let component = profile
                .yaw_attachment
                .as_deref()
                .or(profile.pitch_attachment.as_deref())?;
            (
                self.component_base(anchor?, component)?,
                self.attachment_rotation(component),
            )
        };
        (unit_world * base)
            .transform_vector3(rotation * Vec3::Z)
            .try_normalize()
    }

    pub(super) fn yaw_origin(
        &self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
    ) -> Option<Vec3> {
        let base = if profile.uses_single_bone_ik() {
            single_bone_base(profile, anchor)
        } else {
            let component = profile.yaw_attachment.as_deref()?;
            self.component_base(anchor?, component)?
        };
        valid_transform(unit_world).then(|| (unit_world * base).w_axis.truncate())
    }

    pub(super) fn release(&mut self, hardpoint_name: &str) {
        let key = canonical_name(hardpoint_name);
        if self.active_controllers.remove(&key) {
            self.begin_auto_center(&key);
        }
    }

    pub(super) fn advance_auto_center(&mut self, elapsed: f32) {
        if !elapsed.is_finite() || elapsed <= 0.0 {
            return;
        }
        let controlled = self.controlled_names();
        // Centering changes only the transforms, so it can run in the same
        // key-ordered pass that counts down the other controllers.
        for (key, controller) in &mut self.controllers {
            if self.active_controllers.contains(key)
                || controller.centered
                || !controller.profile.auto_centers()
            {
                continue;
            }
            if controller.auto_center_delay > 0.0 {
                controller.auto_center_delay -= elapsed;
                continue;
            }
            let centered = center_profile(
                &mut self.attachment_transforms,
                &mut self.bone_transforms,
                &controller.profile,
                elapsed,
                &controlled,
            );
            if centered {
                controller.centered = true;
                controller.auto_center_delay = 0.0;
            }
        }
    }

    pub(super) fn attachment_transform(&self, name: &str) -> Option<Mat4> {
        self.attachment_transforms
            .get(&canonical_name(name))
            .copied()
    }

    pub(super) fn bone_transforms(&self) -> impl Iterator<Item = (&str, Mat4)> {
        self.bone_transforms
            .iter()
            .map(|(name, transform)| (name.as_str(), *transform))
    }

    pub(super) fn bone_transform(&self, name: &str) -> Option<Mat4> {
        self.bone_transforms.get(&canonical_name(name)).copied()
    }

    pub(super) fn yaw_target_world(
        &self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
    ) -> Option<Vec3> {
        if !profile.preserves_yaw_on_unit_turn() || profile.uses_angles_as_tolerance() {
            return None;
        }
        let (origin, rotation) = if profile.uses_single_bone_ik() {
            let bone = single_bone_name(profile)?;
            let origin = anchor
                .and_then(|anchor| {
                    anchor
                        .single_bone_poses
                        .iter()
                        .find(|pose| pose.bone.eq_ignore_ascii_case(bone))
                })
                .map_or(Vec3::ZERO, |pose| {
                    (pose.parent_to_component * pose.local_transform)
                        .w_axis
                        .truncate()
                });
            (origin, self.bone_rotation(bone))
        } else {
            let component = profile.yaw_attachment.as_deref()?;
            let origin = self.component_base(anchor?, component)?.w_axis.truncate();
            (origin, self.attachment_rotation(component))
        };
        let direction = unit_world
            .transform_vector3(rotation * Vec3::Z)
            .try_normalize()?;
        Some(unit_world.transform_point3(origin) + direction * 10_000.0)
    }

    pub(super) fn preserve_yaw_target(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
    ) {
        if profile.uses_single_bone_ik() {
            let _aim = self.orient_single_bone(profile, anchor, unit_world, target_world, 10_000.0);
        } else if profile.is_combined() {
            if let Some(anchor) = anchor {
                let _aim =
                    self.orient_combined(profile, anchor, unit_world, target_world, 10_000.0);
            }
        } else if let (Some(anchor), Some(component)) = (anchor, profile.yaw_attachment.as_deref())
        {
            let _aim = self.orient_yaw(
                profile,
                anchor,
                component,
                unit_world,
                target_world,
                10_000.0,
            );
        }
    }

    pub(super) fn active_yaw_target_world(
        &self,
        hardpoint_name: &str,
        unit_world: Mat4,
    ) -> Option<Vec3> {
        let key = canonical_name(hardpoint_name);
        let controller = self.active_controllers.contains(&key).then(|| {
            self.controllers
                .get(&key)
                .expect("active hardpoint has a controller")
        })?;
        self.yaw_target_world(&controller.profile, controller.anchor.as_ref(), unit_world)
    }

    pub(super) fn restore_active_yaw_target(
        &mut self,
        hardpoint_name: &str,
        unit_world: Mat4,
        target_world: Vec3,
    ) {
        let key = canonical_name(hardpoint_name);
        let Some((profile, anchor)) = self
            .active_controllers
            .contains(&key)
            .then(|| self.controllers.get(&key))
            .flatten()
            .map(|controller| (controller.profile.clone(), controller.anchor.clone()))
        else {
            return;
        };
        self.preserve_yaw_target(&profile, anchor.as_ref(), unit_world, target_world);
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.active_controllers.len()).unwrap_or(u32::MAX));
        for name in &self.active_controllers {
            checksum.hash_string(name);
        }
        hash_transforms(checksum, &self.attachment_transforms);
        hash_transforms(checksum, &self.bone_transforms);
        checksum.hash_u32(u32::try_from(self.controllers.len()).unwrap_or(u32::MAX));
        for (name, controller) in &self.controllers {
            checksum.hash_string(name);
            checksum.hash_f32(controller.auto_center_delay);
            checksum.hash_u32(u32::from(controller.centered));
        }
    }

    fn activate(
        &mut self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
    ) {
        let key = canonical_name(&profile.name);
        self.active_controllers.insert(key.clone());
        self.controllers
            .entry(key)
            .and_modify(|controller| {
                controller.profile.clone_from(profile);
                controller.anchor = anchor.cloned();
                controller.auto_center_delay = 0.0;
                controller.centered = false;
            })
            .or_insert_with(|| HardpointController::active(profile, anchor));
    }

    fn tolerance_coverage(
        &self,
        profile: &AttackHardpointProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
    ) -> HardpointAim {
        if !target_world.is_finite() || !valid_transform(unit_world) {
            return HardpointAim::BODY_HANDLED;
        }
        if profile.uses_single_bone_ik() {
            let base = unit_world * single_bone_base(profile, anchor);
            let covered = target_in_base(base, target_world)
                .and_then(Vec3::try_normalize)
                .is_none_or(|direction| yaw_is_covered(profile, direction));
            return HardpointAim {
                can_face: false,
                oriented: covered,
            };
        }
        let Some(anchor) = anchor else {
            return HardpointAim::BODY_HANDLED;
        };
        let yaw = profile.yaw_attachment.as_deref().is_none_or(|component| {
            self.component_base(anchor, component)
                .and_then(|base| target_in_base(unit_world * base, target_world))
                .and_then(Vec3::try_normalize)
                .is_none_or(|direction| {
                    if profile.is_combined() {
                        !combined_target(profile, direction).1
                    } else {
                        yaw_is_covered(profile, direction)
                    }
                })
        });
        let pitch = profile.pitch_attachment.as_deref().is_none_or(|component| {
            if !profile.has_hard_pitch_limits() {
                return true;
            }
            self.component_base(anchor, component)
                .and_then(|base| target_in_base(unit_world * base, target_world))
                .and_then(Vec3::try_normalize)
                .is_none_or(|direction| pitch_is_covered(profile, direction))
        });
        HardpointAim {
            can_face: false,
            oriented: yaw && pitch,
        }
    }

    fn controlled_names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for key in &self.active_controllers {
            let Some(active) = self.controllers.get(key) else {
                continue;
            };
            if active.profile.uses_single_bone_ik() {
                names.extend(single_bone_name(&active.profile).map(canonical_name));
            } else {
                names.extend(
                    active
                        .profile
                        .yaw_attachment
                        .iter()
                        .chain(&active.profile.pitch_attachment)
                        .map(|name| canonical_name(name)),
                );
            }
        }
        names
    }

    fn begin_auto_center(&mut self, key: &str) {
        if let Some(controller) = self.controllers.get_mut(key) {
            controller.auto_center_delay = AUTO_CENTER_DELAY_SECONDS;
            controller.centered = false;
        }
    }
}

fn combined_target(profile: &AttackHardpointProfile, direction: Vec3) -> (Vec3, bool) {
    let same_component = profile.yaw_attachment.as_deref().is_some_and(|yaw| {
        profile
            .pitch_attachment
            .as_deref()
            .is_some_and(|pitch| yaw.eq_ignore_ascii_case(pitch))
    });
    if same_component {
        let yaw = math::atan2(direction.x, direction.z);
        let pitch = math::atan2(direction.y, math::hypot(direction.x, direction.z));
        let limited_yaw = yaw.clamp(profile.yaw_left, profile.yaw_right);
        let limited_pitch = pitch.clamp(profile.pitch_min, profile.pitch_max);
        let yaw_capped = (yaw - limited_yaw).abs() > ROTATION_EPSILON;
        let pitch_capped = (pitch - limited_pitch).abs() > ROTATION_EPSILON;
        let rotation =
            math::quat_from_rotation_y(limited_yaw) * math::quat_from_rotation_x(-limited_pitch);
        return (
            rotation * Vec3::Z,
            yaw_capped || pitch_capped && profile.has_hard_pitch_limits(),
        );
    }
    let maximum = profile.yaw_right.max(0.0);
    let angle = math::acos(direction.dot(Vec3::Z).clamp(-1.0, 1.0));
    if maximum >= std::f32::consts::PI || angle <= maximum {
        return (direction, false);
    }
    let factor = (maximum / angle).clamp(0.0, 1.0);
    let full_rotation = math::quat_from_rotation_arc(Vec3::Z, direction);
    let capped = math::quat_slerp(Quat::IDENTITY, full_rotation, factor) * Vec3::Z;
    (capped, true)
}

fn yaw_is_covered(profile: &AttackHardpointProfile, mut direction: Vec3) -> bool {
    direction.y = 0.0;
    direction.try_normalize().is_none_or(|direction| {
        let angle = math::atan2(direction.x, direction.z);
        angle >= profile.yaw_left && angle <= profile.yaw_right
    })
}

fn pitch_is_covered(profile: &AttackHardpointProfile, direction: Vec3) -> bool {
    let angle = math::atan2(direction.y, math::hypot(direction.x, direction.z));
    angle >= profile.pitch_min && angle <= profile.pitch_max
}

fn direction_orientation(direction: Vec3) -> Quat {
    let forward = direction.normalize_or_zero();
    let Some(right) = Vec3::Y.cross(forward).try_normalize() else {
        return math::quat_from_rotation_arc(Vec3::Z, forward);
    };
    let up = forward.cross(right).normalize_or_zero();
    normalized_or_identity(Quat::from_mat3(&Mat3::from_cols(right, up, forward)))
}

fn turn_toward(
    current: Quat,
    goal: Quat,
    desired_direction: Vec3,
    rate: f32,
    elapsed: f32,
) -> Quat {
    let current = normalized_or_identity(current);
    let goal = normalized_or_identity(goal);
    let current_direction = (current * Vec3::Z).normalize_or_zero();
    let angle = math::acos(
        current_direction
            .dot(desired_direction.normalize_or_zero())
            .clamp(-1.0, 1.0),
    );
    if !angle.is_finite() || angle <= ROTATION_EPSILON {
        return goal;
    }
    let maximum = (rate.max(0.0) * elapsed.max(0.0)).max(0.0);
    math::quat_slerp(current, goal, (maximum / angle).clamp(0.0, 1.0))
}

fn center_profile(
    attachment_transforms: &mut BTreeMap<String, Mat4>,
    bone_transforms: &mut BTreeMap<String, Mat4>,
    profile: &AttackHardpointProfile,
    elapsed: f32,
    controlled: &BTreeSet<String>,
) -> bool {
    if profile.uses_single_bone_ik() {
        return single_bone_name(profile).is_none_or(|bone| {
            center_named_rotation(bone_transforms, bone, profile.yaw_rate, elapsed, controlled)
        });
    }
    let yaw_done = profile.yaw_attachment.as_deref().is_none_or(|component| {
        center_named_rotation(
            attachment_transforms,
            component,
            profile.yaw_rate,
            elapsed,
            controlled,
        )
    });
    let pitch_done = profile.is_combined()
        || profile.pitch_attachment.as_deref().is_none_or(|component| {
            center_named_rotation(
                attachment_transforms,
                component,
                profile.pitch_rate,
                elapsed,
                controlled,
            )
        });
    yaw_done && pitch_done
}

fn center_named_rotation(
    transforms: &mut BTreeMap<String, Mat4>,
    name: &str,
    rate: f32,
    elapsed: f32,
    controlled: &BTreeSet<String>,
) -> bool {
    let key = canonical_name(name);
    if controlled.contains(&key) {
        return false;
    }
    let Some(transform) = transforms.get_mut(&key) else {
        return true;
    };
    let current = rotation_of(*transform);
    let next = turn_toward(current, Quat::IDENTITY, Vec3::Z, rate, elapsed);
    *transform = Mat4::from_quat(next);
    (next * Vec3::Z).dot(Vec3::Z) >= DIRECTION_TOLERANCE
}

fn target_in_base(base_world: Mat4, target_world: Vec3) -> Option<Vec3> {
    valid_transform(base_world)
        .then(|| base_world.inverse().transform_point3(target_world))
        .filter(|target| target.is_finite())
}

fn planar_alignment(base_world: Mat4, rotation: Quat, target_world: Vec3, tolerance: f32) -> bool {
    let Some(mut target) = target_in_base(base_world, target_world) else {
        return true;
    };
    target.y = 0.0;
    let Some(target) = target.try_normalize() else {
        return true;
    };
    let mut forward = rotation * Vec3::Z;
    forward.y = 0.0;
    forward
        .try_normalize()
        .is_none_or(|forward| forward.dot(target) >= tolerance)
}

fn valid_transform(transform: Mat4) -> bool {
    let determinant = transform.determinant();
    transform.is_finite() && determinant.is_finite() && determinant.abs() > f32::EPSILON
}

fn rotation_of(transform: Mat4) -> Quat {
    let rotation = Mat3::from_cols(
        transform.x_axis.truncate(),
        transform.y_axis.truncate(),
        transform.z_axis.truncate(),
    );
    normalized_or_identity(Quat::from_mat3(&rotation))
}

fn normalized_or_identity(rotation: Quat) -> Quat {
    let length_squared = rotation.length_squared();
    if length_squared.is_finite() && length_squared > ROTATION_EPSILON {
        rotation / length_squared.sqrt()
    } else {
        Quat::IDENTITY
    }
}

fn canonical_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn single_bone_name(profile: &AttackHardpointProfile) -> Option<&str> {
    profile
        .single_bone
        .as_deref()
        .or(profile.yaw_attachment.as_deref())
}

fn single_bone_base(
    profile: &AttackHardpointProfile,
    anchor: Option<&AttackAnimationAnchor>,
) -> Mat4 {
    let Some(bone) = single_bone_name(profile) else {
        return Mat4::IDENTITY;
    };
    let Some(pose) = anchor.and_then(|anchor| {
        anchor
            .single_bone_poses
            .iter()
            .find(|pose| pose.bone.eq_ignore_ascii_case(bone))
    }) else {
        return Mat4::IDENTITY;
    };
    let bone_to_component = pose.parent_to_component * pose.local_transform;
    if profile.is_relative_to_unit() {
        Mat4::from_translation(bone_to_component.w_axis.truncate())
    } else {
        bone_to_component
    }
}

fn hash_transforms(checksum: &mut SyncChecksum, transforms: &BTreeMap<String, Mat4>) {
    checksum.hash_u32(u32::try_from(transforms.len()).unwrap_or(u32::MAX));
    for (name, transform) in transforms {
        checksum.hash_string(name);
        for value in transform.to_cols_array() {
            checksum.hash_f32(value);
        }
    }
}

#[cfg(test)]
mod tests;
