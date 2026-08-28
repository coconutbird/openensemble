use glam::{Mat3, Mat4, Quat, Vec3};
use pipeline::uax::types::{Animation, CurveData, CurvePayload, TransformTrack};

const QUATERNION_SCALE_OFFSET: [[f32; 2]; 16] = {
    const H: f32 = 0.707_106_77;
    [
        [H * 2.0, -H],
        [H, -H * 0.5],
        [H * 0.5, -H * 0.75],
        [H * 0.5, -H * 0.25],
        [H * 0.5, H * 0.25],
        [H * 0.25, -H * 0.25],
        [H * 0.25, -H * 0.125],
        [H * 0.25, 0.0],
        [-H * 2.0, H],
        [-H, H * 0.5],
        [-H * 0.5, H * 0.75],
        [-H * 0.5, H * 0.25],
        [-H * 0.5, -H * 0.25],
        [-H * 0.25, H * 0.25],
        [-H * 0.25, H * 0.125],
        [-H * 0.25, 0.0],
    ]
};

#[derive(Clone, Debug, Default)]
pub(super) struct AnimationPose {
    tracks: Vec<(String, Mat4)>,
}

impl AnimationPose {
    pub(super) fn at_start(animation: &Animation) -> Self {
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
            match track_matrix(track) {
                Ok(matrix) => tracks.push((name.to_owned(), matrix)),
                Err((component, reason)) => log::warn!(
                    "UGX animation track '{name}' has an unsupported {component} curve; using the bone's bind pose: {reason}"
                ),
            }
        }
        Self { tracks }
    }

    pub(super) fn local_transform(&self, bone_name: &str) -> Option<Mat4> {
        self.tracks
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(bone_name))
            .map(|(_, matrix)| *matrix)
    }
}

fn track_matrix(track: &TransformTrack) -> Result<Mat4, (&'static str, String)> {
    let position =
        sample_values(&track.position, 3, &[0.0; 3]).map_err(|reason| ("position", reason))?;
    let orientation = sample_values(&track.orientation, 4, &[0.0, 0.0, 0.0, 1.0])
        .map_err(|reason| ("orientation", reason))?;
    let scale_shear = sample_values(
        &track.scale_shear,
        9,
        &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
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

fn sample_values(
    curve: &CurveData,
    dimension: usize,
    identity: &[f32],
) -> Result<Vec<f32>, String> {
    match &curve.payload {
        CurvePayload::Identity {
            dimension: stored_dimension,
        } => {
            // A null Granny curve is represented as identity dimension zero.
            // It is valid for every transform component.
            if *stored_dimension != 0 && usize::try_from(*stored_dimension).ok() != Some(dimension)
            {
                return Err(format!(
                    "identity dimension {stored_dimension} does not match {dimension}"
                ));
            }
            Ok(identity.to_vec())
        }
        CurvePayload::DaConstant32f { controls, .. } => {
            exact_dimension(controls, dimension, "constant")
        }
        CurvePayload::D3Constant32f { controls, .. } => {
            exact_dimension(controls, dimension, "D3 constant")
        }
        CurvePayload::D4Constant32f { controls, .. } => {
            exact_dimension(controls, dimension, "D4 constant")
        }
        CurvePayload::DaK32fC32f {
            knots, controls, ..
        } => {
            if knots.is_empty() {
                return Err("floating-point curve has no knots".to_owned());
            }
            controls
                .get(..dimension)
                .map(<[f32]>::to_vec)
                .ok_or_else(|| "floating-point curve has no complete first control".to_owned())
        }
        CurvePayload::D4nK16uC15u {
            scale_offset_table_entries,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 4, "D4nK16uC15u")?;
            decode_quantized_quaternion::<u16>(*scale_offset_table_entries, knots_controls)
                .map(|values| values.to_vec())
        }
        CurvePayload::D4nK8uC7u {
            scale_offset_table_entries,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 4, "D4nK8uC7u")?;
            decode_quantized_quaternion::<u8>(*scale_offset_table_entries, knots_controls)
                .map(|values| values.to_vec())
        }
        CurvePayload::D3K16uC16u {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 3, "D3K16uC16u")?;
            decode_quantized_vec3::<u16>(control_scales, control_offsets, knots_controls)
                .map(|values| values.to_vec())
        }
        CurvePayload::D3K8uC8u {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 3, "D3K8uC8u")?;
            decode_quantized_vec3::<u8>(control_scales, control_offsets, knots_controls)
                .map(|values| values.to_vec())
        }
        CurvePayload::D3I1K8uC8u {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 3, "D3I1K8uC8u")?;
            decode_identity_quantized_vec3(control_scales, control_offsets, knots_controls)
                .map(|values| values.to_vec())
        }
        _ => Err(format!("unsupported curve format {}", curve.format)),
    }
}

fn exact_dimension(values: &[f32], dimension: usize, label: &str) -> Result<Vec<f32>, String> {
    if values.len() != dimension {
        return Err(format!(
            "{label} has {} values instead of {dimension}",
            values.len()
        ));
    }
    Ok(values.to_vec())
}

fn require_dimension(actual: usize, expected: usize, label: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{label} produces {expected} values instead of {actual}"
        ))
    }
}

trait Quantized: Copy {
    const CONTROL_MASK: u16;
    const CONTROL_MAX: f32;
    const SIGN_MASK: u16;
    const INDEX_SHIFT: u32;

    fn widen(self) -> u16;
}

impl Quantized for u8 {
    const CONTROL_MASK: u16 = 0x7f;
    const CONTROL_MAX: f32 = 127.0;
    const SIGN_MASK: u16 = 0x80;
    const INDEX_SHIFT: u32 = 7;

    fn widen(self) -> u16 {
        u16::from(self)
    }
}

impl Quantized for u16 {
    const CONTROL_MASK: u16 = 0x7fff;
    const CONTROL_MAX: f32 = 32_767.0;
    const SIGN_MASK: u16 = 0x8000;
    const INDEX_SHIFT: u32 = 15;

    fn widen(self) -> u16 {
        self
    }
}

fn decode_quantized_quaternion<T: Quantized>(
    table_entries: u16,
    data: &[T],
) -> Result<[f32; 4], String> {
    let knot_count = data.len() / 4;
    if knot_count == 0 {
        return Err("quantized quaternion has no knots".to_owned());
    }
    let control_offset = knot_count;
    let packed = [
        data.get(control_offset).copied().map(T::widen),
        data.get(control_offset + 1).copied().map(T::widen),
        data.get(control_offset + 2).copied().map(T::widen),
    ];
    let [Some(first), Some(second), Some(third)] = packed else {
        return Err("quantized quaternion has no complete first control".to_owned());
    };
    let packed = [first, second, third];
    let missing_is_negative = packed[0] & T::SIGN_MASK != 0;
    let missing_index =
        (((packed[1] >> (T::INDEX_SHIFT - 1)) & 0x2) | (packed[2] >> T::INDEX_SHIFT)) as usize;
    if missing_index >= 4 {
        return Err(format!(
            "invalid missing quaternion component {missing_index}"
        ));
    }

    let mut result = [0.0; 4];
    let mut sum_squared = 0.0;
    let mut destination = missing_index;
    for value in packed {
        destination = (destination + 1) & 3;
        let table_index = usize::from((table_entries >> (destination * 4)) & 0xf);
        let [range, offset] = QUATERNION_SCALE_OFFSET[table_index];
        let decoded = offset + range * f32::from(value & T::CONTROL_MASK) / T::CONTROL_MAX;
        result[destination] = decoded;
        sum_squared += decoded * decoded;
    }
    result[missing_index] =
        (1.0 - sum_squared).max(0.0).sqrt() * if missing_is_negative { -1.0 } else { 1.0 };
    let quaternion = Quat::from_array(result);
    if quaternion.length_squared() <= f32::EPSILON {
        return Err("quantized quaternion decoded to zero".to_owned());
    }
    Ok(quaternion.normalize().to_array())
}

fn decode_quantized_vec3<T: Quantized>(
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[T],
) -> Result<[f32; 3], String> {
    let knot_count = data.len() / 4;
    if knot_count == 0 {
        return Err("quantized D3 curve has no knots".to_owned());
    }
    let control_offset = knot_count;
    let mut result = [0.0; 3];
    for (component, output) in result.iter_mut().enumerate() {
        let value = data
            .get(control_offset + component)
            .copied()
            .map(T::widen)
            .ok_or_else(|| "quantized D3 curve has no complete first control".to_owned())?;
        *output = offsets[component] + scales[component] * f32::from(value);
    }
    Ok(result)
}

fn decode_identity_quantized_vec3(
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[u8],
) -> Result<[f32; 3], String> {
    let knot_count = data.len() / 2;
    let parameter = data
        .get(knot_count)
        .copied()
        .ok_or_else(|| "identity-quantized D3 curve has no first control".to_owned())?;
    Ok(core::array::from_fn(|component| {
        offsets[component] + scales[component] * f32::from(parameter)
    }))
}

#[cfg(test)]
mod tests {
    use pipeline::uax::types::{CurveData, CurvePayload};

    use super::{decode_quantized_vec3, sample_values};

    #[test]
    fn identity_curves_use_the_component_identity() {
        let curve = CurveData {
            format: 2,
            degree: 0,
            payload: CurvePayload::Identity { dimension: 4 },
        };
        assert_eq!(
            sample_values(&curve, 4, &[0.0, 0.0, 0.0, 1.0]).unwrap(),
            [0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn null_identity_curves_use_the_requested_component_identity() {
        let curve = CurveData {
            format: 0,
            degree: 0,
            payload: CurvePayload::Identity { dimension: 0 },
        };
        assert_eq!(
            sample_values(&curve, 3, &[4.0, 5.0, 6.0]).unwrap(),
            [4.0, 5.0, 6.0]
        );
    }

    #[test]
    fn u16_curve_uses_the_first_control_after_the_knot_block() {
        // Three knots are followed by three three-component controls. Sampling
        // at time zero reads the first complete control after the knot block.
        let data = [0_u16, 1, 2, 10, 20, 30, 11, 21, 31, 12, 22, 32];
        let actual = decode_quantized_vec3::<u16>(&[1.0; 3], &[0.0; 3], &data).unwrap();
        assert!(
            actual
                .iter()
                .zip([10.0, 20.0, 30.0])
                .all(|(actual, expected)| (actual - expected).abs() < f32::EPSILON)
        );
    }
}
