//! Shared evaluation of authored model-to-model bone attachments.

use glam::Mat4;

/// Align a child component's optional source bone to a parent's optional target bone.
///
/// Source asset matrices use row vectors. The matrices exposed by the asset pipeline are
/// transposed into glam's column-vector convention, so retail's `inverse(from) * to`
/// expression becomes `to * inverse(from)` here.
#[must_use]
pub fn attachment_transform(
    to_bone: Option<Mat4>,
    from_bone: Option<Mat4>,
    disregard_orientation: bool,
) -> Mat4 {
    transformed_attachment_transform(to_bone, from_bone, None, disregard_orientation)
}

/// Align an attachment while inserting retail's mutable attachment transform.
///
/// Hardpoints store their live yaw/pitch orientation in this middle transform.
#[must_use]
pub fn transformed_attachment_transform(
    to_bone: Option<Mat4>,
    from_bone: Option<Mat4>,
    transform: Option<Mat4>,
    disregard_orientation: bool,
) -> Mat4 {
    let to = to_bone.unwrap_or(Mat4::IDENTITY);
    let transform = transform.unwrap_or(Mat4::IDENTITY);
    let from = from_bone.map_or(Mat4::IDENTITY, |matrix| matrix.inverse());
    let transform = to * transform * from;
    if disregard_orientation {
        Mat4::from_translation(transform.w_axis.truncate())
    } else {
        transform
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::{attachment_transform, transformed_attachment_transform};

    #[test]
    fn source_bone_is_aligned_to_target_bone() {
        let to = Mat4::from_translation(Vec3::new(2.0, 5.0, 7.0));
        let from = Mat4::from_translation(Vec3::new(0.0, 3.0, 0.0));
        let transform = attachment_transform(Some(to), Some(from), false);

        assert!(
            transform
                .transform_point3(from.transform_point3(Vec3::ZERO))
                .abs_diff_eq(to.transform_point3(Vec3::ZERO), 1.0e-6)
        );
    }

    #[test]
    fn disregarded_orientation_retains_alignment_translation() {
        let to = Mat4::from_rotation_y(0.75) * Mat4::from_translation(Vec3::new(2.0, 5.0, 7.0));
        let transform = attachment_transform(Some(to), None, true);

        assert!(transform.x_axis.abs_diff_eq(Mat4::IDENTITY.x_axis, 1.0e-6));
        assert!(transform.y_axis.abs_diff_eq(Mat4::IDENTITY.y_axis, 1.0e-6));
        assert!(transform.z_axis.abs_diff_eq(Mat4::IDENTITY.z_axis, 1.0e-6));
        assert!(transform.w_axis.abs_diff_eq(to.w_axis, 1.0e-6));
    }

    #[test]
    fn mutable_transform_is_inserted_between_target_and_source_bones() {
        let to = Mat4::from_translation(Vec3::new(2.0, 5.0, 7.0));
        let from = Mat4::from_translation(Vec3::new(0.0, 3.0, 0.0));
        let rotation = Mat4::from_rotation_y(0.5);
        let transform =
            transformed_attachment_transform(Some(to), Some(from), Some(rotation), false);

        assert!(transform.abs_diff_eq(to * rotation * from.inverse(), 1.0e-6));
    }
}
