//! Phoenix `.lgt` animated local-light scenes.
//!
//! The format is the little-endian `LightData::BScene` stream written by the
//! original light exporter. A scene is framed by matching version words and
//! contains timed frames of lights, objects, and cameras. Rendering only uses
//! the light tracks, but the other records still have to be consumed exactly
//! so malformed files cannot be mistaken for valid scenes.

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec4};
use half::f16;
use pipeline::source::{AssetSource, StdFileProvider};

use crate::lighting::{LocalLight, LocalLightShape};

const VERSION_FAMILY: u32 = 0x00CC_DD01;
const MAX_RECORDS: usize = 1 << 20;
const MAX_TO_PHOENIX_SCALE: f32 = 1.0 / 64.0;
const MIN_ENABLED_VALUE: f32 = 0.0125;
const MAX_RADIUS: f32 = 96.0;
const RETAIL_SPECULAR_SCALE: f32 = f32::from_bits(0x4048_f5c3);

/// One authored light kind from a Phoenix light scene.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightEffectKind {
    /// Uninitialized exporter value.
    Invalid,
    /// Cone-shaped local light.
    Spot,
    /// Omnidirectional local light.
    Omni,
    /// Directional light. The retail light-effect runtime intentionally skips it.
    Directional,
    /// Value from a newer or malformed file.
    Unknown(u8),
}

impl From<u8> for LightEffectKind {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Invalid,
            1 => Self::Spot,
            2 => Self::Omni,
            3 => Self::Directional,
            value => Self::Unknown(value),
        }
    }
}

/// Authored contribution switches for one light key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LightEffectFlags(u8);

impl LightEffectFlags {
    const SHADOWS: u8 = 1 << 0;
    const DIFFUSE: u8 = 1 << 1;
    const SPECULAR: u8 = 1 << 2;
    const FOG: u8 = 1 << 3;
    const FOG_SHADOWS: u8 = 1 << 4;
    const LIGHT_BUFFERED: u8 = 1 << 5;

    fn from_switches(switches: [bool; 6]) -> Self {
        let masks = [
            Self::SHADOWS,
            Self::DIFFUSE,
            Self::SPECULAR,
            Self::FOG,
            Self::FOG_SHADOWS,
            Self::LIGHT_BUFFERED,
        ];
        Self(
            switches
                .into_iter()
                .zip(masks)
                .fold(0, |bits, (enabled, mask)| {
                    bits | if enabled { mask } else { 0 }
                }),
        )
    }

    /// Returns whether the source requested a local shadow.
    #[must_use]
    pub const fn shadows(self) -> bool {
        self.0 & Self::SHADOWS != 0
    }

    /// Returns whether the light contributes diffuse color.
    #[must_use]
    pub const fn diffuse(self) -> bool {
        self.0 & Self::DIFFUSE != 0
    }

    /// Returns whether the source requests the retail specular multiplier.
    #[must_use]
    pub const fn specular(self) -> bool {
        self.0 & Self::SPECULAR != 0
    }

    /// Returns whether the source contributes to fog lighting.
    #[must_use]
    pub const fn fog(self) -> bool {
        self.0 & Self::FOG != 0
    }

    /// Returns whether fog receives the source's shadow.
    #[must_use]
    pub const fn fog_shadows(self) -> bool {
        self.0 & Self::FOG_SHADOWS != 0
    }

    /// Returns whether retail routes this source into the 3D light buffer.
    #[must_use]
    pub const fn light_buffered(self) -> bool {
        self.0 & Self::LIGHT_BUFFERED != 0
    }
}

/// One evaluated light together with its authored routing switches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightEffectSample {
    /// Renderer-facing local-light payload.
    pub light: LocalLight,
    /// Authored light-local right axis used to orient spot shadow projections.
    pub spot_right: [f32; 3],
    /// Authored shadow, diffuse, specular, fog, and buffering switches.
    pub flags: LightEffectFlags,
}

/// One keyframed local light.
#[derive(Clone, Debug, PartialEq)]
pub struct LightEffectLight {
    /// Exporter node name.
    pub name: String,
    /// Optional exporter mask name.
    pub mask: String,
    /// Light kind.
    pub kind: LightEffectKind,
    /// Position in 3ds Max units.
    pub position: [f32; 3],
    /// Orientation quaternion in 3ds Max axes.
    pub orientation: [f32; 4],
    /// Radius in 3ds Max units.
    pub radius: f32,
    /// Fraction of the radius at which radial falloff starts.
    pub far_attenuation_start: f32,
    /// Inverse-distance decay distance in 3ds Max units.
    pub decay_distance: f32,
    /// Exporter decay mode. The shipped renderer stores but does not branch on it.
    pub decay_kind: u8,
    /// Scalar light intensity.
    pub intensity: f32,
    /// Linear authored color.
    pub color: [f32; 3],
    /// Spot inner full angle in radians.
    pub spot_inner: f32,
    /// Spot outer full angle in radians.
    pub spot_outer: f32,
    /// Authored diffuse, specular, fog, shadow, and light-buffer switches.
    pub flags: LightEffectFlags,
    /// Authored fog density.
    pub fog_density: f32,
}

/// One timed frame in an animated light scene.
#[derive(Clone, Debug, PartialEq)]
pub struct LightEffectFrame {
    /// Exporter time in seconds.
    pub time: f32,
    /// Light keys in stable exporter order.
    pub lights: Vec<LightEffectLight>,
}

/// Decoded `.lgt` scene.
#[derive(Clone, Debug, PartialEq)]
pub struct LightEffect {
    /// Serialization version, including the `0xCCDD01` family prefix.
    pub version: u32,
    /// Timed scene frames.
    pub frames: Vec<LightEffectFrame>,
}

impl LightEffect {
    /// Decodes one complete little-endian `LightData::BScene` stream.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported version, truncated record, invalid
    /// vector count, or mismatched closing version marker.
    pub fn read(bytes: &[u8]) -> Result<Self, LightEffectError> {
        let mut reader = Reader::new(bytes);
        let version = reader.u32()?;
        if version >> 8 != VERSION_FAMILY {
            return Err(LightEffectError::UnsupportedVersion(version));
        }
        let frame_count = reader.count("frames")?;
        let mut frames = Vec::with_capacity(frame_count);
        for _ in 0..frame_count {
            frames.push(read_frame(&mut reader, version)?);
        }
        let closing_version = reader.u32()?;
        if closing_version != version {
            return Err(LightEffectError::VersionFooter {
                opening: version,
                closing: closing_version,
            });
        }
        Ok(Self { version, frames })
    }

    /// Resolves and decodes an authored light effect, adding `.lgt` when absent.
    ///
    /// # Errors
    ///
    /// Returns [`LightEffectError::Missing`] when the asset cannot be resolved,
    /// or a decode error when its stream is invalid.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        path: &str,
    ) -> Result<Self, LightEffectError> {
        let bytes = source
            .resolve_with_fallback(path, &[".lgt"])
            .ok_or_else(|| LightEffectError::Missing(path.to_owned()))?;
        Self::read(&bytes)
    }

    /// Returns the looping duration used by the retail runtime.
    #[must_use]
    pub fn duration(&self) -> f32 {
        let Some((first, last)) = self.frames.first().zip(self.frames.last()) else {
            return 0.0;
        };
        (last.time - first.time).max(0.0)
    }
}

/// Stateful retail-style player for one decoded light scene.
#[derive(Clone, Debug)]
pub struct LightEffectRuntime {
    effect: Arc<LightEffect>,
    time: f32,
    retained_colors: Vec<[f32; 3]>,
}

impl LightEffectRuntime {
    /// Creates a player at scene time zero.
    #[must_use]
    pub fn new(effect: Arc<LightEffect>) -> Self {
        Self {
            effect,
            time: 0.0,
            retained_colors: Vec::new(),
        }
    }

    /// Returns the current looping scene time.
    #[must_use]
    pub const fn time(&self) -> f32 {
        self.time
    }

    /// Advances and evaluates the scene under a live attachment transform.
    ///
    /// Directional/invalid records and retail-disabled tiny lights are omitted.
    /// Non-finite outputs are also omitted so corrupt presentation data cannot
    /// poison a GPU frame.
    #[must_use]
    pub fn advance_and_sample(
        &mut self,
        delta_seconds: f32,
        attachment_to_world: Mat4,
        intensity_scale: f32,
    ) -> Vec<LocalLight> {
        self.advance_and_sample_with_flags(delta_seconds, attachment_to_world, intensity_scale)
            .into_iter()
            .map(|sample| sample.light)
            .collect()
    }

    /// Advances and evaluates the scene while retaining authored routing flags.
    ///
    /// The retail renderer uses these flags to send non-shadowed buffered lights
    /// to its 3D light field while leaving shadowed and explicitly unbuffered
    /// lights in the direct per-model payload.
    #[must_use]
    pub fn advance_and_sample_with_flags(
        &mut self,
        delta_seconds: f32,
        attachment_to_world: Mat4,
        intensity_scale: f32,
    ) -> Vec<LightEffectSample> {
        self.advance(delta_seconds);
        let Some((frame0, frame1, fraction)) = interpolation_frames(&self.effect, self.time) else {
            return Vec::new();
        };
        self.retained_colors.resize(frame0.lights.len(), [1.0; 3]);
        let max_to_world = attachment_to_world * max_to_phoenix();
        let mut lights = Vec::with_capacity(frame0.lights.len());
        for (index, key0) in frame0.lights.iter().enumerate() {
            let key1 = frame1.lights.get(index).unwrap_or(key0);
            if let Some(light) = sample_light(
                key0,
                key1,
                fraction,
                max_to_world,
                intensity_scale,
                &mut self.retained_colors[index],
            ) {
                lights.push(LightEffectSample {
                    light: light.light,
                    spot_right: light.spot_right,
                    flags: key0.flags,
                });
            }
        }
        lights
    }

    fn advance(&mut self, delta_seconds: f32) {
        let duration = self.effect.duration();
        if duration > 0.0 {
            self.time = (self.time + delta_seconds.max(0.0)).rem_euclid(duration);
        } else {
            self.time = 0.0;
        }
    }
}

/// Light-scene loading failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LightEffectError {
    /// Asset resolution failed.
    #[error("light effect not found: {0}")]
    Missing(String),
    /// Version prefix is not a supported Phoenix light-scene family.
    #[error("unsupported light-effect version 0x{0:08X}")]
    UnsupportedVersion(u32),
    /// A serialized vector count was negative or implausibly large.
    #[error("invalid light-effect {field} count {value}")]
    InvalidCount {
        /// Record group being decoded.
        field: &'static str,
        /// Serialized signed count.
        value: i32,
    },
    /// The stream ended before the requested field was complete.
    #[error("truncated light effect at byte {offset} while reading {field}")]
    Truncated {
        /// Byte offset at which decoding failed.
        offset: usize,
        /// Field type being read.
        field: &'static str,
    },
    /// Opening and closing version markers differ.
    #[error(
        "light-effect version footer mismatch: opening 0x{opening:08X}, closing 0x{closing:08X}"
    )]
    VersionFooter {
        /// Opening marker.
        opening: u32,
        /// Closing marker.
        closing: u32,
    },
}

fn read_frame(reader: &mut Reader<'_>, version: u32) -> Result<LightEffectFrame, LightEffectError> {
    let time = reader.f32()?;
    let count = reader.count("lights")?;
    let mut lights = Vec::with_capacity(count);
    for _ in 0..count {
        lights.push(read_light(reader, version)?);
    }
    let object_count = reader.count("objects")?;
    for _ in 0..object_count {
        skip_object(reader)?;
    }
    let camera_count = reader.count("cameras")?;
    for _ in 0..camera_count {
        skip_camera(reader, version)?;
    }
    Ok(LightEffectFrame { time, lights })
}

fn read_light(reader: &mut Reader<'_>, version: u32) -> Result<LightEffectLight, LightEffectError> {
    let (name, position, orientation) = read_base(reader)?;
    let mask = reader.string()?;
    let kind = reader.u8()?.into();
    let radius = reader.f32()?;
    let far_attenuation_start = reader.f32()?;
    let decay_distance = reader.f32()?;
    let decay_kind = reader.u8()?;
    let intensity = reader.f32()?;
    let color = reader.vec3()?;
    let spot_inner = reader.f32()?;
    let spot_outer = reader.f32()?;
    let shadows = reader.bool()?;
    let diffuse = reader.bool()?;
    let specular = reader.bool()?;
    let fog = reader.bool()?;
    let fog_shadows = reader.bool()?;
    let fog_density = reader.f32()?;
    let light_buffered = if version >= 0xCCDD_0109 {
        reader.bool()?
    } else {
        true
    };
    Ok(LightEffectLight {
        name,
        mask,
        kind,
        position,
        orientation,
        radius,
        far_attenuation_start,
        decay_distance,
        decay_kind,
        intensity,
        color,
        spot_inner,
        spot_outer,
        flags: LightEffectFlags::from_switches([
            shadows,
            diffuse,
            specular,
            fog,
            fog_shadows,
            light_buffered,
        ]),
        fog_density,
    })
}

fn read_base(reader: &mut Reader<'_>) -> Result<(String, [f32; 3], [f32; 4]), LightEffectError> {
    Ok((reader.string()?, reader.vec3()?, reader.vec4()?))
}

fn skip_object(reader: &mut Reader<'_>) -> Result<(), LightEffectError> {
    let _ = read_base(reader)?;
    let _kind = reader.u8()?;
    let _visibility = reader.f32()?;
    let _sphere_radius = reader.f32()?;
    let byte_count = reader.count("object UDP bytes")?;
    let _udp = reader.bytes(byte_count, "object UDP bytes")?;
    Ok(())
}

fn skip_camera(reader: &mut Reader<'_>, version: u32) -> Result<(), LightEffectError> {
    let _ = read_base(reader)?;
    let _fov = reader.f32()?;
    if version & 0xFF >= 8 {
        let _focal_depth = reader.f32()?;
        let _near_range = reader.f32()?;
        let _far_range = reader.f32()?;
    }
    if version & 0xFF >= 10 {
        let _near_clip = reader.f32()?;
        let _far_clip = reader.f32()?;
    }
    Ok(())
}

fn interpolation_frames(
    effect: &LightEffect,
    time: f32,
) -> Option<(&LightEffectFrame, &LightEffectFrame, f32)> {
    let first_time = effect.frames.first()?.time;
    let duration = effect.duration();
    if time >= duration {
        let last = effect.frames.last()?;
        return Some((last, last, 0.0));
    }
    let upper = effect
        .frames
        .iter()
        .position(|frame| time < frame.time - first_time)
        .unwrap_or(effect.frames.len().saturating_sub(1));
    let lower = upper.saturating_sub(1);
    let frame0 = &effect.frames[lower];
    let frame1 = &effect.frames[upper];
    let start = frame0.time - first_time;
    let span = frame1.time - frame0.time;
    let fraction = if span == 0.0 {
        0.0
    } else {
        ((time - start) / span).clamp(0.0, 1.0)
    };
    Some((frame0, frame1, fraction))
}

fn sample_light(
    key0: &LightEffectLight,
    key1: &LightEffectLight,
    fraction: f32,
    max_to_world: Mat4,
    intensity_scale: f32,
    retained_color: &mut [f32; 3],
) -> Option<SampledLight> {
    if !matches!(key0.kind, LightEffectKind::Spot | LightEffectKind::Omni) {
        return None;
    }
    let intensity = lerp(key0.intensity, key1.intensity, fraction) * intensity_scale;
    let radius = (lerp(key0.radius, key1.radius, fraction) * MAX_TO_PHOENIX_SCALE)
        .clamp(0.00125, MAX_RADIUS);
    if radius <= MIN_ENABLED_VALUE || intensity <= MIN_ENABLED_VALUE {
        return None;
    }
    if key0.flags.diffuse() {
        let color =
            Vec3::from_array(key0.color).lerp(Vec3::from_array(key1.color), fraction) * intensity;
        *retained_color = color.to_array().map(round_half);
    }
    let local_position =
        Vec3::from_array(key0.position).lerp(Vec3::from_array(key1.position), fraction);
    let orientation = safe_slerp(key0.orientation, key1.orientation, fraction)?;
    let light_to_world =
        max_to_world * Mat4::from_rotation_translation(orientation, local_position);
    let position = light_to_world.transform_point3(Vec3::ZERO);
    let direction = light_to_world
        .transform_vector3(-Vec3::Z)
        .normalize_or_zero();
    let spot_right = light_to_world
        .transform_vector3(Vec3::X)
        .normalize_or_zero();
    if !position.is_finite()
        || direction == Vec3::ZERO
        || !direction.is_finite()
        || spot_right == Vec3::ZERO
        || !spot_right.is_finite()
    {
        return None;
    }
    let far_attenuation_start = round_half(lerp(
        key0.far_attenuation_start,
        key1.far_attenuation_start,
        fraction,
    ))
    .clamp(0.0, 0.999);
    let decay_distance =
        round_half(lerp(key0.decay_distance, key1.decay_distance, fraction) * MAX_TO_PHOENIX_SCALE)
            .max(0.01);
    let shape = match key0.kind {
        LightEffectKind::Omni => LocalLightShape::Omni,
        LightEffectKind::Spot => {
            let (inner, outer) = enforced_spot_angles(
                lerp(key0.spot_inner, key1.spot_inner, fraction),
                lerp(key0.spot_outer, key1.spot_outer, fraction),
            );
            LocalLightShape::Spot {
                direction: direction.to_array().map(round_half),
                inner_cos: (inner * 0.5).cos(),
                outer_cos: (outer * 0.5).cos(),
            }
        }
        _ => return None,
    };
    let light = LocalLight {
        position: position.to_array(),
        color: *retained_color,
        radius,
        far_attenuation_start,
        decay_distance,
        shape,
        specular_intensity: if key0.flags.specular() {
            RETAIL_SPECULAR_SCALE
        } else {
            0.0
        },
        shadow: None,
    };
    light_is_finite(light).then_some(SampledLight {
        light,
        spot_right: spot_right.to_array().map(round_half),
    })
}

struct SampledLight {
    light: LocalLight,
    spot_right: [f32; 3],
}

fn safe_slerp(a: [f32; 4], b: [f32; 4], fraction: f32) -> Option<Quat> {
    let a = Quat::from_array(a);
    let b = Quat::from_array(b);
    if !a.is_finite() || !b.is_finite() || a.length_squared() <= f32::EPSILON {
        return None;
    }
    let b = if b.length_squared() <= f32::EPSILON {
        a
    } else {
        b.normalize()
    };
    let value = a.normalize().slerp(b, fraction);
    value.is_finite().then_some(value)
}

fn enforced_spot_angles(inner: f32, outer: f32) -> (f32, f32) {
    let mut inner = round_half(inner);
    let mut outer = round_half(outer);
    if outer < 1.0_f32.to_radians() {
        outer = round_half(1.0_f32.to_radians());
    }
    if inner < 0.5_f32.to_radians() {
        inner = round_half(0.5_f32.to_radians());
    }
    inner = inner.clamp(0.0, 160.0_f32.to_radians());
    outer = outer.clamp(0.0, 160.0_f32.to_radians());
    outer = outer.max(0.025_f32.to_radians());
    inner = inner.min(outer - 0.0125_f32.to_radians());
    (round_half(inner), round_half(outer))
}

fn light_is_finite(light: LocalLight) -> bool {
    light.position.into_iter().all(f32::is_finite)
        && light.color.into_iter().all(f32::is_finite)
        && light.radius.is_finite()
        && light.far_attenuation_start.is_finite()
        && light.decay_distance.is_finite()
}

fn max_to_phoenix() -> Mat4 {
    Mat4::from_cols(
        Vec4::new(-MAX_TO_PHOENIX_SCALE, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -MAX_TO_PHOENIX_SCALE, 0.0),
        Vec4::new(0.0, MAX_TO_PHOENIX_SCALE, 0.0, 0.0),
        Vec4::W,
    )
}

fn lerp(a: f32, b: f32, fraction: f32) -> f32 {
    a + (b - a) * fraction
}

fn round_half(value: f32) -> f32 {
    f16::from_f32(value).to_f32()
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn bytes(&mut self, count: usize, field: &'static str) -> Result<&'a [u8], LightEffectError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(LightEffectError::Truncated {
                offset: self.offset,
                field,
            })?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(LightEffectError::Truncated {
                offset: self.offset,
                field,
            })?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, LightEffectError> {
        Ok(self.bytes(1, "u8")?[0])
    }

    fn bool(&mut self) -> Result<bool, LightEffectError> {
        Ok(self.u8()? != 0)
    }

    fn u32(&mut self) -> Result<u32, LightEffectError> {
        let bytes: [u8; 4] = self
            .bytes(4, "u32")?
            .try_into()
            .expect("a four-byte slice always converts to an array");
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32, LightEffectError> {
        let bytes: [u8; 4] = self
            .bytes(4, "i32")?
            .try_into()
            .expect("a four-byte slice always converts to an array");
        Ok(i32::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> Result<f32, LightEffectError> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn vec3(&mut self) -> Result<[f32; 3], LightEffectError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    fn vec4(&mut self) -> Result<[f32; 4], LightEffectError> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }

    fn count(&mut self, field: &'static str) -> Result<usize, LightEffectError> {
        let value = self.i32()?;
        let Ok(count) = usize::try_from(value) else {
            return Err(LightEffectError::InvalidCount { field, value });
        };
        if count > MAX_RECORDS || count > self.bytes.len() {
            return Err(LightEffectError::InvalidCount { field, value });
        }
        Ok(count)
    }

    fn string(&mut self) -> Result<String, LightEffectError> {
        let raw_length = self.u32()?;
        let Ok(length) = usize::try_from(raw_length) else {
            return Err(LightEffectError::InvalidCount {
                field: "string bytes",
                value: i32::MAX,
            });
        };
        if length > self.bytes.len() {
            return Err(LightEffectError::InvalidCount {
                field: "string bytes",
                value: i32::try_from(raw_length).unwrap_or(i32::MAX),
            });
        }
        Ok(String::from_utf8_lossy(self.bytes(length, "string bytes")?).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::{Mat4, Vec3};

    use super::{LightEffect, LightEffectError, LightEffectRuntime, RETAIL_SPECULAR_SCALE};

    #[test]
    fn decodes_version_109_light_and_footer() {
        let bytes = scene_bytes(0xCCDD_0109, true);
        let scene = LightEffect::read(&bytes).unwrap();
        assert_eq!(scene.frames.len(), 1);
        let light = &scene.frames[0].lights[0];
        assert_eq!(light.name, "test");
        assert!(light.flags.light_buffered());
        assert_eq!(
            light.position.map(f32::to_bits),
            [64.0, 128.0, 192.0].map(f32::to_bits)
        );
    }

    #[test]
    fn version_107_defaults_light_buffering_on() {
        let scene = LightEffect::read(&scene_bytes(0xCCDD_0107, false)).unwrap();
        assert!(scene.frames[0].lights[0].flags.light_buffered());
    }

    #[test]
    fn rejects_mismatched_footer() {
        let mut bytes = scene_bytes(0xCCDD_0109, true);
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(matches!(
            LightEffect::read(&bytes),
            Err(LightEffectError::VersionFooter { .. })
        ));
    }

    #[test]
    fn runtime_applies_max_axes_scale_and_retail_intensity() {
        let scene = Arc::new(LightEffect::read(&scene_bytes(0xCCDD_0109, true)).unwrap());
        let mut runtime = LightEffectRuntime::new(scene);
        let lights = runtime.advance_and_sample(0.0, Mat4::IDENTITY, 1.3);
        assert_eq!(lights.len(), 1);
        assert!(
            Vec3::from_array(lights[0].position).abs_diff_eq(Vec3::new(-1.0, 3.0, -2.0), 1.0e-6)
        );
        assert!((lights[0].radius - 4.0).abs() < 1.0e-6);
        assert_eq!(
            lights[0].specular_intensity.to_bits(),
            RETAIL_SPECULAR_SCALE.to_bits()
        );
    }

    fn scene_bytes(version: u32, include_light_buffered: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, version);
        push_i32(&mut bytes, 1);
        push_f32(&mut bytes, 0.0);
        push_i32(&mut bytes, 1);
        push_string(&mut bytes, "test");
        for value in [64.0, 128.0, 192.0, 0.0, 0.0, 0.0, 1.0] {
            push_f32(&mut bytes, value);
        }
        push_string(&mut bytes, "");
        bytes.push(2);
        for value in [256.0, 0.25, 64.0] {
            push_f32(&mut bytes, value);
        }
        bytes.push(0);
        push_f32(&mut bytes, 2.0);
        for value in [0.5, 0.25, 0.125, 0.1, 0.2] {
            push_f32(&mut bytes, value);
        }
        bytes.extend([1, 1, 1, 0, 0]);
        push_f32(&mut bytes, 1.0);
        if include_light_buffered {
            bytes.push(1);
        }
        push_i32(&mut bytes, 0);
        push_i32(&mut bytes, 0);
        push_u32(&mut bytes, version);
        bytes
    }

    fn push_string(bytes: &mut Vec<u8>, value: &str) {
        push_u32(bytes, u32::try_from(value.len()).unwrap());
        bytes.extend(value.as_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend(value.to_le_bytes());
    }

    fn push_i32(bytes: &mut Vec<u8>, value: i32) {
        bytes.extend(value.to_le_bytes());
    }

    fn push_f32(bytes: &mut Vec<u8>, value: f32) {
        push_u32(bytes, value.to_bits());
    }
}
