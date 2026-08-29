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
        Self::at_position(animation, 0.0)
    }

    pub(super) fn at_position(animation: &Animation, normalized_position: f32) -> Self {
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
            match track_matrix(track, normalized_position) {
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

fn track_matrix(
    track: &TransformTrack,
    normalized_position: f32,
) -> Result<Mat4, (&'static str, String)> {
    let position = sample_values_at(&track.position, 3, &[0.0; 3], normalized_position)
        .map_err(|reason| ("position", reason))?;
    let orientation = sample_values_at(
        &track.orientation,
        4,
        &[0.0, 0.0, 0.0, 1.0],
        normalized_position,
    )
    .map_err(|reason| ("orientation", reason))?;
    let scale_shear = sample_values_at(
        &track.scale_shear,
        9,
        &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        normalized_position,
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
fn sample_values(
    curve: &CurveData,
    dimension: usize,
    identity: &[f32],
) -> Result<Vec<f32>, String> {
    sample_values_at(curve, dimension, identity, 0.0)
}

fn sample_values_at(
    curve: &CurveData,
    dimension: usize,
    identity: &[f32],
    normalized_position: f32,
) -> Result<Vec<f32>, String> {
    let position = normalized_position.clamp(0.0, 1.0);
    match &curve.payload {
        CurvePayload::DaKeyframes32f {
            dimension: stored_dimension,
            controls,
        } => {
            require_dimension(
                usize::try_from(*stored_dimension).unwrap_or_default(),
                dimension,
                "keyframes",
            )?;
            sample_even_controls(controls, dimension, position)
        }
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
        } => sample_knotted_controls(knots, controls, dimension, position),
        _ => sample_compressed_values(curve, dimension, position),
    }
}

fn sample_compressed_values(
    curve: &CurveData,
    dimension: usize,
    position: f32,
) -> Result<Vec<f32>, String> {
    match &curve.payload {
        CurvePayload::D4nK16uC15u {
            scale_offset_table_entries,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 4, "D4nK16uC15u")?;
            sample_quantized_quaternion::<u16>(
                *scale_offset_table_entries,
                knots_controls,
                position,
            )
            .map(|values| values.to_vec())
        }
        CurvePayload::D4nK8uC7u {
            scale_offset_table_entries,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 4, "D4nK8uC7u")?;
            sample_quantized_quaternion::<u8>(*scale_offset_table_entries, knots_controls, position)
                .map(|values| values.to_vec())
        }
        CurvePayload::D3K16uC16u {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 3, "D3K16uC16u")?;
            sample_quantized_vec3::<u16>(control_scales, control_offsets, knots_controls, position)
                .map(|values| values.to_vec())
        }
        CurvePayload::D3K8uC8u {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 3, "D3K8uC8u")?;
            sample_quantized_vec3::<u8>(control_scales, control_offsets, knots_controls, position)
                .map(|values| values.to_vec())
        }
        CurvePayload::D3I1K32fC32f {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 3, "D3I1K32fC32f")?;
            sample_identity_vec3(control_scales, control_offsets, knots_controls, position)
                .map(|values| values.to_vec())
        }
        CurvePayload::D3I1K16uC16u {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 3, "D3I1K16uC16u")?;
            sample_identity_quantized_vec3(
                control_scales,
                control_offsets,
                knots_controls,
                position,
            )
            .map(|values| values.to_vec())
        }
        CurvePayload::D3I1K8uC8u {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => {
            require_dimension(dimension, 3, "D3I1K8uC8u")?;
            sample_identity_quantized_vec3(
                control_scales,
                control_offsets,
                knots_controls,
                position,
            )
            .map(|values| values.to_vec())
        }
        _ => Err(format!("unsupported curve format {}", curve.format)),
    }
}

fn sample_even_controls(
    controls: &[f32],
    dimension: usize,
    position: f32,
) -> Result<Vec<f32>, String> {
    if dimension == 0 || !controls.len().is_multiple_of(dimension) {
        return Err("floating-point curve has incomplete controls".to_owned());
    }
    let count = controls.len() / dimension;
    let (lower, upper, blend) = even_interval(count, position)?;
    interpolate_controls(controls, dimension, lower, upper, blend)
}

fn sample_knotted_controls(
    knots: &[f32],
    controls: &[f32],
    dimension: usize,
    position: f32,
) -> Result<Vec<f32>, String> {
    if controls.len() < knots.len().saturating_mul(dimension) {
        return Err("floating-point curve has incomplete controls".to_owned());
    }
    let (lower, upper, blend) = knot_interval(knots, position)?;
    interpolate_controls(controls, dimension, lower, upper, blend)
}

fn interpolate_controls(
    controls: &[f32],
    dimension: usize,
    lower: usize,
    upper: usize,
    blend: f32,
) -> Result<Vec<f32>, String> {
    let lower = controls
        .get(lower.saturating_mul(dimension)..lower.saturating_add(1).saturating_mul(dimension))
        .ok_or_else(|| "curve has no complete lower control".to_owned())?;
    let upper = controls
        .get(upper.saturating_mul(dimension)..upper.saturating_add(1).saturating_mul(dimension))
        .ok_or_else(|| "curve has no complete upper control".to_owned())?;
    Ok(lower
        .iter()
        .zip(upper)
        .map(|(lower, upper)| lower + (upper - lower) * blend)
        .collect())
}

fn even_interval(count: usize, position: f32) -> Result<(usize, usize, f32), String> {
    if count == 0 {
        return Err("curve has no controls".to_owned());
    }
    let scaled =
        position * num_traits::ToPrimitive::to_f32(&count.saturating_sub(1)).unwrap_or(0.0);
    let lower = num_traits::ToPrimitive::to_usize(&scaled.floor()).unwrap_or_default();
    let upper = lower.saturating_add(1).min(count - 1);
    Ok((lower, upper, scaled.fract()))
}

fn knot_interval(knots: &[f32], position: f32) -> Result<(usize, usize, f32), String> {
    let Some((&first, &last)) = knots.first().zip(knots.last()) else {
        return Err("curve has no knots".to_owned());
    };
    if knots.len() == 1 || !first.is_finite() || !last.is_finite() || last <= first {
        return even_interval(knots.len(), position);
    }
    let target = first + (last - first) * position;
    let upper = knots
        .partition_point(|knot| *knot < target)
        .min(knots.len() - 1);
    let lower = upper.saturating_sub(1);
    let span = knots[upper] - knots[lower];
    let blend = if span > f32::EPSILON {
        ((target - knots[lower]) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Ok((lower, upper, blend))
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

fn sample_quantized_quaternion<T: Quantized>(
    table_entries: u16,
    data: &[T],
    position: f32,
) -> Result<[f32; 4], String> {
    let knot_count = data.len() / 4;
    let (lower, upper, blend) = quantized_knot_interval(data, knot_count, position)?;
    let lower = Quat::from_array(decode_quantized_quaternion_at(table_entries, data, lower)?);
    let upper = Quat::from_array(decode_quantized_quaternion_at(table_entries, data, upper)?);
    Ok(lower.slerp(upper, blend).normalize().to_array())
}

fn decode_quantized_quaternion_at<T: Quantized>(
    table_entries: u16,
    data: &[T],
    index: usize,
) -> Result<[f32; 4], String> {
    let knot_count = data.len() / 4;
    if index >= knot_count {
        return Err("quantized quaternion has no requested control".to_owned());
    }
    let control_offset = knot_count + index.saturating_mul(3);
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

#[cfg(test)]
fn decode_quantized_vec3<T: Quantized>(
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[T],
) -> Result<[f32; 3], String> {
    decode_quantized_vec3_at(scales, offsets, data, 0)
}

fn sample_quantized_vec3<T: Quantized>(
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[T],
    position: f32,
) -> Result<[f32; 3], String> {
    let knot_count = data.len() / 4;
    let (lower, upper, blend) = quantized_knot_interval(data, knot_count, position)?;
    let lower = decode_quantized_vec3_at(scales, offsets, data, lower)?;
    let upper = decode_quantized_vec3_at(scales, offsets, data, upper)?;
    Ok(core::array::from_fn(|component| {
        lower[component] + (upper[component] - lower[component]) * blend
    }))
}

fn decode_quantized_vec3_at<T: Quantized>(
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[T],
    index: usize,
) -> Result<[f32; 3], String> {
    let knot_count = data.len() / 4;
    if index >= knot_count {
        return Err("quantized D3 curve has no requested control".to_owned());
    }
    let control_offset = knot_count + index.saturating_mul(3);
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

fn sample_identity_quantized_vec3<T: Quantized>(
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[T],
    position: f32,
) -> Result<[f32; 3], String> {
    let knot_count = data.len() / 2;
    let (lower, upper, blend) = quantized_knot_interval(data, knot_count, position)?;
    let lower = data
        .get(knot_count + lower)
        .copied()
        .map(T::widen)
        .ok_or_else(|| "identity-quantized D3 curve has no lower control".to_owned())?;
    let upper = data
        .get(knot_count + upper)
        .copied()
        .map(T::widen)
        .ok_or_else(|| "identity-quantized D3 curve has no upper control".to_owned())?;
    let parameter = f32::from(lower) + (f32::from(upper) - f32::from(lower)) * blend;
    Ok(core::array::from_fn(|component| {
        offsets[component] + scales[component] * parameter
    }))
}

fn sample_identity_vec3(
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[f32],
    position: f32,
) -> Result<[f32; 3], String> {
    let knot_count = data.len() / 2;
    let (lower, upper, blend) = knot_interval(
        data.get(..knot_count)
            .ok_or_else(|| "identity D3 curve has no knots".to_owned())?,
        position,
    )?;
    let lower = *data
        .get(knot_count + lower)
        .ok_or_else(|| "identity D3 curve has no lower control".to_owned())?;
    let upper = *data
        .get(knot_count + upper)
        .ok_or_else(|| "identity D3 curve has no upper control".to_owned())?;
    let parameter = lower + (upper - lower) * blend;
    Ok(core::array::from_fn(|component| {
        offsets[component] + scales[component] * parameter
    }))
}

fn quantized_knot_interval<T: Quantized>(
    data: &[T],
    knot_count: usize,
    position: f32,
) -> Result<(usize, usize, f32), String> {
    let knots = data
        .get(..knot_count)
        .ok_or_else(|| "quantized curve has no knots".to_owned())?
        .iter()
        .copied()
        .map(T::widen)
        .map(f32::from)
        .collect::<Vec<_>>();
    knot_interval(&knots, position)
}

#[cfg(test)]
mod tests {
    use pipeline::uax::types::{CurveData, CurvePayload};

    use super::{decode_quantized_vec3, sample_values, sample_values_at};

    #[test]
    fn keyframes_interpolate_at_the_sim_owned_playback_position() {
        let curve = CurveData {
            format: 0,
            degree: 1,
            payload: CurvePayload::DaKeyframes32f {
                dimension: 3,
                controls: vec![0.0, 10.0, 20.0, 20.0, 30.0, 40.0],
            },
        };

        assert_eq!(
            sample_values_at(&curve, 3, &[0.0; 3], 0.25).unwrap(),
            [5.0, 15.0, 25.0]
        );
    }

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
