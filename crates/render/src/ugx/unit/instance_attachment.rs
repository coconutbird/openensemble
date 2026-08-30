//! Per-instance model attachment bindings evaluated from animated poses.

use glam::Mat4;
use pipeline::database::hw1::visual::Attachment;

use crate::ugx::Model;
use crate::ugx::model::ModelPose;

pub(super) fn allocate_index(next: &mut usize) -> usize {
    let index = *next;
    *next = next.saturating_add(1);
    index
}

#[derive(Clone, Debug)]
pub(super) struct InstanceAttachment {
    parent_index: usize,
    to_bone: Option<String>,
    from_bone: Option<String>,
    disregard_orientation: bool,
}

#[derive(Clone, Debug)]
pub(super) struct AttachmentAnchor {
    owner_index: usize,
    to_bone: Option<String>,
    disregard_orientation: bool,
}

impl AttachmentAnchor {
    pub(super) fn new(owner_index: usize, attachment: &Attachment) -> Self {
        Self {
            owner_index,
            to_bone: attachment.to_bone.clone(),
            disregard_orientation: attachment.disregard_orient.unwrap_or(false),
        }
    }
}

impl InstanceAttachment {
    pub(super) fn new(parent_index: usize, attachment: &Attachment) -> Self {
        Self {
            parent_index,
            to_bone: attachment.to_bone.clone(),
            from_bone: attachment.from_bone.clone(),
            disregard_orientation: attachment.disregard_orient.unwrap_or(false),
        }
    }

    pub(super) fn parent_index(&self) -> usize {
        self.parent_index
    }

    pub(super) fn resolve(
        &self,
        parent_transform: Mat4,
        parent_model: &Model,
        parent_pose: &ModelPose,
        child_model: &Model,
        child_pose: &ModelPose,
        dynamic_transform: Option<Mat4>,
    ) -> Mat4 {
        let to_bone = self
            .to_bone
            .as_deref()
            .and_then(|name| parent_model.posed_bone_to_model(parent_pose, name));
        let from_bone = self
            .from_bone
            .as_deref()
            .and_then(|name| child_model.posed_bone_to_model(child_pose, name));
        resolve_matrices(
            parent_transform,
            to_bone,
            from_bone,
            dynamic_transform,
            self.disregard_orientation,
        )
    }
}

pub(super) fn update_transforms(
    instances: &mut [super::RenderedUnitInstance],
    dynamic_transform: impl Fn(&str) -> Option<Mat4>,
) {
    for child_index in 0..instances.len() {
        let Some(attachment) = instances[child_index].attachment.clone() else {
            continue;
        };
        let parent_index = attachment.parent_index();
        if parent_index >= child_index {
            log::warn!(
                "UGX component attachment has invalid parent index {parent_index} for child {child_index}"
            );
            continue;
        }
        let (parents, children) = instances.split_at_mut(child_index);
        let parent = &parents[parent_index];
        let child = &mut children[0];
        child.local_transform = attachment.resolve(
            parent.local_transform,
            &parent.model,
            &parent.pose,
            &child.model,
            &child.pose,
            dynamic_transform(&child.name),
        );
    }
}

pub(super) fn update_anchors(
    instances: &[super::RenderedUnitInstance],
    attachments: &mut [super::UnitAttachment],
) {
    for attachment in attachments {
        let Some(binding) = &attachment.anchor_binding else {
            continue;
        };
        let Some(owner) = instances.get(binding.owner_index) else {
            log::warn!(
                "UGX effect attachment has invalid owner index {}",
                binding.owner_index
            );
            continue;
        };
        let to_bone = binding
            .to_bone
            .as_deref()
            .and_then(|name| owner.model.posed_bone_to_model(&owner.pose, name));
        attachment.anchor_transform = resolve_anchor(
            owner.local_transform,
            to_bone,
            binding.disregard_orientation,
        );
    }
}

pub(super) fn visual_transform(
    parent: &Model,
    parent_pose: &ModelPose,
    child: &Model,
    child_pose: &ModelPose,
    attachment: &Attachment,
) -> Mat4 {
    let to_bone = attachment
        .to_bone
        .as_deref()
        .and_then(|name| find_bone(parent, parent_pose, name, "tobone", &attachment.name));
    let from_bone = attachment
        .from_bone
        .as_deref()
        .and_then(|name| find_bone(child, child_pose, name, "frombone", &attachment.name));
    attachment_transform(
        to_bone,
        from_bone,
        attachment.disregard_orient.unwrap_or(false),
    )
}

pub(super) fn find_bone(
    model: &Model,
    pose: &ModelPose,
    bone_name: &str,
    role: &str,
    attachment_name: &str,
) -> Option<Mat4> {
    let matrix = model.posed_bone_to_model(pose, bone_name);
    if matrix.is_none() {
        log::debug!("UGX attachment '{attachment_name}' has no {role} '{bone_name}'");
    }
    matrix
}

pub(super) fn attachment_transform(
    to_bone: Option<Mat4>,
    from_bone: Option<Mat4>,
    disregard_orientation: bool,
) -> Mat4 {
    motion::attachment_transform(to_bone, from_bone, disregard_orientation)
}

fn resolve_matrices(
    parent_transform: Mat4,
    to_bone: Option<Mat4>,
    from_bone: Option<Mat4>,
    dynamic_transform: Option<Mat4>,
    disregard_orientation: bool,
) -> Mat4 {
    parent_transform
        * motion::transformed_attachment_transform(
            to_bone,
            from_bone,
            dynamic_transform,
            disregard_orientation,
        )
}

fn resolve_anchor(
    owner_transform: Mat4,
    to_bone: Option<Mat4>,
    disregard_orientation: bool,
) -> Mat4 {
    owner_transform * attachment_transform(to_bone, None, disregard_orientation)
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::{resolve_anchor, resolve_matrices};

    #[test]
    fn animated_parent_bone_moves_the_aligned_child() {
        let parent = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let from = Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0));
        let first = resolve_matrices(
            parent,
            Some(Mat4::from_translation(Vec3::Y)),
            Some(from),
            None,
            false,
        );
        let animated = resolve_matrices(
            parent,
            Some(Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0))),
            Some(from),
            None,
            false,
        );
        assert!(
            first
                .transform_point3(from.transform_point3(Vec3::ZERO))
                .abs_diff_eq(Vec3::new(10.0, 1.0, 0.0), 1.0e-6)
        );
        assert!(
            animated
                .transform_point3(from.transform_point3(Vec3::ZERO))
                .abs_diff_eq(Vec3::new(10.0, 5.0, 0.0), 1.0e-6)
        );
    }

    #[test]
    fn missing_bones_retain_the_parent_component_transform() {
        let parent = Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0));
        assert!(resolve_matrices(parent, None, None, None, false).abs_diff_eq(parent, 0.0));
    }

    #[test]
    fn disregard_orientation_retains_only_local_translation() {
        let parent = Mat4::from_rotation_y(0.25);
        let local = Mat4::from_rotation_x(0.75) * Mat4::from_translation(Vec3::new(2.0, 3.0, 4.0));
        let resolved = resolve_matrices(parent, Some(local), None, None, true);

        assert!(resolved.abs_diff_eq(
            parent * Mat4::from_translation(local.w_axis.truncate()),
            1.0e-6
        ));
    }

    #[test]
    fn animated_owner_bone_moves_an_effect_anchor() {
        let owner = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let first = resolve_anchor(
            owner,
            Some(Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0))),
            false,
        );
        let animated = resolve_anchor(
            owner,
            Some(Mat4::from_translation(Vec3::new(0.0, 7.0, 0.0))),
            false,
        );

        assert!(
            first
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(10.0, 2.0, 0.0), 1.0e-6)
        );
        assert!(
            animated
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(10.0, 7.0, 0.0), 1.0e-6)
        );
    }

    #[test]
    fn simulation_hardpoint_transform_is_between_attachment_bones() {
        let parent = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let to = Mat4::from_translation(Vec3::Y * 2.0);
        let from = Mat4::from_translation(Vec3::Z);
        let dynamic = Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);

        let resolved = resolve_matrices(parent, Some(to), Some(from), Some(dynamic), false);

        assert!(resolved.abs_diff_eq(parent * to * dynamic * from.inverse(), 1.0e-6));
    }
}
