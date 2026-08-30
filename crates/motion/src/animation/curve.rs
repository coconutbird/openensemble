//! Curve decoding for retail Granny/UAX animation payloads.

use num_traits::ToPrimitive;
use pipeline::uax::types::{Animation, CurveData, CurvePayload};

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

#[derive(Clone, Copy, Debug)]
pub(super) struct AnimationSample {
    local_clock: f32,
    frame_index: usize,
}

impl AnimationSample {
    pub(super) fn from_normalized(animation: &Animation, normalized_position: f32) -> Self {
        let position = normalized_position.clamp(0.0, 1.0);
        let duration = if animation.duration.is_finite() {
            animation.duration.max(0.0)
        } else {
            0.0
        };
        let local_clock = position * duration;
        let frame_index = (animation.time_step.is_finite() && animation.time_step > 0.0)
            .then(|| (local_clock / animation.time_step).floor().to_usize())
            .flatten()
            .unwrap_or_default();
        Self {
            local_clock,
            frame_index,
        }
    }

    #[cfg(test)]
    const fn new(local_clock: f32, frame_index: usize) -> Self {
        Self {
            local_clock,
            frame_index,
        }
    }
}

pub(super) fn sample_curve(
    curve: &CurveData,
    dimension: usize,
    identity: &[f32],
    sample: AnimationSample,
    normalized: bool,
) -> Result<Vec<f32>, String> {
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
            sample_keyframes(controls, dimension, sample.frame_index)
        }
        CurvePayload::Identity {
            dimension: stored_dimension,
        } => sample_identity(*stored_dimension, dimension, identity),
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
        } if knots.is_empty() => {
            if controls.is_empty() {
                Ok(identity.to_vec())
            } else {
                sample_keyframes(controls, dimension, sample.frame_index)
            }
        }
        CurvePayload::DaK32fC32f {
            knots, controls, ..
        } if knots.len() == 1 => exact_dimension(controls, dimension, "raw constant"),
        CurvePayload::DaK32fC32f {
            knots, controls, ..
        } => {
            let decoded =
                DecodedCurve::from_float(knots, controls.clone(), dimension, sample.local_clock)?;
            sample_decoded(curve, &decoded, dimension, sample.local_clock, normalized)
        }
        _ => sample_compressed_curve(curve, dimension, sample.local_clock, normalized),
    }
}

fn sample_identity(
    stored_dimension: i16,
    dimension: usize,
    identity: &[f32],
) -> Result<Vec<f32>, String> {
    if stored_dimension != 0 && usize::try_from(stored_dimension).ok() != Some(dimension) {
        return Err(format!(
            "identity dimension {stored_dimension} does not match {dimension}"
        ));
    }
    exact_dimension(identity, dimension, "identity")
}

fn sample_keyframes(
    controls: &[f32],
    dimension: usize,
    frame_index: usize,
) -> Result<Vec<f32>, String> {
    if dimension == 0 || !controls.len().is_multiple_of(dimension) {
        return Err("keyframed curve has incomplete controls".to_owned());
    }
    let frame_count = controls.len() / dimension;
    let frame_index = frame_index.min(frame_count.saturating_sub(1));
    controls
        .get(
            frame_index.saturating_mul(dimension)
                ..frame_index.saturating_add(1).saturating_mul(dimension),
        )
        .map(<[f32]>::to_vec)
        .ok_or_else(|| "keyframed curve has no complete frame".to_owned())
}

fn sample_compressed_curve(
    curve: &CurveData,
    dimension: usize,
    local_clock: f32,
    normalized: bool,
) -> Result<Vec<f32>, String> {
    let decoded = match curve.payload.format() {
        Some(6 | 7) => decode_arbitrary_payload(&curve.payload, dimension, local_clock),
        Some(8 | 9) => decode_quaternion_payload(&curve.payload, dimension, local_clock),
        Some(10 | 11) => decode_vec3_payload(&curve.payload, dimension, local_clock),
        Some(12 | 14) => decode_uniform_payload(&curve.payload, dimension, local_clock),
        Some(13 | 15) => decode_diagonal_payload(&curve.payload, dimension, local_clock),
        Some(16..=18) => decode_parameter_vec3_payload(&curve.payload, dimension, local_clock),
        Some(_) | None => Err(format!("unsupported curve format {}", curve.format)),
    }?;
    sample_decoded(curve, &decoded, dimension, local_clock, normalized)
}

fn decode_arbitrary_payload(
    payload: &CurvePayload,
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    match payload {
        CurvePayload::DaK16uC16u {
            one_over_knot_scale_trunc,
            control_scale_offsets,
            knots_controls,
        } => decode_arbitrary_quantized::<u16>(
            *one_over_knot_scale_trunc,
            control_scale_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        CurvePayload::DaK8uC8u {
            one_over_knot_scale_trunc,
            control_scale_offsets,
            knots_controls,
        } => decode_arbitrary_quantized::<u8>(
            *one_over_knot_scale_trunc,
            control_scale_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        _ => Err("curve payload does not match its arbitrary format".to_owned()),
    }
}

fn decode_quaternion_payload(
    payload: &CurvePayload,
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    match payload {
        CurvePayload::D4nK16uC15u {
            scale_offset_table_entries,
            one_over_knot_scale,
            knots_controls,
        } => {
            require_dimension(dimension, 4, "D4nK16uC15u")?;
            decode_quantized_quaternions::<u16>(
                *scale_offset_table_entries,
                *one_over_knot_scale,
                knots_controls,
                local_clock,
            )
        }
        CurvePayload::D4nK8uC7u {
            scale_offset_table_entries,
            one_over_knot_scale,
            knots_controls,
        } => {
            require_dimension(dimension, 4, "D4nK8uC7u")?;
            decode_quantized_quaternions::<u8>(
                *scale_offset_table_entries,
                *one_over_knot_scale,
                knots_controls,
                local_clock,
            )
        }
        _ => Err("curve payload does not match its quaternion format".to_owned()),
    }
}

fn decode_vec3_payload(
    payload: &CurvePayload,
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    match payload {
        CurvePayload::D3K16uC16u {
            one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
        } => decode_quantized_vec3::<u16>(
            *one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        CurvePayload::D3K8uC8u {
            one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
        } => decode_quantized_vec3::<u8>(
            *one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        _ => Err("curve payload does not match its vector format".to_owned()),
    }
}

fn decode_uniform_payload(
    payload: &CurvePayload,
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    match payload {
        CurvePayload::D9I1K16uC16u {
            one_over_knot_scale_trunc,
            control_scale,
            control_offset,
            knots_controls,
        } => decode_uniform_scale::<u16>(
            *one_over_knot_scale_trunc,
            *control_scale,
            *control_offset,
            knots_controls,
            dimension,
            local_clock,
        ),
        CurvePayload::D9I1K8uC8u {
            one_over_knot_scale_trunc,
            control_scale,
            control_offset,
            knots_controls,
        } => decode_uniform_scale::<u8>(
            *one_over_knot_scale_trunc,
            *control_scale,
            *control_offset,
            knots_controls,
            dimension,
            local_clock,
        ),
        _ => Err("curve payload does not match its uniform-scale format".to_owned()),
    }
}

fn decode_diagonal_payload(
    payload: &CurvePayload,
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    match payload {
        CurvePayload::D9I3K16uC16u {
            one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
        } => decode_diagonal_scale::<u16>(
            *one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        CurvePayload::D9I3K8uC8u {
            one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
        } => decode_diagonal_scale::<u8>(
            *one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        _ => Err("curve payload does not match its diagonal-scale format".to_owned()),
    }
}

fn decode_parameter_vec3_payload(
    payload: &CurvePayload,
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    match payload {
        CurvePayload::D3I1K32fC32f {
            control_scales,
            control_offsets,
            knots_controls,
            ..
        } => decode_identity_float_vec3(
            control_scales,
            control_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        CurvePayload::D3I1K16uC16u {
            one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
        } => decode_identity_quantized_vec3::<u16>(
            *one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        CurvePayload::D3I1K8uC8u {
            one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
        } => decode_identity_quantized_vec3::<u8>(
            *one_over_knot_scale_trunc,
            control_scales,
            control_offsets,
            knots_controls,
            dimension,
            local_clock,
        ),
        _ => Err("curve payload does not match its parameter-vector format".to_owned()),
    }
}

#[derive(Debug)]
struct DecodedCurve {
    knots: Vec<f32>,
    controls: Vec<f32>,
    knot_index: usize,
}

impl DecodedCurve {
    fn from_float(
        knots: &[f32],
        controls: Vec<f32>,
        dimension: usize,
        local_clock: f32,
    ) -> Result<Self, String> {
        validate_curve_shape(knots.len(), controls.len(), dimension)?;
        if knots.iter().any(|knot| !knot.is_finite()) {
            return Err("curve contains a non-finite knot".to_owned());
        }
        let knot_index = knots
            .partition_point(|knot| *knot <= local_clock)
            .min(knots.len() - 1);
        Ok(Self {
            knots: knots.to_vec(),
            controls,
            knot_index,
        })
    }

    fn from_quantized<T: Quantized>(
        data: &[T],
        knot_count: usize,
        controls: Vec<f32>,
        dimension: usize,
        one_over_knot_scale: f32,
        local_clock: f32,
    ) -> Result<Self, String> {
        validate_curve_shape(knot_count, controls.len(), dimension)?;
        if !one_over_knot_scale.is_finite() || one_over_knot_scale <= 0.0 {
            return Err("quantized curve has an invalid knot scale".to_owned());
        }
        let raw_knots = data
            .get(..knot_count)
            .ok_or_else(|| "quantized curve has incomplete knots".to_owned())?;
        let scaled_clock = (local_clock * one_over_knot_scale)
            .floor()
            .clamp(0.0, f32::from(T::KNOT_MAX));
        let quantized_clock = scaled_clock.to_u16().unwrap_or(T::KNOT_MAX);
        let knot_index = raw_knots
            .partition_point(|knot| knot.widen() <= quantized_clock)
            .min(knot_count - 1);
        let knots = raw_knots
            .iter()
            .map(|knot| f32::from(knot.widen()) / one_over_knot_scale)
            .collect();
        Ok(Self {
            knots,
            controls,
            knot_index,
        })
    }
}

fn validate_curve_shape(
    knot_count: usize,
    control_count: usize,
    dimension: usize,
) -> Result<(), String> {
    if knot_count == 0 {
        return Err("curve has no knots".to_owned());
    }
    if dimension == 0 || control_count != knot_count.saturating_mul(dimension) {
        return Err("curve has incomplete controls".to_owned());
    }
    Ok(())
}

trait Quantized: Copy {
    const KNOT_MAX: u16;
    const CONTROL_MASK: u16;
    const CONTROL_MAX: f32;
    const SIGN_MASK: u16;
    const INDEX_SHIFT: u32;

    fn widen(self) -> u16;
}

impl Quantized for u8 {
    const KNOT_MAX: u16 = 255;
    const CONTROL_MASK: u16 = 0x7f;
    const CONTROL_MAX: f32 = 127.0;
    const SIGN_MASK: u16 = 0x80;
    const INDEX_SHIFT: u32 = 7;

    fn widen(self) -> u16 {
        u16::from(self)
    }
}

impl Quantized for u16 {
    const KNOT_MAX: u16 = u16::MAX;
    const CONTROL_MASK: u16 = 0x7fff;
    const CONTROL_MAX: f32 = 32_767.0;
    const SIGN_MASK: u16 = 0x8000;
    const INDEX_SHIFT: u32 = 15;

    fn widen(self) -> u16 {
        self
    }
}

fn truncated_knot_scale(value: u16) -> f32 {
    f32::from_bits(u32::from(value) << 16)
}

fn decode_arbitrary_quantized<T: Quantized>(
    knot_scale: u16,
    scale_offsets: &[f32],
    data: &[T],
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    if scale_offsets.len() != dimension.saturating_mul(2) {
        return Err("arbitrary-dimensional curve has invalid scale/offset data".to_owned());
    }
    let stride = dimension.saturating_add(1);
    if stride == 0 || !data.len().is_multiple_of(stride) {
        return Err("arbitrary-dimensional curve has incomplete data".to_owned());
    }
    let knot_count = data.len() / stride;
    let (scales, offsets) = scale_offsets.split_at(dimension);
    let mut controls = Vec::with_capacity(knot_count.saturating_mul(dimension));
    for control in data[knot_count..].chunks_exact(dimension) {
        controls.extend(control.iter().enumerate().map(|(component, value)| {
            offsets[component] + scales[component] * f32::from(value.widen())
        }));
    }
    DecodedCurve::from_quantized(
        data,
        knot_count,
        controls,
        dimension,
        truncated_knot_scale(knot_scale),
        local_clock,
    )
}

fn decode_quantized_quaternions<T: Quantized>(
    table_entries: u16,
    one_over_knot_scale: f32,
    data: &[T],
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    if !data.len().is_multiple_of(4) {
        return Err("quantized quaternion curve has incomplete data".to_owned());
    }
    let knot_count = data.len() / 4;
    let mut controls = Vec::with_capacity(knot_count.saturating_mul(4));
    for index in 0..knot_count {
        controls.extend(decode_quantized_quaternion_at(
            table_entries,
            data,
            knot_count,
            index,
        )?);
    }
    DecodedCurve::from_quantized(
        data,
        knot_count,
        controls,
        4,
        one_over_knot_scale,
        local_clock,
    )
}

fn decode_quantized_quaternion_at<T: Quantized>(
    table_entries: u16,
    data: &[T],
    knot_count: usize,
    index: usize,
) -> Result<[f32; 4], String> {
    let control_offset = knot_count + index.saturating_mul(3);
    let packed = [
        data.get(control_offset).copied().map(T::widen),
        data.get(control_offset + 1).copied().map(T::widen),
        data.get(control_offset + 2).copied().map(T::widen),
    ];
    let [Some(first), Some(second), Some(third)] = packed else {
        return Err("quantized quaternion has an incomplete control".to_owned());
    };
    let packed = [first, second, third];
    let missing_is_negative = packed[0] & T::SIGN_MASK != 0;
    let missing_index =
        usize::from(((packed[1] >> (T::INDEX_SHIFT - 1)) & 0x2) | (packed[2] >> T::INDEX_SHIFT));
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
    Ok(result)
}

fn decode_quantized_vec3<T: Quantized>(
    knot_scale: u16,
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[T],
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    require_dimension(dimension, 3, "D3 quantized")?;
    if !data.len().is_multiple_of(4) {
        return Err("quantized D3 curve has incomplete data".to_owned());
    }
    let knot_count = data.len() / 4;
    let mut controls = Vec::with_capacity(knot_count.saturating_mul(3));
    for control in data[knot_count..].as_chunks::<3>().0 {
        controls.extend(core::array::from_fn::<_, 3, _>(|component| {
            offsets[component] + scales[component] * f32::from(control[component].widen())
        }));
    }
    DecodedCurve::from_quantized(
        data,
        knot_count,
        controls,
        3,
        truncated_knot_scale(knot_scale),
        local_clock,
    )
}

fn decode_uniform_scale<T: Quantized>(
    knot_scale: u16,
    scale: f32,
    offset: f32,
    data: &[T],
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    require_dimension(dimension, 9, "D9 uniform scale")?;
    if !data.len().is_multiple_of(2) {
        return Err("D9 uniform scale curve has incomplete data".to_owned());
    }
    let knot_count = data.len() / 2;
    let mut controls = Vec::with_capacity(knot_count.saturating_mul(9));
    for value in &data[knot_count..] {
        let value = offset + scale * f32::from(value.widen());
        controls.extend([value, 0.0, 0.0, 0.0, value, 0.0, 0.0, 0.0, value]);
    }
    DecodedCurve::from_quantized(
        data,
        knot_count,
        controls,
        9,
        truncated_knot_scale(knot_scale),
        local_clock,
    )
}

fn decode_diagonal_scale<T: Quantized>(
    knot_scale: u16,
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[T],
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    require_dimension(dimension, 9, "D9 diagonal scale")?;
    if !data.len().is_multiple_of(4) {
        return Err("D9 diagonal scale curve has incomplete data".to_owned());
    }
    let knot_count = data.len() / 4;
    let mut controls = Vec::with_capacity(knot_count.saturating_mul(9));
    for control in data[knot_count..].as_chunks::<3>().0 {
        let diagonal = core::array::from_fn::<_, 3, _>(|component| {
            offsets[component] + scales[component] * f32::from(control[component].widen())
        });
        controls.extend([
            diagonal[0],
            0.0,
            0.0,
            0.0,
            diagonal[1],
            0.0,
            0.0,
            0.0,
            diagonal[2],
        ]);
    }
    DecodedCurve::from_quantized(
        data,
        knot_count,
        controls,
        9,
        truncated_knot_scale(knot_scale),
        local_clock,
    )
}

fn decode_identity_float_vec3(
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[f32],
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    require_dimension(dimension, 3, "D3 identity")?;
    if !data.len().is_multiple_of(2) {
        return Err("identity D3 curve has incomplete data".to_owned());
    }
    let knot_count = data.len() / 2;
    let controls = data[knot_count..]
        .iter()
        .flat_map(|value| {
            core::array::from_fn::<_, 3, _>(|component| {
                offsets[component] + scales[component] * value
            })
        })
        .collect();
    DecodedCurve::from_float(&data[..knot_count], controls, 3, local_clock)
}

fn decode_identity_quantized_vec3<T: Quantized>(
    knot_scale: u16,
    scales: &[f32; 3],
    offsets: &[f32; 3],
    data: &[T],
    dimension: usize,
    local_clock: f32,
) -> Result<DecodedCurve, String> {
    require_dimension(dimension, 3, "D3 identity-quantized")?;
    if !data.len().is_multiple_of(2) {
        return Err("identity-quantized D3 curve has incomplete data".to_owned());
    }
    let knot_count = data.len() / 2;
    let controls = data[knot_count..]
        .iter()
        .flat_map(|value| {
            core::array::from_fn::<_, 3, _>(|component| {
                offsets[component] + scales[component] * f32::from(value.widen())
            })
        })
        .collect();
    DecodedCurve::from_quantized(
        data,
        knot_count,
        controls,
        3,
        truncated_knot_scale(knot_scale),
        local_clock,
    )
}

fn sample_decoded(
    curve: &CurveData,
    decoded: &DecodedCurve,
    dimension: usize,
    local_clock: f32,
    normalized: bool,
) -> Result<Vec<f32>, String> {
    let degree = usize::from(curve.degree);
    let coefficients =
        bspline_coefficients(&decoded.knots, decoded.knot_index, degree, local_clock)?;
    let is_boundary_window = decoded.knot_index < degree
        || decoded.knot_index.saturating_add(degree) > decoded.knots.len();
    let mut window = Vec::with_capacity(coefficients.len().saturating_mul(dimension));
    for offset in 0..coefficients.len() {
        let index = if offset < degree {
            decoded.knot_index.saturating_sub(degree - offset)
        } else {
            decoded.knot_index.saturating_add(offset - degree)
        }
        .min(decoded.knots.len() - 1);
        let start = index.saturating_mul(dimension);
        window.extend_from_slice(&decoded.controls[start..start + dimension]);
    }
    if normalized && is_boundary_window {
        ensure_quaternion_continuity(&mut window, dimension)?;
    }
    let mut result = vec![0.0; dimension];
    for (control, coefficient) in window.chunks_exact(dimension).zip(coefficients) {
        for (output, value) in result.iter_mut().zip(control) {
            *output += coefficient * value;
        }
    }
    if normalized && degree != 0 {
        normalize(&mut result)?;
    }
    Ok(result)
}

fn bspline_coefficients(
    knots: &[f32],
    knot_index: usize,
    degree: usize,
    time: f32,
) -> Result<Vec<f32>, String> {
    let knot = |offset: isize| {
        let index = knot_index
            .saturating_add_signed(offset)
            .min(knots.len().saturating_sub(1));
        knots[index]
    };
    match degree {
        0 => Ok(vec![1.0]),
        1 => {
            let blend = ratio(time - knot(-1), knot(0) - knot(-1))?;
            Ok(vec![1.0 - blend, blend])
        }
        2 => quadratic_coefficients(&knot, time),
        3 => cubic_coefficients(&knot, time),
        _ => Err(format!("unsupported Granny spline degree {degree}")),
    }
}

fn quadratic_coefficients(knot: &impl Fn(isize) -> f32, time: f32) -> Result<Vec<f32>, String> {
    let blend0 = ratio(time - knot(-1), knot(0) - knot(-1))?;
    let blend11 = ratio(time - knot(-2), knot(0) - knot(-2))?;
    let blend12 = ratio(time - knot(-1), knot(1) - knot(-1))?;
    let mut c2 = blend11 + blend0 - blend0 * blend11;
    let c0 = blend0 * blend12;
    let c1 = c2 - c0;
    c2 = 1.0 - c2;
    Ok(vec![c2, c1, c0])
}

fn cubic_coefficients(knot: &impl Fn(isize) -> f32, time: f32) -> Result<Vec<f32>, String> {
    let blend0 = ratio(time - knot(-1), knot(0) - knot(-1))?;
    let blend11 = ratio(time - knot(-2), knot(0) - knot(-2))?;
    let blend12 = ratio(time - knot(-1), knot(1) - knot(-1))?;
    let blend21 = ratio(time - knot(-3), knot(0) - knot(-3))?;
    let blend22 = ratio(time - knot(-2), knot(1) - knot(-2))?;
    let blend23 = ratio(time - knot(-1), knot(2) - knot(-1))?;
    let inv0 = 1.0 - blend0;
    let inv11 = 1.0 - blend11;
    let inv12 = 1.0 - blend12;
    let inv21 = 1.0 - blend21;
    let inv22 = 1.0 - blend22;
    let inv23 = 1.0 - blend23;
    let inv0_inv11 = inv0 * inv11;
    let inv0_blend11 = inv0 * blend11;
    let blend0_inv12 = blend0 * inv12;
    let blend0_blend12 = blend0 * blend12;
    Ok(vec![
        inv0_inv11 * inv21,
        inv0_inv11 * blend21 + inv0_blend11 * inv22 + blend0_inv12 * inv22,
        inv0_blend11 * blend22 + blend0_inv12 * blend22 + blend0_blend12 * inv23,
        blend0_blend12 * blend23,
    ])
}

fn ratio(numerator: f32, denominator: f32) -> Result<f32, String> {
    if !denominator.is_finite() || denominator <= 0.0 {
        Err("spline has non-increasing knots".to_owned())
    } else {
        Ok(numerator / denominator)
    }
}

fn ensure_quaternion_continuity(values: &mut [f32], dimension: usize) -> Result<(), String> {
    require_dimension(dimension, 4, "normalized curve")?;
    for index in 1..values.len() / dimension {
        let split = index * dimension;
        let (before, current_and_after) = values.split_at_mut(split);
        let previous = &before[split - dimension..split];
        let current = &mut current_and_after[..dimension];
        let dot = previous
            .iter()
            .zip(current.iter())
            .map(|(a, b)| a * b)
            .sum::<f32>();
        if dot < 0.0 {
            for value in current {
                *value = -*value;
            }
        }
    }
    Ok(())
}

fn normalize(values: &mut [f32]) -> Result<(), String> {
    let length_squared = values.iter().map(|value| value * value).sum::<f32>();
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        return Err("normalized curve evaluated to a zero vector".to_owned());
    }
    let inverse_length = length_squared.sqrt().recip();
    for value in values {
        *value *= inverse_length;
    }
    Ok(())
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
            "{label} produces {actual} values instead of {expected}"
        ))
    }
}

#[cfg(test)]
mod tests;
