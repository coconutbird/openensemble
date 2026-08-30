//! Granny-compatible single-bone orientation transforms shared by sim and rendering.

use glam::{Mat3, Mat4};

/// Pre-multiply a local bone orientation while retaining its translation and scale/shear.
#[must_use]
pub fn premultiplied_orientation_transform(local: Mat4, transform: Mat4) -> Mat4 {
    let linear = linear_part(local);
    let rotation = polar_rotation(linear);
    let scale_shear = rotation.transpose() * linear;
    let dynamic = polar_rotation(linear_part(transform));
    let rotated = dynamic * rotation * scale_shear;
    Mat4::from_cols(
        rotated.x_axis.extend(0.0),
        rotated.y_axis.extend(0.0),
        rotated.z_axis.extend(0.0),
        local.w_axis,
    )
}

fn linear_part(transform: Mat4) -> Mat3 {
    Mat3::from_cols(
        transform.x_axis.truncate(),
        transform.y_axis.truncate(),
        transform.z_axis.truncate(),
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

    use super::premultiplied_orientation_transform;

    #[test]
    fn dynamic_orientation_does_not_rotate_local_translation() {
        let local = Mat4::from_rotation_x(0.25);
        let mut local = local;
        local.w_axis = Vec3::new(1.0, 2.0, 3.0).extend(1.0);
        let transformed = premultiplied_orientation_transform(
            local,
            Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2),
        );

        assert!(transformed.w_axis.abs_diff_eq(local.w_axis, 1.0e-6));
        let expected =
            Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2) * Mat4::from_rotation_x(0.25);
        assert!(transformed.x_axis.abs_diff_eq(expected.x_axis, 1.0e-5));
        assert!(transformed.y_axis.abs_diff_eq(expected.y_axis, 1.0e-5));
        assert!(transformed.z_axis.abs_diff_eq(expected.z_axis, 1.0e-5));
    }
}
