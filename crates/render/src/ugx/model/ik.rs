//! Granny-compatible cyclic-coordinate-descent inverse kinematics.

use glam::{Mat3, Mat4, Quat, Vec3};

use super::{Model, ModelPose};

const CCD_ITERATIONS: usize = 20;
const ROTATION_EPSILON: f32 = 1.0e-12;

impl Model {
    /// Pre-multiply one animated local bone by retail's `SingleBoneIK` transform.
    pub(in crate::ugx) fn apply_single_bone_ik(
        &self,
        pose: &mut ModelPose,
        bone: &str,
        transform: Mat4,
    ) -> bool {
        let Some(index) = self.bone_index(bone) else {
            return false;
        };
        let Some(local) = pose.local_transforms.get_mut(index) else {
            return false;
        };
        *local = motion::premultiplied_orientation_transform(*local, transform);
        self.rebuild_pose(pose);
        true
    }

    /// Rotate an end-effector's parent chain toward a model-space target.
    pub(in crate::ugx) fn apply_ccd_ik(
        &self,
        pose: &mut ModelPose,
        end_effector: &str,
        link_count: usize,
        desired_position: Vec3,
    ) -> bool {
        if !desired_position.is_finite() {
            return false;
        }
        let Some(end_index) = self.bone_index(end_effector) else {
            return false;
        };
        let links = self.ik_links(end_index, link_count);
        if links.len() < 2 {
            return false;
        }

        for _ in 0..CCD_ITERATIONS {
            for &bone_index in links.iter().skip(1) {
                let Some(increment) =
                    Self::ccd_increment(pose, end_index, bone_index, desired_position)
                else {
                    continue;
                };
                let Some(local) = pose.local_transforms.get_mut(bone_index) else {
                    continue;
                };
                *local = rotate_local_orientation(*local, increment);
                self.rebuild_pose(pose);
            }
        }
        true
    }

    fn bone_index(&self, name: &str) -> Option<usize> {
        self.bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(name))
    }

    fn ik_links(&self, end_index: usize, link_count: usize) -> Vec<usize> {
        let mut links = Vec::with_capacity(link_count.saturating_add(1));
        links.push(end_index);
        let mut current = end_index;
        for _ in 0..link_count {
            let Some(parent) = self.bones.get(current).and_then(|bone| bone.parent_index) else {
                break;
            };
            if parent >= self.bones.len() {
                break;
            }
            links.push(parent);
            current = parent;
        }
        links
    }

    fn ccd_increment(
        pose: &ModelPose,
        end_index: usize,
        bone_index: usize,
        desired_position: Vec3,
    ) -> Option<Quat> {
        let end_position = pose.bone_to_model.get(end_index)?.w_axis.truncate();
        let absolute = *pose.bone_to_model.get(bone_index)?;
        let origin = absolute.w_axis.truncate();
        let transpose = Mat3::from_cols(
            absolute.x_axis.truncate(),
            absolute.y_axis.truncate(),
            absolute.z_axis.truncate(),
        )
        .transpose();
        let desired = transpose * (desired_position - origin);
        let actual = transpose * (end_position - origin);
        let half_vector = (actual + desired) * 0.5;
        let increment = Quat::from_xyzw(
            half_vector.cross(desired).x,
            half_vector.cross(desired).y,
            half_vector.cross(desired).z,
            half_vector.dot(desired),
        );
        let length_squared = increment.length_squared();
        (length_squared.is_finite() && length_squared > ROTATION_EPSILON)
            .then(|| increment / length_squared.sqrt())
    }

    fn rebuild_pose(&self, pose: &mut ModelPose) {
        pose.bone_to_model.clear();
        pose.bone_to_model.reserve(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            let local = pose
                .local_transforms
                .get(index)
                .copied()
                .unwrap_or(bone.bind_local);
            let world = bone
                .parent_index
                .filter(|&parent| parent < index)
                .and_then(|parent| pose.bone_to_model.get(parent).copied())
                .map_or(local, |parent| parent * local);
            pose.bone_to_model.push(world);
        }
        pose.joint_matrices.fill(Mat4::IDENTITY);
        for (index, (bone, world)) in self.bones.iter().zip(&pose.bone_to_model).enumerate() {
            if let Some(joint) = pose.joint_matrices.get_mut(index + 1) {
                *joint = *world * bone.inverse_bind;
            }
        }
    }
}

fn rotate_local_orientation(local: Mat4, increment: Quat) -> Mat4 {
    let linear = Mat3::from_cols(
        local.x_axis.truncate(),
        local.y_axis.truncate(),
        local.z_axis.truncate(),
    );
    let rotation = polar_rotation(linear);
    let scale_shear = rotation.transpose() * linear;
    let orientation = Quat::from_mat3(&rotation).normalize() * increment;
    let rotated = Mat3::from_quat(orientation.normalize()) * scale_shear;
    Mat4::from_cols(
        rotated.x_axis.extend(0.0),
        rotated.y_axis.extend(0.0),
        rotated.z_axis.extend(0.0),
        local.w_axis,
    )
}

fn polar_rotation(linear: Mat3) -> Mat3 {
    if !linear.is_finite() || linear.determinant().abs() <= f32::EPSILON {
        return Mat3::IDENTITY;
    }
    let mut rotation = linear;
    for _ in 0..8 {
        let determinant = rotation.determinant();
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return Mat3::IDENTITY;
        }
        rotation = (rotation + rotation.inverse().transpose()) * 0.5;
    }
    if rotation.determinant() < 0.0 {
        rotation.x_axis = -rotation.x_axis;
    }
    rotation
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::super::{Bone, Model};

    fn two_link_model() -> Model {
        let bone = |name: &str, parent_index, translation| Bone {
            name: name.to_owned(),
            parent_index,
            bind_local: Mat4::from_translation(translation),
            inverse_bind: Mat4::IDENTITY,
            bind_world: Mat4::IDENTITY,
        };
        Model {
            asset_path: "ik-test".to_owned(),
            bounds_min: [0.0; 3],
            bounds_max: [0.0; 3],
            sphere_center: [0.0; 3],
            sphere_radius: 0.0,
            materials: Vec::new(),
            sections: Vec::new(),
            joint_count: 4,
            bones: vec![
                bone("root", None, Vec3::ZERO),
                bone("link", Some(0), Vec3::X),
                bone("end", Some(1), Vec3::X),
            ],
            upper_body_weights: vec![0.0; 3],
        }
    }

    #[test]
    fn granny_ccd_chain_reaches_a_model_space_target() {
        let model = two_link_model();
        let mut pose = model.pose(None);
        let target = Vec3::new(1.0, 1.0, 0.0);

        assert!(model.apply_ccd_ik(&mut pose, "end", 2, target));
        let end = model
            .posed_bone_to_model(&pose, "end")
            .unwrap()
            .w_axis
            .truncate();
        assert!(end.abs_diff_eq(target, 1.0e-3), "end={end:?}");
    }

    #[test]
    fn missing_end_effector_leaves_the_pose_unchanged() {
        let model = two_link_model();
        let mut pose = model.pose(None);
        let before = pose.clone();

        assert!(!model.apply_ccd_ik(&mut pose, "missing", 2, Vec3::Y));
        assert_eq!(pose.joint_matrices, before.joint_matrices);
    }

    #[test]
    fn single_bone_ik_pre_multiplies_orientation_without_moving_the_bone() {
        let model = two_link_model();
        let mut pose = model.pose(None);
        let index = model.bone_index("link").unwrap();
        pose.local_transforms[index] =
            Mat4::from_rotation_x(0.25) * Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let before_translation = pose.local_transforms[index].w_axis;
        let dynamic = Mat4::from_rotation_y(0.5);

        assert!(model.apply_single_bone_ik(&mut pose, "link", dynamic));

        let local = pose.local_transforms[index];
        assert!(local.w_axis.abs_diff_eq(before_translation, 1.0e-6));
        let expected = Mat4::from_rotation_y(0.5) * Mat4::from_rotation_x(0.25);
        for (actual, expected) in local
            .to_cols_array()
            .into_iter()
            .take(12)
            .zip(expected.to_cols_array())
        {
            assert!((actual - expected).abs() <= 1.0e-5);
        }
    }
}
