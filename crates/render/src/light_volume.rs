//! Retail-style buffered local-light field.
//!
//! Halo Wars routes non-shadowed lights marked `mLightBuffered` into two
//! 128×128×16 volumes. The color field stores radiance at one twelfth scale;
//! the vector field stores the normalized, attenuation-weighted light
//! direction. This implementation keeps those shader-visible semantics while
//! using half-float storage instead of the original packed 10:10:10:2 target.

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::lighting::{LocalLight, LocalLightShape};
use crate::terrain::LightingParams;

const SHADER: &str = include_str!("light_volume.wgsl");

/// Width of the retail local-light volume.
pub const LIGHT_VOLUME_WIDTH: u32 = 128;
/// Height of the retail local-light volume (world Z after the Y/Z swizzle).
pub const LIGHT_VOLUME_HEIGHT: u32 = 128;
/// Depth of the retail local-light volume (world Y after the Y/Z swizzle).
pub const LIGHT_VOLUME_DEPTH: u32 = 16;
/// Maximum number of local-light handles managed by the retail scene manager.
pub const MAX_BUFFERED_LIGHTS: usize = 1024;

const COLOR_SCALE: f32 = 1.0 / 12.0;
/// Multiplier used by consumers to decode the normalized color field.
pub const LIGHT_VOLUME_DECODE_SCALE: f32 = 12.0;
const MIN_BOUNDS_DIMENSION: f32 = 0.5;
const BOUNDS_PADDING: f32 = 1.0;

/// GPU resources and addressing state for the shared local-light field.
pub struct LocalLightVolume {
    color_view: wgpu::TextureView,
    vector_view: wgpu::TextureView,
    params_buffer: wgpu::Buffer,
    lights_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
    rows: [[f32; 4]; 3],
    light_count: usize,
}

impl LocalLightVolume {
    /// Creates the fixed-size color and direction fields and their fill pipeline.
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        let color_view = create_volume_view(device, "Local Light Volume Color");
        let vector_view = create_volume_view(device, "Local Light Volume Vector");
        let params = PackedVolumeParams::empty();
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Local Light Volume Parameters"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let empty_lights = vec![PackedBufferedLight::default(); MAX_BUFFERED_LIGHTS];
        let lights_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Local Light Volume Lights"),
            contents: bytemuck::cast_slice(&empty_lights),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let layout = create_layout(device);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Local Light Volume Bind Group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lights_buffer.as_entire_binding(),
                },
                texture_entry(2, &color_view),
                texture_entry(3, &vector_view),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Local Light Volume Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Local Light Volume Pipeline Layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Local Light Volume Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("fill_light_volume"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        Self {
            color_view,
            vector_view,
            params_buffer,
            lights_buffer,
            bind_group,
            pipeline,
            rows: identity_rows(),
            light_count: 0,
        }
    }

    /// Rebuilds the volume from the current visible buffered lights.
    ///
    /// Invalid records are ignored. Inputs beyond the original 1,024-handle
    /// scene-manager capacity are deterministically omitted.
    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, lights: &[LocalLight]) {
        let valid = lights
            .iter()
            .copied()
            .filter(light_is_finite)
            .take(MAX_BUFFERED_LIGHTS)
            .collect::<Vec<_>>();
        let transform = VolumeTransform::from_lights(&valid);
        self.rows = transform.map_or_else(identity_rows, |value| value.rows);
        self.light_count = valid.len();
        let params = transform.map_or_else(PackedVolumeParams::empty, |value| {
            PackedVolumeParams::new(value, valid.len())
        });
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));
        if !valid.is_empty() {
            let packed = valid
                .iter()
                .copied()
                .map(PackedBufferedLight::from_light)
                .collect::<Vec<_>>();
            queue.write_buffer(&self.lights_buffer, 0, bytemuck::cast_slice(&packed));
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Local Light Volume Encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Local Light Volume Fill"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(
                LIGHT_VOLUME_WIDTH.div_ceil(8),
                LIGHT_VOLUME_HEIGHT.div_ceil(8),
                LIGHT_VOLUME_DEPTH,
            );
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Returns the sampleable radiance field.
    #[must_use]
    pub const fn color_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }

    /// Returns the sampleable encoded-direction field.
    #[must_use]
    pub const fn vector_view(&self) -> &wgpu::TextureView {
        &self.vector_view
    }

    /// Returns the world-to-volume transform rows used by all consumers.
    #[must_use]
    pub const fn world_to_volume_rows(&self) -> [[f32; 4]; 3] {
        self.rows
    }

    /// Returns whether the field currently contains at least one light.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.light_count != 0
    }

    /// Returns the number of lights accumulated in the latest field.
    #[must_use]
    pub const fn light_count(&self) -> usize {
        self.light_count
    }

    /// Applies the field switch and transform to the shared lighting constants.
    pub fn apply_to_lighting(&self, lighting: &mut LightingParams) {
        lighting.light_volume_params = [f32::from(self.is_active()), 0.0, 0.0, 0.0];
        lighting.light_volume_row0 = self.rows[0];
        lighting.light_volume_row1 = self.rows[1];
        lighting.light_volume_row2 = self.rows[2];
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VolumeTransform {
    low: Vec3,
    extent: Vec3,
    rows: [[f32; 4]; 3],
}

impl VolumeTransform {
    fn from_lights(lights: &[LocalLight]) -> Option<Self> {
        let first = lights.first()?;
        let first_position = Vec3::from_array(first.position);
        let first_radius = Vec3::splat(first.radius.max(0.0));
        let mut low = first_position - first_radius;
        let mut high = first_position + first_radius;
        for light in &lights[1..] {
            let position = Vec3::from_array(light.position);
            let radius = Vec3::splat(light.radius.max(0.0));
            low = low.min(position - radius);
            high = high.max(position + radius);
        }
        low -= Vec3::splat(BOUNDS_PADDING);
        high += Vec3::splat(BOUNDS_PADDING);
        for axis in 0..3 {
            if high[axis] - low[axis] < MIN_BOUNDS_DIMENSION {
                low[axis] -= MIN_BOUNDS_DIMENSION * 0.5;
                high[axis] += MIN_BOUNDS_DIMENSION * 0.5;
            }
        }
        let extent = high - low;
        let inverse = extent.recip();
        let rows = [
            [inverse.x, 0.0, 0.0, -low.x * inverse.x],
            [0.0, 0.0, inverse.z, -low.z * inverse.z],
            [0.0, inverse.y, 0.0, -low.y * inverse.y],
        ];
        Some(Self { low, extent, rows })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedVolumeParams {
    world_low: [f32; 4],
    world_extent: [f32; 4],
    counts: [u32; 4],
}

impl PackedVolumeParams {
    const fn empty() -> Self {
        Self {
            world_low: [0.0; 4],
            world_extent: [1.0, 1.0, 1.0, 0.0],
            counts: [0; 4],
        }
    }

    fn new(transform: VolumeTransform, count: usize) -> Self {
        Self {
            world_low: transform.low.extend(0.0).to_array(),
            world_extent: transform.extent.extend(0.0).to_array(),
            counts: [u32::try_from(count).unwrap_or(u32::MAX), 0, 0, 0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedBufferedLight {
    position_and_radius: [f32; 4],
    color_and_omni_mul: [f32; 4],
    attenuation_and_specular: [f32; 4],
    spot_at_and_add: [f32; 4],
}

impl PackedBufferedLight {
    fn from_light(light: LocalLight) -> Self {
        let radius = light.radius.max(f32::EPSILON);
        let far_start = light.far_attenuation_start.clamp(0.0, 0.999);
        let inverse_falloff = (1.0 - far_start).recip();
        let omni_mul = -radius.recip() * inverse_falloff;
        let omni_add = 1.0 + far_start * inverse_falloff;
        let (spot_at, spot_mul, spot_add) = spot_parameters(light.shape);
        Self {
            position_and_radius: [
                light.position[0],
                light.position[1],
                light.position[2],
                radius,
            ],
            color_and_omni_mul: [
                light.color[0] * COLOR_SCALE,
                light.color[1] * COLOR_SCALE,
                light.color[2] * COLOR_SCALE,
                omni_mul,
            ],
            attenuation_and_specular: [
                light.decay_distance.max(0.0),
                spot_mul,
                spot_add,
                f32::from(light.specular_intensity > 0.0),
            ],
            spot_at_and_add: [spot_at.x, spot_at.y, spot_at.z, omni_add],
        }
    }
}

fn spot_parameters(shape: LocalLightShape) -> (Vec3, f32, f32) {
    match shape {
        LocalLightShape::Omni => (Vec3::Y, 0.0, 1.0),
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
            let inner = inner_cos.clamp(-1.0, 1.0);
            let outer = outer_cos.clamp(-1.0, inner);
            let range = (inner - outer).max(f32::EPSILON);
            (direction, range.recip(), -outer / range)
        }
    }
}

fn light_is_finite(light: &LocalLight) -> bool {
    light.position.into_iter().all(f32::is_finite)
        && light.color.into_iter().all(f32::is_finite)
        && light.radius.is_finite()
        && light.radius > 0.0
        && light.far_attenuation_start.is_finite()
        && light.decay_distance.is_finite()
        && light.specular_intensity.is_finite()
        && match light.shape {
            LocalLightShape::Omni => true,
            LocalLightShape::Spot {
                direction,
                inner_cos,
                outer_cos,
            } => {
                direction.into_iter().all(f32::is_finite)
                    && inner_cos.is_finite()
                    && outer_cos.is_finite()
            }
        }
}

fn identity_rows() -> [[f32; 4]; 3] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ]
}

fn create_volume_view(device: &wgpu::Device, label: &'static str) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: LIGHT_VOLUME_WIDTH,
                height: LIGHT_VOLUME_HEIGHT,
                depth_or_array_layers: LIGHT_VOLUME_DEPTH,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Local Light Volume Layout"),
        entries: &[
            buffer_layout_entry(0, wgpu::BufferBindingType::Uniform),
            buffer_layout_entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
            storage_texture_layout_entry(2),
            storage_texture_layout_entry(3),
        ],
    })
}

fn buffer_layout_entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_texture_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba16Float,
            view_dimension: wgpu::TextureViewDimension::D3,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

#[cfg(test)]
mod tests {
    use glam::{Vec3, Vec4};

    use super::{COLOR_SCALE, PackedBufferedLight, VolumeTransform};
    use crate::lighting::LocalLight;

    #[test]
    fn volume_transform_uses_retail_yz_texture_swizzle() {
        let light = LocalLight::omni([10.0, 20.0, 30.0], [1.0; 3], 2.0);
        let transform = VolumeTransform::from_lights(&[light]).expect("one light has bounds");
        let low = transform.low.extend(1.0);
        let high = (transform.low + transform.extent).extend(1.0);
        let map = |position: Vec4| {
            Vec3::new(
                Vec4::from_array(transform.rows[0]).dot(position),
                Vec4::from_array(transform.rows[1]).dot(position),
                Vec4::from_array(transform.rows[2]).dot(position),
            )
        };
        assert!(map(low).abs().max_element() < 1.0e-6);
        assert!((map(high) - Vec3::ONE).abs().max_element() < 1.0e-6);
        let y_step = map(low + Vec4::Y) - map(low);
        let z_step = map(low + Vec4::Z) - map(low);
        assert!(y_step.z > 0.0);
        assert!(z_step.y > 0.0);
    }

    #[test]
    fn packed_volume_light_keeps_retail_attenuation_and_color_scale() {
        let mut light = LocalLight::spot(
            [1.0, 2.0, 3.0],
            [12.0, 6.0, 3.0],
            8.0,
            [0.0, 0.0, 2.0],
            0.9,
            0.7,
        );
        light.far_attenuation_start = 0.5;
        light.decay_distance = 4.0;
        light.specular_intensity = f32::from_bits(0x4048_f5c3);
        let packed = PackedBufferedLight::from_light(light);
        assert_eq!(
            packed.color_and_omni_mul[0].to_bits(),
            (12.0 * COLOR_SCALE).to_bits()
        );
        assert_eq!(
            packed.color_and_omni_mul[3].to_bits(),
            (-0.25_f32).to_bits()
        );
        assert_eq!(packed.spot_at_and_add[3].to_bits(), 2.0_f32.to_bits());
        assert!((packed.attenuation_and_specular[1] - 5.0).abs() < 1.0e-5);
        assert!((packed.attenuation_and_specular[2] + 3.5).abs() < 1.0e-5);
        assert_eq!(
            packed.attenuation_and_specular[3].to_bits(),
            1.0_f32.to_bits()
        );
        assert_eq!(packed.spot_at_and_add[0..3], [0.0_f32, 0.0_f32, 1.0_f32]);
    }
}
