mod curve;

use glam::{Mat3, Mat4, Quat, Vec3};
use pipeline::uax::types::{Animation, TransformTrack};

use self::curve::{AnimationSample, sample_curve};

#[derive(Clone, Debug, Default)]
pub struct AnimationPose {
    tracks: Vec<(String, Mat4)>,
}

impl AnimationPose {
    #[must_use]
    pub fn at_start(animation: &Animation) -> Self {
        Self::at_position(animation, 0.0)
    }

    #[must_use]
    pub fn at_position(animation: &Animation, normalized_position: f32) -> Self {
        let sample = AnimationSample::from_normalized(animation, normalized_position);
        let mut tracks: Vec<(String, Mat4)> = Vec::new();
        for track in animation
            .track_groups
            .iter()
            .flat_map(|group| &group.transform_tracks)
        {
            let Some(name) = track.name.as_deref().filter(|name| !name.is_empty()) else {
                continue;
            };
            if tracks
                .iter()
                .any(|(existing, _)| existing.eq_ignore_ascii_case(name))
            {
                continue;
            }
            match track_matrix(track, sample) {
                Ok(matrix) => tracks.push((name.to_owned(), matrix)),
                Err((component, reason)) => log::warn!(
                    "UGX animation track '{name}' has an unsupported {component} curve; using the bone's bind pose: {reason}"
                ),
            }
        }
        Self { tracks }
    }

    #[must_use]
    pub fn local_transform(&self, bone_name: &str) -> Option<Mat4> {
        self.tracks
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(bone_name))
            .map(|(_, matrix)| *matrix)
    }

    #[must_use]
    pub fn blended(
        previous: &Self,
        current: &Self,
        weight: f32,
        bind_local: impl Fn(&str) -> Option<Mat4>,
    ) -> Self {
        if weight <= 0.0 {
            return previous.clone();
        }
        if weight >= 1.0 {
            return current.clone();
        }
        let mut tracks = Vec::with_capacity(previous.tracks.len().max(current.tracks.len()));
        for (name, current_transform) in &current.tracks {
            let previous_transform = previous
                .local_transform(name)
                .or_else(|| bind_local(name))
                .unwrap_or(*current_transform);
            tracks.push((
                name.clone(),
                blended_local_transform(*current_transform, previous_transform, weight),
            ));
        }
        for (name, previous_transform) in &previous.tracks {
            if current.local_transform(name).is_none() {
                let current_transform = bind_local(name).unwrap_or(*previous_transform);
                tracks.push((
                    name.clone(),
                    blended_local_transform(current_transform, *previous_transform, weight),
                ));
            }
        }
        Self { tracks }
    }
}

#[must_use]
pub fn blended_local_transform(action: Mat4, movement: Mat4, weight: f32) -> Mat4 {
    if weight <= 0.0 {
        return movement;
    }
    if weight >= 1.0 {
        return action;
    }
    let (action_scale, action_rotation, action_translation) =
        action.to_scale_rotation_translation();
    let (movement_scale, movement_rotation, movement_translation) =
        movement.to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(
        movement_scale.lerp(action_scale, weight),
        movement_rotation.slerp(action_rotation, weight),
        movement_translation.lerp(action_translation, weight),
    )
}

fn track_matrix(
    track: &TransformTrack,
    sample: AnimationSample,
) -> Result<Mat4, (&'static str, String)> {
    let position = sample_curve(&track.position, 3, &[0.0; 3], sample, false)
        .map_err(|reason| ("position", reason))?;
    let orientation = sample_curve(&track.orientation, 4, &[0.0, 0.0, 0.0, 1.0], sample, true)
        .map_err(|reason| ("orientation", reason))?;
    let scale_shear = sample_curve(
        &track.scale_shear,
        9,
        &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        sample,
        false,
    )
    .map_err(|reason| ("scale/shear", reason))?;

    let quaternion = Quat::from_xyzw(
        orientation[0],
        orientation[1],
        orientation[2],
        orientation[3],
    );
    let rotation = if quaternion.length_squared() > f32::EPSILON {
        Mat3::from_quat(quaternion.normalize())
    } else {
        Mat3::IDENTITY
    };
    let scale_shear = Mat3::from_cols_array(&[
        scale_shear[0],
        scale_shear[3],
        scale_shear[6],
        scale_shear[1],
        scale_shear[4],
        scale_shear[7],
        scale_shear[2],
        scale_shear[5],
        scale_shear[8],
    ]);
    let composite = rotation * scale_shear;
    Ok(Mat4::from_cols(
        composite.x_axis.extend(0.0),
        composite.y_axis.extend(0.0),
        composite.z_axis.extend(0.0),
        Vec3::new(position[0], position[1], position[2]).extend(1.0),
    ))
}

#[cfg(test)]
mod tween_tests {
    use glam::{Mat4, Vec3};

    use super::AnimationPose;

    #[test]
    fn tween_blends_tracks_missing_from_either_clip_through_bind_pose() {
        let previous = AnimationPose {
            tracks: vec![(
                "previous_only".to_owned(),
                Mat4::from_translation(Vec3::X * 10.0),
            )],
        };
        let current = AnimationPose {
            tracks: vec![(
                "current_only".to_owned(),
                Mat4::from_translation(Vec3::Y * 20.0),
            )],
        };

        let blended = AnimationPose::blended(&previous, &current, 0.5, |_| Some(Mat4::IDENTITY));

        assert_eq!(
            blended
                .local_transform("previous_only")
                .unwrap()
                .w_axis
                .truncate(),
            Vec3::X * 5.0
        );
        assert_eq!(
            blended
                .local_transform("current_only")
                .unwrap()
                .w_axis
                .truncate(),
            Vec3::Y * 10.0
        );
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::blended_local_transform;

    #[test]
    fn track_mask_weight_selects_and_blends_action_over_movement() {
        let action = Mat4::from_translation(Vec3::new(8.0, 4.0, 2.0));
        let movement = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.0));

        assert_eq!(blended_local_transform(action, movement, 0.0), movement);
        assert_eq!(blended_local_transform(action, movement, 1.0), action);
        assert!(
            blended_local_transform(action, movement, 0.25)
                .w_axis
                .truncate()
                .abs_diff_eq(Vec3::new(2.0, 1.0, 0.5), 1.0e-6)
        );
    }
}
