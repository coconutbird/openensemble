//! Deterministic mutable hardpoint orientation shared by attack simulation and presentation.

use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat3, Mat4, Quat, Vec3};

use crate::gameplay::{AttackAnimationAnchor, AttackHardpointProfile};
use crate::sync::SyncChecksum;

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
    active_controller: Option<String>,
}

impl UnitHardpointState {
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

    pub(super) fn release_active(&mut self) {
        let Some(key) = self.active_controller.take() else {
            return;
        };
        if let Some(controller) = self.controllers.get_mut(&key) {
            controller.auto_center_delay = AUTO_CENTER_DELAY_SECONDS;
            controller.centered = false;
        }
    }

    pub(super) fn advance_auto_center(&mut self, elapsed: f32) {
        if !elapsed.is_finite() || elapsed <= 0.0 {
            return;
        }
        let controlled = self.controlled_names();
        let active = self.active_controller.as_deref();
        let mut ready = Vec::new();
        for (key, controller) in &mut self.controllers {
            if active == Some(key) || controller.centered || !controller.profile.auto_centers() {
                continue;
            }
            if controller.auto_center_delay > 0.0 {
                controller.auto_center_delay -= elapsed;
                continue;
            }
            ready.push((key.clone(), controller.profile.clone()));
        }
        for (key, profile) in ready {
            let centered = self.center_profile(&profile, elapsed, &controlled);
            if centered && let Some(controller) = self.controllers.get_mut(&key) {
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

    pub(super) fn active_yaw_target_world(&self, unit_world: Mat4) -> Option<Vec3> {
        let controller = self
            .active_controller
            .as_deref()
            .and_then(|key| self.controllers.get(key))?;
        self.yaw_target_world(&controller.profile, controller.anchor.as_ref(), unit_world)
    }

    pub(super) fn restore_active_yaw_target(&mut self, unit_world: Mat4, target_world: Vec3) {
        let Some((profile, anchor)) = self
            .active_controller
            .as_deref()
            .and_then(|key| self.controllers.get(key))
            .map(|controller| (controller.profile.clone(), controller.anchor.clone()))
        else {
            return;
        };
        self.preserve_yaw_target(&profile, anchor.as_ref(), unit_world, target_world);
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_optional_name(checksum, self.active_controller.as_deref());
        hash_transforms(checksum, &self.attachment_transforms);
        hash_transforms(checksum, &self.bone_transforms);
        checksum.hash_u32(u32::try_from(self.controllers.len()).unwrap_or(u32::MAX));
        for (name, controller) in &self.controllers {
            hash_name(checksum, name);
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
        if self.active_controller.as_deref() != Some(&key) {
            self.release_active();
            self.active_controller = Some(key.clone());
        }
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
        let desired_angle = actual_direction.x.atan2(actual_direction.z);
        let clamped_angle = desired_angle.clamp(profile.yaw_left, profile.yaw_right);
        let capped = (desired_angle - clamped_angle).abs() > ROTATION_EPSILON;
        let goal = Quat::from_rotation_y(clamped_angle);
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
        let planar = target.x.hypot(target.z);
        let desired_angle = target.y.atan2(planar);
        let clamped_angle = desired_angle.clamp(profile.pitch_min, profile.pitch_max);
        let capped = (desired_angle - clamped_angle).abs() > ROTATION_EPSILON;
        let actual_direction = Vec3::new(0.0, target.y, planar)
            .try_normalize()
            .unwrap_or(Vec3::Z);
        let goal = Quat::from_rotation_x(-clamped_angle);
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
        let desired_angle = actual_direction.x.atan2(actual_direction.z);
        let clamped_angle = desired_angle.clamp(profile.yaw_left, profile.yaw_right);
        let capped = (desired_angle - clamped_angle).abs() > ROTATION_EPSILON;
        let goal = Quat::from_rotation_y(clamped_angle);
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

    fn center_profile(
        &mut self,
        profile: &AttackHardpointProfile,
        elapsed: f32,
        controlled: &BTreeSet<String>,
    ) -> bool {
        if profile.uses_single_bone_ik() {
            return single_bone_name(profile).is_none_or(|bone| {
                center_named_rotation(
                    &mut self.bone_transforms,
                    bone,
                    profile.yaw_rate,
                    elapsed,
                    controlled,
                )
            });
        }
        let yaw_done = profile.yaw_attachment.as_deref().is_none_or(|component| {
            center_named_rotation(
                &mut self.attachment_transforms,
                component,
                profile.yaw_rate,
                elapsed,
                controlled,
            )
        });
        let pitch_done = profile.is_combined()
            || profile.pitch_attachment.as_deref().is_none_or(|component| {
                center_named_rotation(
                    &mut self.attachment_transforms,
                    component,
                    profile.pitch_rate,
                    elapsed,
                    controlled,
                )
            });
        yaw_done && pitch_done
    }

    fn controlled_names(&self) -> BTreeSet<String> {
        let Some(active) = self
            .active_controller
            .as_deref()
            .and_then(|key| self.controllers.get(key))
        else {
            return BTreeSet::new();
        };
        if active.profile.uses_single_bone_ik() {
            single_bone_name(&active.profile)
                .map(canonical_name)
                .into_iter()
                .collect()
        } else {
            active
                .profile
                .yaw_attachment
                .iter()
                .chain(&active.profile.pitch_attachment)
                .map(|name| canonical_name(name))
                .collect()
        }
    }

    fn attachment_rotation(&self, name: &str) -> Quat {
        rotation_of(self.attachment_transform(name).unwrap_or(Mat4::IDENTITY))
    }

    fn bone_rotation(&self, name: &str) -> Quat {
        rotation_of(
            self.bone_transforms
                .get(&canonical_name(name))
                .copied()
                .unwrap_or(Mat4::IDENTITY),
        )
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

fn combined_target(profile: &AttackHardpointProfile, direction: Vec3) -> (Vec3, bool) {
    let same_component = profile.yaw_attachment.as_deref().is_some_and(|yaw| {
        profile
            .pitch_attachment
            .as_deref()
            .is_some_and(|pitch| yaw.eq_ignore_ascii_case(pitch))
    });
    if same_component {
        let yaw = direction.x.atan2(direction.z);
        let pitch = direction.y.atan2(direction.x.hypot(direction.z));
        let limited_yaw = yaw.clamp(profile.yaw_left, profile.yaw_right);
        let limited_pitch = pitch.clamp(profile.pitch_min, profile.pitch_max);
        let yaw_capped = (yaw - limited_yaw).abs() > ROTATION_EPSILON;
        let pitch_capped = (pitch - limited_pitch).abs() > ROTATION_EPSILON;
        let rotation = Quat::from_rotation_y(limited_yaw) * Quat::from_rotation_x(-limited_pitch);
        return (
            rotation * Vec3::Z,
            yaw_capped || pitch_capped && profile.has_hard_pitch_limits(),
        );
    }
    let maximum = profile.yaw_right.max(0.0);
    let angle = direction.dot(Vec3::Z).clamp(-1.0, 1.0).acos();
    if maximum >= std::f32::consts::PI || angle <= maximum {
        return (direction, false);
    }
    let factor = (maximum / angle).clamp(0.0, 1.0);
    let full_rotation = Quat::from_rotation_arc(Vec3::Z, direction);
    let capped = Quat::IDENTITY.slerp(full_rotation, factor) * Vec3::Z;
    (capped, true)
}

fn yaw_is_covered(profile: &AttackHardpointProfile, mut direction: Vec3) -> bool {
    direction.y = 0.0;
    direction.try_normalize().is_none_or(|direction| {
        let angle = direction.x.atan2(direction.z);
        angle >= profile.yaw_left && angle <= profile.yaw_right
    })
}

fn pitch_is_covered(profile: &AttackHardpointProfile, direction: Vec3) -> bool {
    let angle = direction.y.atan2(direction.x.hypot(direction.z));
    angle >= profile.pitch_min && angle <= profile.pitch_max
}

fn direction_orientation(direction: Vec3) -> Quat {
    let forward = direction.normalize_or_zero();
    let Some(right) = Vec3::Y.cross(forward).try_normalize() else {
        return Quat::from_rotation_arc(Vec3::Z, forward);
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
    let angle = current_direction
        .dot(desired_direction.normalize_or_zero())
        .clamp(-1.0, 1.0)
        .acos();
    if !angle.is_finite() || angle <= ROTATION_EPSILON {
        return goal;
    }
    let maximum = (rate.max(0.0) * elapsed.max(0.0)).max(0.0);
    current.slerp(goal, (maximum / angle).clamp(0.0, 1.0))
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
        hash_name(checksum, name);
        for value in transform.to_cols_array() {
            checksum.hash_f32(value);
        }
    }
}

fn hash_optional_name(checksum: &mut SyncChecksum, name: Option<&str>) {
    checksum.hash_u32(u32::from(name.is_some()));
    if let Some(name) = name {
        hash_name(checksum, name);
    }
}

fn hash_name(checksum: &mut SyncChecksum, name: &str) {
    checksum.hash_u32(u32::try_from(name.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(name.as_bytes());
}

#[cfg(test)]
mod tests;
