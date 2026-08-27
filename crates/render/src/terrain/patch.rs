//! Dynamic terrain-conforming patches used by impact decals, trails, and the
//! particle system's `eTerrainPatch` geometry.
//!
//! The input packing and shader equations follow the decompiled PC
//! `terrainheightfield` pipeline. Hardware tessellation is represented by a
//! fixed 16-by-16 indexed carrier grid, matching the oracle's maximum hull
//! factor without introducing a CPU height-sampling path.

use std::collections::HashMap;
use std::mem;

use glam::Mat4;
use num_traits::ToPrimitive;
use pipeline::ddx::DdxTexture;
use pipeline::source::{AssetSource, StdFileProvider};
use wgpu::util::DeviceExt;

use super::{CameraUniform, HEIGHTFIELD_SHADER, LightingParams, TerrainHeightfield};
use crate::lighting::LocalLightBuffer;

const PATCH_SUBDIVISIONS: u16 = 16;
const PATCH_VERTICES_PER_AXIS: u16 = PATCH_SUBDIVISIONS + 1;

/// Lighting family selected for one terrain-patch material.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerrainPatchShading {
    /// The full terrain lighting, shadow, blackmap, local-light, and fog path.
    #[default]
    Lit,
    /// The oracle's simple diffuse/color path used by unlit effects.
    Unlit,
}

/// One validated, tightly packed RGBA8 image.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPatchImage {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Row-major RGBA8 texels.
    pub pixels: Vec<u8>,
}

impl TerrainPatchImage {
    /// Creates a checked decoded image.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainPatchError::InvalidImage`] for zero dimensions,
    /// arithmetic overflow, or a byte count other than `width * height * 4`.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TerrainPatchError> {
        validate_image(width, height, pixels.len())?;
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Creates a one-texel fallback or test image.
    #[must_use]
    pub fn solid(rgba: [u8; 4]) -> Self {
        Self {
            width: 1,
            height: 1,
            pixels: rgba.to_vec(),
        }
    }
}

/// Decoded DDX maps and fixed shader settings for one patch batch.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPatchMaterial {
    /// Authored base asset path without a map suffix.
    pub name: String,
    /// `_df` diffuse map.
    pub diffuse: TerrainPatchImage,
    /// `_nm` tangent-space normal map.
    pub normal: TerrainPatchImage,
    /// `_op` opacity map.
    pub opacity: TerrainPatchImage,
    /// Optional `_sp` colored specular map; missing maps bind black.
    pub specular: Option<TerrainPatchImage>,
    /// Lit or simple diffuse oracle path.
    pub shading: TerrainPatchShading,
    /// Reciprocal-Schlick material specular power.
    pub specular_power: f32,
}

impl TerrainPatchMaterial {
    /// Resolves the shipped `_df`, `_nm`, `_op`, and optional `_sp` DDX family.
    ///
    /// # Errors
    ///
    /// Returns an error if a required map is absent or any present DDX cannot
    /// be decoded. Missing specular maps are intentional and use black.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        path: &str,
    ) -> Result<Self, TerrainPatchError> {
        let base = canonical_patch_path(path);
        let diffuse = load_required_map(source, &base, "_df")?;
        let normal = load_required_map(source, &base, "_nm")?;
        let opacity = load_required_map(source, &base, "_op")?;
        let specular = load_map(source, &base, "_sp")?;
        Ok(Self {
            name: base,
            diffuse,
            normal,
            opacity,
            specular,
            shading: TerrainPatchShading::Lit,
            specular_power: 16.0,
        })
    }

    /// Creates a material from already-decoded maps.
    #[must_use]
    pub fn from_images(
        name: impl Into<String>,
        diffuse: TerrainPatchImage,
        normal: TerrainPatchImage,
        opacity: TerrainPatchImage,
    ) -> Self {
        Self {
            name: name.into(),
            diffuse,
            normal,
            opacity,
            specular: None,
            shading: TerrainPatchShading::Lit,
            specular_power: 16.0,
        }
    }
}

/// One simulation-resolved terrain patch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainPatchInstance {
    /// World-space patch center before terrain conformance.
    pub center: [f32; 3],
    /// Half-axis corresponding to material U.
    pub axis_u: [f32; 3],
    /// Half-axis corresponding to material V.
    pub axis_v: [f32; 3],
    /// Vertical bias applied after terrain conformance.
    pub y_offset: f32,
    /// Authored unlit color intensity.
    pub intensity: f32,
    /// Linear instance color and opacity.
    pub color: [f32; 4],
    /// Material UV origin and extent `[u, v, width, height]`.
    pub uv_rect: [f32; 4],
}

impl Default for TerrainPatchInstance {
    fn default() -> Self {
        Self {
            center: [0.0; 3],
            axis_u: [0.5, 0.0, 0.0],
            axis_v: [0.0, 0.0, 0.5],
            y_offset: 0.01,
            intensity: 1.0,
            color: [1.0; 4],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

impl TerrainPatchInstance {
    /// Creates a horizontal axis-aligned patch from world half extents.
    #[must_use]
    pub fn axis_aligned(center: [f32; 3], half_extents: [f32; 2]) -> Self {
        Self {
            center,
            axis_u: [half_extents[0], 0.0, 0.0],
            axis_v: [0.0, 0.0, half_extents[1]],
            ..Self::default()
        }
    }

    fn packed(self) -> PackedPatchInstance {
        PackedPatchInstance {
            position: [self.center[0], self.center[1], self.center[2], 1.0],
            axis_v_and_axis_u_x: [
                self.axis_v[0],
                self.axis_v[1],
                self.axis_v[2],
                self.axis_u[0],
            ],
            axis_u_yz_offset_intensity: [
                self.axis_u[1],
                self.axis_u[2],
                self.y_offset,
                self.intensity,
            ],
            tex_uv: self.uv_rect,
            color: self.color,
        }
    }
}

/// Optional world textures shared by terrain-patch batches.
#[derive(Clone, Copy, Default)]
pub struct TerrainPatchWorldBindings<'a> {
    /// Canonical accepted-axis terrain position texture.
    pub heightfield: Option<TerrainHeightfield<'a>>,
    /// Decoded terrain visibility/holes texture.
    pub terrain_alpha: Option<&'a wgpu::TextureView>,
    /// Directional variance-shadow array.
    pub directional_shadow: Option<&'a wgpu::TextureView>,
    /// Fog-of-war visibility texture.
    pub blackmap: Option<&'a wgpu::TextureView>,
    /// Fog-of-war unexplored-color texture.
    pub unexplored: Option<&'a wgpu::TextureView>,
    /// Oracle-packed local-light buffer.
    pub local_lights: Option<&'a LocalLightBuffer>,
    /// Local spot/omni shadow array.
    pub local_shadow: Option<&'a wgpu::TextureView>,
    /// Optional 3D local-light color field.
    pub light_volume_color: Option<&'a wgpu::TextureView>,
    /// Optional 3D local-light direction field.
    pub light_volume_vector: Option<&'a wgpu::TextureView>,
}

/// Fixed creation inputs for a [`TerrainPatchRenderer`].
#[derive(Clone, Copy)]
pub struct TerrainPatchRendererDescriptor<'a> {
    /// HDR or presentation color format.
    pub color_format: wgpu::TextureFormat,
    /// Depth target format, or `None` for depth-free tests/passes.
    pub depth_format: Option<wgpu::TextureFormat>,
    /// Decoded material maps and shading family.
    pub material: &'a TerrainPatchMaterial,
    /// Scenario-global terrain and lighting resources.
    pub world: TerrainPatchWorldBindings<'a>,
}

/// Persistent GPU state for one dynamic terrain-patch material batch.
pub struct TerrainPatchRenderer {
    camera_buffer: wgpu::Buffer,
    lighting_buffer: wgpu::Buffer,
    world_bind_group: wgpu::BindGroup,
    material_bind_group: wgpu::BindGroup,
    domain_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: u32,
    pipeline: wgpu::RenderPipeline,
}

impl TerrainPatchRenderer {
    /// Uploads one material and creates the oracle-equivalent patch pipeline.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        descriptor: TerrainPatchRendererDescriptor<'_>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain Heightfield Oracle Shader"),
            source: wgpu::ShaderSource::Wgsl(HEIGHTFIELD_SHADER.into()),
        });
        let world_layout = create_world_layout(device);
        let material_layout = create_material_layout(device);
        let (camera_buffer, lighting_buffer, world_bind_group) = create_world_bind_group(
            device,
            queue,
            &world_layout,
            descriptor.world,
            descriptor.material,
        );
        let material_bind_group =
            create_material_bind_group(device, queue, &material_layout, descriptor.material);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain Patch Pipeline Layout"),
            bind_group_layouts: &[&world_layout, &material_layout],
            push_constant_ranges: &[],
        });
        let fragment_entry = match descriptor.material.shading {
            TerrainPatchShading::Lit => "fs_lit",
            TerrainPatchShading::Unlit => "fs_unlit",
        };
        let pipeline = create_pipeline(
            device,
            &shader,
            &pipeline_layout,
            descriptor.color_format,
            descriptor.depth_format,
            fragment_entry,
        );
        let (domain, indices) = patch_grid();
        let domain_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Patch Domain Grid"),
            contents: bytemuck::cast_slice(&domain),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Patch Domain Indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let subdivisions = u32::from(PATCH_SUBDIVISIONS);
        let index_count = subdivisions * subdivisions * 6;
        let instance_capacity = 1;
        let instance_buffer = create_instance_buffer(device, instance_capacity);
        Self {
            camera_buffer,
            lighting_buffer,
            world_bind_group,
            material_bind_group,
            domain_buffer,
            index_buffer,
            index_count,
            instance_buffer,
            instance_capacity,
            instance_count: 0,
            pipeline,
        }
    }

    /// Updates camera and scenario lighting without rebuilding materials.
    pub fn update_frame(
        &self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        lighting: &LightingParams,
    ) {
        let camera = CameraUniform {
            view_proj: view_projection.to_cols_array_2d(),
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        queue.write_buffer(&self.lighting_buffer, 0, bytemuck::bytes_of(lighting));
    }

    /// Replaces resolved patch instances, growing the GPU stream as needed.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainPatchError::TooManyInstances`] if the count cannot be
    /// represented by the GPU draw API.
    pub fn update_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[TerrainPatchInstance],
    ) -> Result<(), TerrainPatchError> {
        self.instance_count =
            u32::try_from(instances.len()).map_err(|_| TerrainPatchError::TooManyInstances {
                actual: instances.len(),
            })?;
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = create_instance_buffer(device, self.instance_capacity);
        }
        if instances.is_empty() {
            return Ok(());
        }
        let packed = instances
            .iter()
            .copied()
            .map(TerrainPatchInstance::packed)
            .collect::<Vec<_>>();
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&packed));
        Ok(())
    }

    /// Draws every current patch instance.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.instance_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.world_bind_group, &[]);
        pass.set_bind_group(1, &self.material_bind_group, &[]);
        pass.set_vertex_buffer(0, self.domain_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..self.index_count, 0, 0..self.instance_count);
    }
}

/// Terrain-patch asset or upload error.
#[derive(Debug, thiserror::Error)]
pub enum TerrainPatchError {
    /// A required DDX map was absent.
    #[error("terrain patch map not found: {0}")]
    MapNotFound(String),
    /// A present DDX map failed to parse or decode.
    #[error("failed to decode terrain patch map '{path}': {reason}")]
    MapDecode {
        /// Canonical DDX path stem.
        path: String,
        /// Decoder diagnostic.
        reason: String,
    },
    /// A decoded RGBA image had invalid dimensions or size.
    #[error(
        "invalid terrain patch image {width}x{height}: expected {expected} bytes, got {actual}"
    )]
    InvalidImage {
        /// Width in texels.
        width: u32,
        /// Height in texels.
        height: u32,
        /// Required RGBA byte count.
        expected: usize,
        /// Supplied byte count.
        actual: usize,
    },
    /// The simulation supplied more instances than one draw can address.
    #[error("terrain patch instance count {actual} exceeds the GPU u32 range")]
    TooManyInstances {
        /// Requested instance count.
        actual: usize,
    },
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedPatchInstance {
    position: [f32; 4],
    axis_v_and_axis_u_x: [f32; 4],
    axis_u_yz_offset_intensity: [f32; 4],
    tex_uv: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedPatchParams {
    terrain_info: [f32; 4],
    terrain_origin: [f32; 4],
    material_params: [f32; 4],
}

impl PackedPatchParams {
    fn new(world: TerrainPatchWorldBindings<'_>, material: &TerrainPatchMaterial) -> Self {
        let (terrain_info, terrain_origin) = world.heightfield.map_or(
            ([1.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]),
            |heightfield| {
                (
                    [
                        heightfield.dimension.max(1).to_f32().unwrap_or(f32::MAX),
                        heightfield.tile_scale.abs().max(f32::EPSILON).recip(),
                        heightfield.y_range,
                        heightfield.y_mid,
                    ],
                    [
                        heightfield.normalized_y_bias,
                        heightfield.world_min_xz[0],
                        heightfield.world_min_xz[1],
                        1.0,
                    ],
                )
            },
        );
        Self {
            terrain_info,
            terrain_origin,
            material_params: [
                material.specular_power.max(f32::EPSILON),
                f32::from(world.terrain_alpha.is_some()),
                0.0,
                0.0,
            ],
        }
    }
}

fn load_required_map(
    source: &mut AssetSource<StdFileProvider>,
    base: &str,
    suffix: &str,
) -> Result<TerrainPatchImage, TerrainPatchError> {
    load_map(source, base, suffix)?
        .ok_or_else(|| TerrainPatchError::MapNotFound(format!("{base}{suffix}")))
}

fn load_map(
    source: &mut AssetSource<StdFileProvider>,
    base: &str,
    suffix: &str,
) -> Result<Option<TerrainPatchImage>, TerrainPatchError> {
    let path = format!("{base}{suffix}");
    let Some(bytes) = source.resolve_with_fallback(&path, &[".ddx"]) else {
        return Ok(None);
    };
    let texture = DdxTexture::from_bytes(&bytes).map_err(|error| TerrainPatchError::MapDecode {
        path: path.clone(),
        reason: error.to_string(),
    })?;
    let decoded = texture
        .decode_to_rgba()
        .map_err(|error| TerrainPatchError::MapDecode {
            path,
            reason: error.to_string(),
        })?;
    TerrainPatchImage::from_rgba(decoded.width, decoded.height, decoded.pixels).map(Some)
}

fn canonical_patch_path(path: &str) -> String {
    let mut normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    let has_ddx_extension = normalized
        .get(normalized.len().saturating_sub(4)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".ddx"));
    if has_ddx_extension {
        normalized.truncate(normalized.len() - 4);
    }
    let lower = normalized.to_ascii_lowercase();
    for suffix in ["_df", "_nm", "_op", "_sp"] {
        if lower.ends_with(suffix) {
            normalized.truncate(normalized.len() - suffix.len());
            break;
        }
    }
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}

fn validate_image(width: u32, height: u32, actual: usize) -> Result<(), TerrainPatchError> {
    let expected = usize::try_from(
        u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|texels| texels.checked_mul(4))
            .unwrap_or(u64::MAX),
    )
    .unwrap_or(usize::MAX);
    if width == 0 || height == 0 || actual != expected {
        Err(TerrainPatchError::InvalidImage {
            width,
            height,
            expected,
            actual,
        })
    } else {
        Ok(())
    }
}

fn patch_grid() -> (Vec<[f32; 2]>, Vec<u16>) {
    let mut vertices = Vec::with_capacity(usize::from(PATCH_VERTICES_PER_AXIS).pow(2));
    for y in 0..=PATCH_SUBDIVISIONS {
        for x in 0..=PATCH_SUBDIVISIONS {
            vertices.push([
                f32::from(x) / f32::from(PATCH_SUBDIVISIONS),
                f32::from(y) / f32::from(PATCH_SUBDIVISIONS),
            ]);
        }
    }
    let mut indices = Vec::with_capacity(usize::from(PATCH_SUBDIVISIONS).pow(2) * 6);
    for y in 0..PATCH_SUBDIVISIONS {
        for x in 0..PATCH_SUBDIVISIONS {
            let top_left = y * PATCH_VERTICES_PER_AXIS + x;
            let top_right = top_left + 1;
            let bottom_left = top_left + PATCH_VERTICES_PER_AXIS;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }
    (vertices, indices)
}

fn create_world_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entries = [
        uniform_entry(0, wgpu::ShaderStages::VERTEX),
        uniform_entry(1, wgpu::ShaderStages::VERTEX_FRAGMENT),
        uniform_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT),
        texture_entry_layout(
            3,
            wgpu::TextureViewDimension::D2,
            wgpu::ShaderStages::VERTEX,
        ),
        sampler_entry(4, wgpu::ShaderStages::VERTEX),
        texture_entry_layout(
            5,
            wgpu::TextureViewDimension::D2,
            wgpu::ShaderStages::VERTEX,
        ),
        texture_entry_layout(
            6,
            wgpu::TextureViewDimension::D2Array,
            wgpu::ShaderStages::FRAGMENT,
        ),
        texture_entry_layout(
            7,
            wgpu::TextureViewDimension::D2,
            wgpu::ShaderStages::FRAGMENT,
        ),
        texture_entry_layout(
            8,
            wgpu::TextureViewDimension::D2,
            wgpu::ShaderStages::FRAGMENT,
        ),
        storage_entry(9, wgpu::ShaderStages::FRAGMENT),
        sampler_entry(10, wgpu::ShaderStages::FRAGMENT),
        texture_entry_layout(
            11,
            wgpu::TextureViewDimension::D2Array,
            wgpu::ShaderStages::FRAGMENT,
        ),
        texture_entry_layout(
            12,
            wgpu::TextureViewDimension::D3,
            wgpu::ShaderStages::FRAGMENT,
        ),
        texture_entry_layout(
            13,
            wgpu::TextureViewDimension::D3,
            wgpu::ShaderStages::FRAGMENT,
        ),
    ];
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Terrain Patch World Layout"),
        entries: &entries,
    })
}

fn create_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entries = [
        texture_entry_layout(
            0,
            wgpu::TextureViewDimension::D2,
            wgpu::ShaderStages::FRAGMENT,
        ),
        texture_entry_layout(
            1,
            wgpu::TextureViewDimension::D2,
            wgpu::ShaderStages::FRAGMENT,
        ),
        texture_entry_layout(
            2,
            wgpu::TextureViewDimension::D2,
            wgpu::ShaderStages::FRAGMENT,
        ),
        texture_entry_layout(
            3,
            wgpu::TextureViewDimension::D2,
            wgpu::ShaderStages::FRAGMENT,
        ),
        sampler_entry(4, wgpu::ShaderStages::FRAGMENT),
    ];
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Terrain Patch Material Layout"),
        entries: &entries,
    })
}

fn create_world_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    world: TerrainPatchWorldBindings<'_>,
    material: &TerrainPatchMaterial,
) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Terrain Patch Camera Uniform"),
        contents: bytemuck::bytes_of(&CameraUniform::default()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let patch_params = PackedPatchParams::new(world, material);
    let patch_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Terrain Patch Parameters"),
        contents: bytemuck::bytes_of(&patch_params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let lighting_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Terrain Patch Lighting Uniform"),
        contents: bytemuck::bytes_of(&LightingParams::default()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let fallback_heightfield = create_2d_fallback(device, queue, "Patch Heightfield", [0; 4]);
    let heightfield = world
        .heightfield
        .map_or(&fallback_heightfield, |heightfield| heightfield.view);
    let fallback_alpha = create_2d_fallback(device, queue, "Patch Terrain Alpha", [255; 4]);
    let terrain_alpha = world.terrain_alpha.unwrap_or(&fallback_alpha);
    let fallback_directional = create_array_fallback(device, queue, "Patch CSM", 4);
    let directional = world.directional_shadow.unwrap_or(&fallback_directional);
    let fallback_blackmap = create_2d_fallback(device, queue, "Patch Blackmap", [0; 4]);
    let blackmap = world.blackmap.unwrap_or(&fallback_blackmap);
    let fallback_unexplored = create_2d_fallback(device, queue, "Patch Unexplored", [0; 4]);
    let unexplored = world.unexplored.unwrap_or(&fallback_unexplored);
    let fallback_lights = LocalLightBuffer::empty(device);
    let local_lights = world.local_lights.unwrap_or(&fallback_lights);
    let fallback_local_shadow = create_array_fallback(device, queue, "Patch Local Shadow", 8);
    let local_shadow = world.local_shadow.unwrap_or(&fallback_local_shadow);
    let fallback_volume_color = create_volume_fallback(device, queue, "Patch Volume Color", [0; 4]);
    let volume_color = world.light_volume_color.unwrap_or(&fallback_volume_color);
    let fallback_volume_vector =
        create_volume_fallback(device, queue, "Patch Volume Vector", [128, 128, 128, 255]);
    let volume_vector = world.light_volume_vector.unwrap_or(&fallback_volume_vector);
    let height_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Terrain Patch Heightfield Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let lighting_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Terrain Patch Lighting Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Terrain Patch World Bind Group"),
        layout,
        entries: &[
            buffer_entry(0, &camera_buffer),
            buffer_entry(1, &patch_buffer),
            buffer_entry(2, &lighting_buffer),
            texture_entry(3, heightfield),
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&height_sampler),
            },
            texture_entry(5, terrain_alpha),
            texture_entry(6, directional),
            texture_entry(7, blackmap),
            texture_entry(8, unexplored),
            buffer_entry(9, local_lights.buffer()),
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::Sampler(&lighting_sampler),
            },
            texture_entry(11, local_shadow),
            texture_entry(12, volume_color),
            texture_entry(13, volume_vector),
        ],
    });
    (camera_buffer, lighting_buffer, bind_group)
}

fn create_material_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    material: &TerrainPatchMaterial,
) -> wgpu::BindGroup {
    let diffuse = create_image_view(device, queue, "Patch Diffuse", &material.diffuse);
    let normal = create_image_view(device, queue, "Patch Normal", &material.normal);
    let opacity = create_image_view(device, queue, "Patch Opacity", &material.opacity);
    let fallback_specular = TerrainPatchImage::solid([0, 0, 0, 255]);
    let specular = create_image_view(
        device,
        queue,
        "Patch Specular",
        material.specular.as_ref().unwrap_or(&fallback_specular),
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Terrain Patch Material Sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Terrain Patch Material Bind Group"),
        layout,
        entries: &[
            texture_entry(0, &diffuse),
            texture_entry(1, &normal),
            texture_entry(2, &opacity),
            texture_entry(3, &specular),
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    fragment_entry: &str,
) -> wgpu::RenderPipeline {
    let constants = HashMap::new();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Terrain Patch Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[domain_vertex_layout(), instance_vertex_layout()],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                zero_initialize_workgroup_memory: false,
            },
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                zero_initialize_workgroup_memory: false,
            },
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn domain_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<[f32; 2]>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

fn instance_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<PackedPatchInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let byte_size = capacity
        .checked_mul(mem::size_of::<PackedPatchInstance>())
        .and_then(|size| u64::try_from(size).ok())
        .unwrap_or(u64::MAX);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Terrain Patch Instance Buffer"),
        size: byte_size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_image_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    image: &TerrainPatchImage,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &image.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width * 4),
            rows_per_image: Some(image.height),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_2d_fallback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    color: [u8; 4],
) -> wgpu::TextureView {
    create_image_view(device, queue, label, &TerrainPatchImage::solid(color))
}

fn create_array_fallback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    layers: u32,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let byte_count = usize::try_from(layers).unwrap_or(1).saturating_mul(4);
    queue.write_texture(
        texture.as_image_copy(),
        &vec![255; byte_count],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

fn create_volume_fallback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    color: [u8; 4],
) -> wgpu::TextureView {
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &color,
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry_layout(
    binding: u32,
    view_dimension: wgpu::TextureViewDimension,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
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
    use std::mem;

    use super::{
        PATCH_SUBDIVISIONS, PackedPatchInstance, TerrainPatchImage, TerrainPatchInstance,
        canonical_patch_path, patch_grid,
    };

    #[test]
    fn instance_matches_five_float4_oracle_control_point() {
        let instance = TerrainPatchInstance {
            center: [1.0, 2.0, 3.0],
            axis_u: [4.0, 5.0, 6.0],
            axis_v: [7.0, 8.0, 9.0],
            y_offset: 10.0,
            intensity: 11.0,
            color: [12.0, 13.0, 14.0, 15.0],
            uv_rect: [16.0, 17.0, 18.0, 19.0],
        }
        .packed();
        assert_eq!(mem::size_of::<PackedPatchInstance>(), 5 * 16);
        assert_eq!(
            instance.position.map(f32::to_bits),
            [1.0, 2.0, 3.0, 1.0].map(f32::to_bits)
        );
        assert_eq!(
            instance.axis_v_and_axis_u_x.map(f32::to_bits),
            [7.0, 8.0, 9.0, 4.0].map(f32::to_bits)
        );
        assert_eq!(
            instance.axis_u_yz_offset_intensity.map(f32::to_bits),
            [5.0, 6.0, 10.0, 11.0].map(f32::to_bits)
        );
    }

    #[test]
    fn carrier_grid_matches_oracle_maximum_tessellation() {
        let (vertices, indices) = patch_grid();
        let vertices_per_axis = usize::from(PATCH_SUBDIVISIONS + 1);
        assert_eq!(vertices.len(), vertices_per_axis.pow(2));
        assert_eq!(indices.len(), usize::from(PATCH_SUBDIVISIONS).pow(2) * 6);
        assert_eq!(
            vertices
                .first()
                .copied()
                .map(|value| value.map(f32::to_bits)),
            Some([0.0, 0.0].map(f32::to_bits))
        );
        assert_eq!(
            vertices
                .last()
                .copied()
                .map(|value| value.map(f32::to_bits)),
            Some([1.0, 1.0].map(f32::to_bits))
        );
    }

    #[test]
    fn decal_map_suffixes_canonicalize_to_one_base() {
        assert_eq!(
            canonical_patch_path("decals/warthog01_df.ddx"),
            "art\\decals\\warthog01"
        );
        assert_eq!(
            canonical_patch_path("art\\decals\\warthog01_nm"),
            "art\\decals\\warthog01"
        );
    }

    #[test]
    fn invalid_rgba_size_is_rejected() {
        assert!(TerrainPatchImage::from_rgba(2, 2, vec![0; 15]).is_err());
    }
}
