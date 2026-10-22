//! Phoenix `.lgt` animated local-light scenes.
//!
//! The [`lgt`] crate decodes the scene records. This module resolves `.lgt`
//! assets and plays the light tracks of a decoded [`Scene`]: it interpolates
//! the keys, keeps the retail retained colors, converts 3ds Max coordinates,
//! and produces renderer [`LocalLight`] values.

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec4};
use half::f16;
use lgt::{Frame, Light, LightFlags, LightKind, Scene};
use pipeline::source::{AssetSource, StdFileProvider};

use crate::lighting::{LocalLight, LocalLightShape};

const MAX_TO_PHOENIX_SCALE: f32 = 1.0 / 64.0;
const MIN_ENABLED_VALUE: f32 = 0.0125;
const MAX_RADIUS: f32 = 96.0;
const RETAIL_SPECULAR_SCALE: f32 = f32::from_bits(0x4048_f5c3);

/// One evaluated light together with its authored routing switches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightEffectSample {
    /// Renderer-facing local-light payload.
    pub light: LocalLight,
    /// Authored light-local right axis used to orient spot shadow projections.
    pub spot_right: [f32; 3],
    /// Authored shadow, diffuse, specular, fog, and buffering switches.
    pub flags: LightFlags,
}

/// Resolves and decodes an authored light scene, adding `.lgt` when absent.
///
/// # Errors
///
/// Returns [`LightEffectError::Missing`] when the asset cannot be resolved,
/// or [`LightEffectError::Decode`] when its stream is invalid.
pub fn load(
    source: &mut AssetSource<StdFileProvider>,
    path: &str,
) -> Result<Scene, LightEffectError> {
    let bytes = source
        .resolve_with_fallback(path, &[".lgt"])
        .ok_or_else(|| LightEffectError::Missing(path.to_owned()))?;
    Ok(Scene::from_bytes(&bytes)?)
}

/// Stateful retail-style player for one decoded light scene.
#[derive(Clone, Debug)]
pub struct LightEffectRuntime {
    effect: Arc<Scene>,
    time: f32,
    retained_colors: Vec<[f32; 3]>,
}

impl LightEffectRuntime {
    /// Creates a player at scene time zero.
    #[must_use]
    pub fn new(effect: Arc<Scene>) -> Self {
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
    /// The `.lgt` stream is invalid.
    #[error(transparent)]
    Decode(#[from] lgt::Error),
}

fn interpolation_frames(effect: &Scene, time: f32) -> Option<(&Frame, &Frame, f32)> {
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
    key0: &Light,
    key1: &Light,
    fraction: f32,
    max_to_world: Mat4,
    intensity_scale: f32,
    retained_color: &mut [f32; 3],
) -> Option<SampledLight> {
    if !matches!(key0.kind, LightKind::Spot | LightKind::Omni) {
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
        LightKind::Omni => LocalLightShape::Omni,
        LightKind::Spot => {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::{Mat4, Vec3};
    use lgt::{Frame, Light, LightFlags, LightKind, Scene};

    use super::{LightEffectRuntime, RETAIL_SPECULAR_SCALE};

    #[test]
    fn runtime_applies_max_axes_scale_and_retail_intensity() {
        let scene = Arc::new(Scene {
            version: 0xCCDD_0109,
            frames: vec![Frame {
                time: 0.0,
                lights: vec![Light {
                    name: "test".to_owned(),
                    mask: String::new(),
                    kind: LightKind::Omni,
                    position: [64.0, 128.0, 192.0],
                    orientation: [0.0, 0.0, 0.0, 1.0],
                    radius: 256.0,
                    far_attenuation_start: 0.25,
                    decay_distance: 64.0,
                    decay_kind: 0,
                    intensity: 2.0,
                    color: [0.5, 0.25, 0.125],
                    spot_inner: 0.1,
                    spot_outer: 0.2,
                    flags: LightFlags::from_bits(
                        LightFlags::SHADOWS
                            | LightFlags::DIFFUSE
                            | LightFlags::SPECULAR
                            | LightFlags::LIGHT_BUFFERED,
                    ),
                    fog_density: 1.0,
                }],
                objects: Vec::new(),
                cameras: Vec::new(),
            }],
        });
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
}
