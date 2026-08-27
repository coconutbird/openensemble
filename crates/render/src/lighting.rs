//! Renderer-side local-light payloads.
//!
//! The PC shaders consume at most twenty lights. Each light occupies thirty
//! meaningful `f32` values in an eight-`vec4` (128-byte) storage-buffer slot;
//! the final two values are padding. This module owns that ABI so terrain,
//! foliage, roads, particles, and UGX models can share one uploaded light set.

use glam::Vec3;
use num_traits::ToPrimitive;
use wgpu::util::DeviceExt;

use crate::terrain::LightingParams;

/// Maximum number of local lights evaluated by the shipped PC shaders.
pub const MAX_LOCAL_LIGHTS: usize = 20;

/// A local light's angular attenuation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LocalLightShape {
    /// Emits equally in every direction.
    Omni,
    /// Emits along `direction`, fading between the inner and outer cone.
    Spot {
        /// Unit direction from the light toward illuminated geometry.
        direction: [f32; 3],
        /// Cosine of the fully illuminated inner half-angle.
        inner_cos: f32,
        /// Cosine of the outer half-angle where illumination reaches zero.
        outer_cos: f32,
    },
}

/// Optional local-shadow metadata stored in the oracle light record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalShadow {
    /// Shadow texture-array index. Values below `-1` select dual-paraboloid
    /// omni-shadow addressing, matching the original shader.
    pub texture_index: f32,
    /// World-to-shadow transform rows.
    pub transform_rows: [[f32; 4]; 3],
    /// Atlas bounds preset in the range `0..=4`.
    pub bounds_preset: u32,
    /// Blend toward fully lit, where zero is the authored shadow and one is
    /// fully faded.
    pub fade: f32,
}

/// Renderer-facing local-light description.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalLight {
    /// World-space position.
    pub position: [f32; 3],
    /// Linear RGB radiance.
    pub color: [f32; 3],
    /// Radius at which smooth distance attenuation reaches zero.
    pub radius: f32,
    /// Near-field inverse-distance decay scale.
    pub decay_distance: f32,
    /// Diffuse/specular angular shape.
    pub shape: LocalLightShape,
    /// Specular multiplier.
    pub specular_intensity: f32,
    /// Optional authored shadow projection.
    pub shadow: Option<LocalShadow>,
}

impl LocalLight {
    /// Creates an omnidirectional light without a local shadow.
    #[must_use]
    pub fn omni(position: [f32; 3], color: [f32; 3], radius: f32) -> Self {
        Self {
            position,
            color,
            radius,
            decay_distance: radius,
            shape: LocalLightShape::Omni,
            specular_intensity: 1.0,
            shadow: None,
        }
    }

    /// Creates a spot light without a local shadow.
    #[must_use]
    pub fn spot(
        position: [f32; 3],
        color: [f32; 3],
        radius: f32,
        direction: [f32; 3],
        inner_cos: f32,
        outer_cos: f32,
    ) -> Self {
        Self {
            position,
            color,
            radius,
            decay_distance: radius,
            shape: LocalLightShape::Spot {
                direction,
                inner_cos,
                outer_cos,
            },
            specular_intensity: 1.0,
            shadow: None,
        }
    }

    fn packed(self) -> PackedLocalLight {
        let safe_radius = self.radius.max(f32::EPSILON);
        let omni_mul = -safe_radius.recip();
        let omni_add = 1.0;
        let (spot_at, spot_mul, spot_add) = match self.shape {
            LocalLightShape::Omni => ([0.0, 1.0, 0.0], 0.0, 1.0),
            LocalLightShape::Spot {
                direction,
                inner_cos,
                outer_cos,
            } => {
                let direction = Vec3::from_array(direction).normalize_or_zero();
                let direction = if direction == Vec3::ZERO {
                    Vec3::Y
                } else {
                    direction
                };
                let inner_cos = inner_cos.clamp(-1.0, 1.0);
                let outer_cos = outer_cos.clamp(-1.0, inner_cos);
                let angular_range = (inner_cos - outer_cos).max(f32::EPSILON);
                (
                    direction.to_array(),
                    angular_range.recip(),
                    -outer_cos / angular_range,
                )
            }
        };
        let (shadow_index, shadow_rows, shadow_metadata) =
            self.shadow
                .map_or((-1.0, [[0.0; 4]; 3], [0.0; 4]), |shadow| {
                    (
                        shadow.texture_index,
                        shadow.transform_rows,
                        [
                            shadow.bounds_preset.min(4).to_f32().unwrap_or(4.0),
                            shadow.fade.clamp(0.0, 1.0),
                            0.0,
                            0.0,
                        ],
                    )
                });
        PackedLocalLight {
            position_and_omni: [
                self.position[0],
                self.position[1],
                self.position[2],
                omni_mul,
            ],
            color_and_omni: [self.color[0], self.color[1], self.color[2], omni_add],
            attenuation_and_shadow: [
                self.decay_distance.max(0.0),
                spot_mul,
                spot_add,
                shadow_index,
            ],
            spot_and_specular: [
                spot_at[0],
                spot_at[1],
                spot_at[2],
                self.specular_intensity.max(0.0),
            ],
            shadow_row0: shadow_rows[0],
            shadow_row1: shadow_rows[1],
            shadow_row2: shadow_rows[2],
            shadow_metadata,
        }
    }
}

/// A bounded local-light set and its shared shader settings.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalLightSet {
    lights: Vec<LocalLight>,
    /// Specular exponent used by the terrain/local-light oracle path.
    pub specular_power: f32,
    /// Whether local shadow-map sampling is enabled.
    pub shadows_enabled: bool,
}

impl Default for LocalLightSet {
    fn default() -> Self {
        Self {
            lights: Vec::new(),
            specular_power: 16.0,
            shadows_enabled: false,
        }
    }
}

impl LocalLightSet {
    /// Creates a checked light set.
    ///
    /// # Errors
    ///
    /// Returns [`LocalLightError::TooManyLights`] when more than the oracle's
    /// twenty-light limit is supplied.
    pub fn new(lights: Vec<LocalLight>) -> Result<Self, LocalLightError> {
        validate_count(lights.len())?;
        Ok(Self {
            lights,
            ..Self::default()
        })
    }

    /// Returns the active lights in upload order.
    #[must_use]
    pub fn lights(&self) -> &[LocalLight] {
        &self.lights
    }

    /// Appends one light without silently truncating the shader-visible set.
    ///
    /// # Errors
    ///
    /// Returns [`LocalLightError::TooManyLights`] at the twenty-light limit.
    pub fn push(&mut self, light: LocalLight) -> Result<(), LocalLightError> {
        validate_count(self.lights.len().saturating_add(1))?;
        self.lights.push(light);
        Ok(())
    }

    /// Removes all active lights.
    pub fn clear(&mut self) {
        self.lights.clear();
    }

    /// Applies count/specular/shadow switches to the shared lighting uniform.
    pub fn apply_to_lighting(&self, lighting: &mut LightingParams) {
        lighting.local_light_params = [
            self.lights.len().to_f32().unwrap_or(0.0),
            self.specular_power.max(f32::EPSILON),
            f32::from(self.shadows_enabled),
            f32::from(!self.lights.is_empty()),
        ];
    }
}

/// Persistent GPU storage for a [`LocalLightSet`].
#[derive(Clone)]
pub struct LocalLightBuffer {
    buffer: wgpu::Buffer,
}

impl LocalLightBuffer {
    /// Creates and initializes the fixed-size oracle light buffer.
    #[must_use]
    pub fn new(device: &wgpu::Device, lights: &LocalLightSet) -> Self {
        let packed = pack_lights(lights);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shared Local Lights"),
            contents: bytemuck::cast_slice(&packed),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        Self { buffer }
    }

    /// Creates the common zero-light buffer.
    #[must_use]
    pub fn empty(device: &wgpu::Device) -> Self {
        Self::new(device, &LocalLightSet::default())
    }

    /// Replaces the whole fixed-size payload.
    pub fn update(&self, queue: &wgpu::Queue, lights: &LocalLightSet) {
        let packed = pack_lights(lights);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&packed));
    }

    /// Returns the storage buffer shared by renderer bind groups.
    #[must_use]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

/// Local-light validation error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LocalLightError {
    /// The fixed shader loop cannot address the requested count.
    #[error("local-light count {actual} exceeds the shader limit of {MAX_LOCAL_LIGHTS}")]
    TooManyLights {
        /// Requested light count.
        actual: usize,
    },
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedLocalLight {
    position_and_omni: [f32; 4],
    color_and_omni: [f32; 4],
    attenuation_and_shadow: [f32; 4],
    spot_and_specular: [f32; 4],
    shadow_row0: [f32; 4],
    shadow_row1: [f32; 4],
    shadow_row2: [f32; 4],
    shadow_metadata: [f32; 4],
}

fn validate_count(count: usize) -> Result<(), LocalLightError> {
    if count > MAX_LOCAL_LIGHTS {
        Err(LocalLightError::TooManyLights { actual: count })
    } else {
        Ok(())
    }
}

fn pack_lights(lights: &LocalLightSet) -> [PackedLocalLight; MAX_LOCAL_LIGHTS] {
    let mut packed = [PackedLocalLight::default(); MAX_LOCAL_LIGHTS];
    for (destination, light) in packed.iter_mut().zip(lights.lights()) {
        *destination = light.packed();
    }
    packed
}

#[cfg(test)]
mod tests {
    use std::mem;

    use super::{
        LocalLight, LocalLightError, LocalLightSet, MAX_LOCAL_LIGHTS, PackedLocalLight, pack_lights,
    };
    use crate::terrain::LightingParams;

    #[test]
    fn packed_light_matches_oracle_stride() {
        assert_eq!(mem::size_of::<PackedLocalLight>(), 8 * 16);
    }

    #[test]
    fn omni_light_fades_at_its_radius() {
        let packed = LocalLight::omni([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], 8.0).packed();
        assert_eq!(
            packed.position_and_omni.map(f32::to_bits),
            [1.0, 2.0, 3.0, -0.125].map(f32::to_bits)
        );
        assert_eq!(
            packed.color_and_omni.map(f32::to_bits),
            [4.0, 5.0, 6.0, 1.0].map(f32::to_bits)
        );
        assert_eq!(
            packed.attenuation_and_shadow[3].to_bits(),
            (-1.0_f32).to_bits()
        );
        assert_eq!(
            packed.spot_and_specular[0..3]
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            [0.0, 1.0, 0.0].map(f32::to_bits)
        );
    }

    #[test]
    fn spot_cone_maps_outer_to_zero_and_inner_to_one() {
        let packed = LocalLight::spot([0.0; 3], [1.0; 3], 10.0, [0.0, 0.0, 2.0], 0.9, 0.7).packed();
        assert_eq!(
            packed.spot_and_specular[0..3]
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            [0.0, 0.0, 1.0].map(f32::to_bits)
        );
        assert!((packed.attenuation_and_shadow[1] - 5.0).abs() < 1.0e-5);
        assert!((packed.attenuation_and_shadow[2] + 3.5).abs() < 1.0e-5);
    }

    #[test]
    fn oversized_sets_are_rejected_instead_of_truncated() {
        let lights = vec![LocalLight::omni([0.0; 3], [1.0; 3], 1.0); MAX_LOCAL_LIGHTS + 1];
        assert_eq!(
            LocalLightSet::new(lights),
            Err(LocalLightError::TooManyLights {
                actual: MAX_LOCAL_LIGHTS + 1
            })
        );
    }

    #[test]
    fn set_controls_shared_lighting_switches() {
        let mut set = LocalLightSet {
            specular_power: 32.0,
            shadows_enabled: true,
            ..LocalLightSet::default()
        };
        set.push(LocalLight::omni([0.0; 3], [1.0; 3], 5.0)).unwrap();
        let mut lighting = LightingParams::default();
        set.apply_to_lighting(&mut lighting);
        assert_eq!(
            lighting.local_light_params.map(f32::to_bits),
            [1.0, 32.0, 1.0, 1.0].map(f32::to_bits)
        );
        assert_eq!(pack_lights(&set)[0].position_and_omni[0].to_bits(), 0);
    }
}
