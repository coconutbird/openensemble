//! Pipeline and bind group creation for terrain rendering.
//!
//! Contains the main GPU resource initialization functions that create
//! render pipelines, bind group layouts, and wire everything together.

use glam::Vec3;
use num_traits::ToPrimitive;
use render::lighting::LocalLightBuffer;
use render::terrain::{
    CompositeBindings, CompositingConfig, CompositorResources, GPU_TESS_SHADER, GpuTessParams,
    LightingParams, NORMALIZED_TERRAIN_Y_OFFSET, TerrainParams,
};
use render::wgpu;
use wgpu::util::DeviceExt;

use crate::gpu::{create_depth_texture, xtd_packed_to_world};
use crate::types::{AlbedoData, GpuResources, RawXtdData};
use crate::viewer::TerrainViewer;

struct CameraResources {
    buffer: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

struct TerrainSamplers {
    position: wgpu::Sampler,
    terrain: wgpu::Sampler,
    alpha: wgpu::Sampler,
    lighting: wgpu::Sampler,
}

struct CompositorInputs<'a> {
    terrain_array: &'a wgpu::TextureView,
    normal_array: &'a wgpu::TextureView,
    specular_array: &'a wgpu::TextureView,
    alpha_atlas: &'a wgpu::TextureView,
    alpha_atlas_hi: &'a wgpu::TextureView,
    chunk_layers: &'a wgpu::Buffer,
    texture_scales: &'a wgpu::Buffer,
    terrain_sampler: &'a wgpu::Sampler,
    alpha_sampler: &'a wgpu::Sampler,
}

fn buffer_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    buffer_type: wgpu::BufferBindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: buffer_type,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    dimension: wgpu::TextureViewDimension,
    sample_type: wgpu::TextureSampleType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: dimension,
            sample_type,
        },
        count: None,
    }
}

fn sampler_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn sampler_entry(binding: u32, sampler: &wgpu::Sampler) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Sampler(sampler),
    }
}

fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn create_camera_resources(device: &wgpu::Device) -> CameraResources {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Camera Uniform Buffer"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Camera Bind Group Layout"),
        entries: &[buffer_layout_entry(
            0,
            wgpu::ShaderStages::VERTEX,
            wgpu::BufferBindingType::Uniform,
        )],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Camera Bind Group"),
        layout: &layout,
        entries: &[buffer_entry(0, &buffer)],
    });
    CameraResources {
        buffer,
        layout,
        bind_group,
    }
}

fn create_uniform_buffer<T: bytemuck::Pod>(
    device: &wgpu::Device,
    label: &str,
    value: &T,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::bytes_of(value),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

fn create_terrain_samplers(device: &wgpu::Device) -> TerrainSamplers {
    let position = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Terrain Position Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let terrain = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Terrain Sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        anisotropy_clamp: 16,
        ..Default::default()
    });
    let alpha = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Alpha Atlas Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    let lighting = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Terrain Lighting Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    TerrainSamplers {
        position,
        terrain,
        alpha,
        lighting,
    }
}

impl TerrainViewer {
    fn init_compositor(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        inputs: &CompositorInputs<'_>,
    ) {
        let config = CompositingConfig::default();
        log::info!(
            "Initializing GPU compositor: {}×{} atlas ({} chunks)",
            config.atlas_width,
            config.atlas_height,
            config.total_chunks()
        );
        let compositor = CompositorResources::new(device, config);
        let (_, decal_alpha_view) = self.create_decal_alpha_atlas(device, queue);
        let (_, decal_alpha_hi_view) = self.create_decal_alpha_atlas_hi(device, queue);
        let (decal_diffuse_view, decal_opacity_view) =
            self.create_decal_texture_arrays(device, queue);
        let decal_layers = self.create_chunk_decal_layers_buffer(device);
        let decal_instances = self.create_decal_instances_buffer(device);
        let decal_scales = self.create_decal_uv_scales_buffer(device);
        let bind_group = compositor.create_bind_group(
            device,
            &CompositeBindings {
                terrain_array: inputs.terrain_array,
                alpha_atlas: inputs.alpha_atlas,
                chunk_layers: inputs.chunk_layers,
                texture_scales: inputs.texture_scales,
                terrain_sampler: inputs.terrain_sampler,
                alpha_sampler: inputs.alpha_sampler,
                alpha_atlas_hi: inputs.alpha_atlas_hi,
                decal_alpha_atlas: &decal_alpha_view,
                chunk_decal_layers: &decal_layers,
                decal_instances: &decal_instances,
                decal_uv_scales: &decal_scales,
                normal_array: inputs.normal_array,
                decal_alpha_atlas_hi: &decal_alpha_hi_view,
                decal_diffuse_array: &decal_diffuse_view,
                decal_opacity_array: &decal_opacity_view,
                specular_array: inputs.specular_array,
            },
        );
        self.compositor = Some(compositor);
        self.compositor_bind_group = Some(bind_group);
        log::info!("GPU compositor initialized successfully");
    }

    /// Calculate chunk center positions based on terrain bounds.
    /// Chunks are arranged in a 16×16 grid covering the terrain.
    pub(crate) fn calculate_chunk_centers(&mut self) {
        let Some(scene) = &self.scene else {
            return;
        };
        let terrain = &scene.mesh;

        let world_min = terrain.world_min;
        let world_max = terrain.world_max;
        let chunks_x = 16_u32;
        let chunks_z = 16_u32;

        let chunk_width = (world_max[0] - world_min[0]) / 16.0;
        let chunk_depth = (world_max[2] - world_min[2]) / 16.0;
        let chunk_height = (world_max[1] - world_min[1]) / 2.0; // Average Y for center

        self.chunk_centers.clear();
        for cz in 0..chunks_z {
            for cx in 0..chunks_x {
                let center_x = world_min[0]
                    + (cx.to_f32().expect("chunk X coordinate must fit f32") + 0.5) * chunk_width;
                let center_y = world_min[1] + chunk_height; // Approximate center Y
                let center_z = world_min[2]
                    + (cz.to_f32().expect("chunk Z coordinate must fit f32") + 0.5) * chunk_depth;
                self.chunk_centers.push([center_x, center_y, center_z]);
            }
        }

        log::info!(
            "Calculated {} chunk centers for LOD (chunk size: {:.1} x {:.1})",
            self.chunk_centers.len(),
            chunk_width,
            chunk_depth
        );
    }

    /// Initialize foliage GPU resources for rendering grass/vegetation.
    pub(crate) fn init_foliage_resources(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: Option<&crate::foliage::FoliageWorldBindings<'_>>,
    ) {
        let Some(scene) = &self.scene else { return };

        if scene.foliage_sets.is_empty() {
            log::info!("No foliage sets to render");
            return;
        }

        let Some(gpu) = &self.gpu else {
            log::warn!("Cannot initialize foliage: GPU resources not available");
            return;
        };

        log::info!(
            "Initializing foliage resources: {} sets, {} QN chunks",
            scene.foliage_sets.len(),
            scene.foliage_qn_chunks.len()
        );

        let mut foliage_resources = crate::foliage::FoliageResources::new(
            device,
            self.scene_format,
            &gpu.camera_bind_group_layout,
        );

        for (i, set) in scene.foliage_sets.iter().enumerate() {
            if let Some(set_resources) = foliage_resources.create_set_resources(device, queue, set)
            {
                log::info!(
                    "  Created foliage set {} resources: {} blade types, {} verts per blade",
                    i,
                    set_resources.num_blade_types,
                    set_resources.num_verts_per_blade
                );
                foliage_resources.set_resources.push(set_resources);
            }
        }

        if !foliage_resources.set_resources.is_empty() {
            if let Some(raw_data) = &scene.raw_xtd_data {
                foliage_resources.build_draw_calls(
                    device,
                    queue,
                    &scene.foliage_qn_chunks,
                    raw_data.num_verts_per_axis,
                );
                foliage_resources.create_params_bind_group(device, queue, raw_data, world);
            } else {
                log::warn!("Cannot create foliage params bind group: no terrain data");
            }
        }

        self.foliage_resources = Some(foliage_resources);
    }

    /// Initialize road GPU resources for rendering roads on terrain.
    pub(crate) fn init_road_resources(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(scene) = &self.scene else { return };
        if scene.road_chunks.is_empty() {
            log::info!("No road data to render");
            return;
        }
        let Some(gpu) = &self.gpu else {
            log::warn!("Cannot initialize roads: GPU resources not available");
            return;
        };
        let camera_layout = gpu.camera_bind_group_layout.clone();
        let road_chunks = scene.road_chunks.clone();
        let Some(mut source) = self.asset_source.take() else {
            log::warn!("Cannot load road textures: no asset source");
            return;
        };
        let loaded: Vec<_> = road_chunks
            .into_iter()
            .filter_map(|road| {
                if road.positions.is_empty() {
                    log::warn!("Road '{}' has no vertices", road.texture_name);
                    return None;
                }
                render::terrain::load_road_textures(&mut source, &road.texture_name)
                    .map(|textures| (road, textures))
            })
            .collect();
        self.asset_source = Some(source);
        if loaded.is_empty() {
            log::warn!("No road material textures could be loaded");
            return;
        }
        let batch_inputs: Vec<_> = loaded
            .iter()
            .map(|(road, textures)| crate::roads::RoadBatchInput {
                positions: &road.positions,
                uvs: &road.uvs,
                albedo_pixels: &textures.albedo_pixels,
                normal_pixels: &textures.normal_pixels,
                specular_pixels: &textures.specular_pixels,
                texture_size: [textures.width, textures.height],
            })
            .collect();
        let Some(raw_terrain) = self
            .scene
            .as_ref()
            .and_then(|loaded_scene| loaded_scene.raw_xtd_data.as_ref())
        else {
            log::warn!("Cannot conform roads without raw terrain position data");
            return;
        };
        let shadow_view = self
            .shadow_resources
            .as_ref()
            .map(|shadow| shadow.shadow_view.clone());
        self.road_resources = Some(crate::roads::create_road_resources(
            device,
            queue,
            &crate::roads::RoadResourceInput {
                camera_bind_group_layout: &camera_layout,
                surface_format: self.scene_format,
                raw_terrain,
                shadow_view: shadow_view.as_ref(),
                batches: &batch_inputs,
                bump_power: self.bump_power,
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        create_patch_instances, downsample_rgb10a2, tessellation_factor, world_patch_level,
    };
    use crate::types::RawXtdData;
    use render::terrain::TerrainTessellationData;

    fn raw_with_tessellation(levels: Vec<u8>, patches_x: u32, patches_z: u32) -> RawXtdData {
        RawXtdData {
            packed_positions: Vec::new(),
            packed_normals: Vec::new(),
            num_verts_per_axis: patches_x * 16,
            mid: [0.0; 3],
            range: [1.0; 3],
            tile_scale: 1.0,
            world_min: [0.0; 3],
            world_max: [1.0; 3],
            tessellation: Some(TerrainTessellationData {
                patches_x,
                patches_z,
                levels,
            }),
            ao_data: None,
            alpha_data: None,
        }
    }

    #[test]
    fn tessellation_levels_match_the_hull_shader_factors() {
        assert_eq!(tessellation_factor(0), 16);
        assert_eq!(tessellation_factor(1), 8);
        assert_eq!(tessellation_factor(2), 4);
        assert_eq!(tessellation_factor(3), 2);
    }

    #[test]
    fn tessellation_levels_follow_the_xtd_world_axis_conversion() {
        let levels = [0, 1, 2, 3];

        assert_eq!(world_patch_level(&levels, 2, 0, 0), 0);
        assert_eq!(world_patch_level(&levels, 2, 1, 0), 2);
        assert_eq!(world_patch_level(&levels, 2, 0, 1), 1);
        assert_eq!(world_patch_level(&levels, 2, 1, 1), 3);
    }

    #[test]
    fn patch_instances_raise_shared_edges_to_the_finer_neighbor() {
        let raw = raw_with_tessellation(vec![0, 3, 3, 3], 2, 2);
        let instances = create_patch_instances(&raw, 2, 2);

        assert_eq!(instances[0], [0, 16, 16, 16, 16, 16, 16, 0]);
        assert_eq!(instances[1], [1, 2, 16, 2, 2, 2, 2, 0]);
        assert_eq!(instances[2], [2, 16, 2, 2, 2, 2, 2, 0]);
        assert_eq!(instances[3], [3, 2, 2, 2, 2, 2, 2, 0]);
    }

    #[test]
    fn packed_position_mip_averages_each_unorm_channel() {
        let pack = |red: u32, green: u32, blue: u32, alpha: u32| {
            red | (green << 10) | (blue << 20) | (alpha << 30)
        };
        let source = [
            pack(0, 100, 200, 0),
            pack(4, 104, 204, 1),
            pack(8, 108, 208, 2),
            pack(12, 112, 212, 3),
        ];

        assert_eq!(downsample_rgb10a2(&source, 2), [pack(6, 106, 206, 2)]);
    }
}

#[derive(Copy, Clone)]
struct TessellationBuildConfig {
    surface_size: [u32; 2],
    num_patches: u32,
    total_patches: u32,
}

struct PatchMesh {
    vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

struct TessellationTextures {
    position: wgpu::TextureView,
    position_for_shadow: wgpu::TextureView,
    normal: wgpu::TextureView,
    ao: wgpu::TextureView,
    alpha: wgpu::TextureView,
    normal_map_array: wgpu::TextureView,
    specular_map_array: wgpu::TextureView,
    terrain_array: wgpu::TextureView,
    alpha_atlas: wgpu::TextureView,
    alpha_atlas_hi: wgpu::TextureView,
    dynamic_alpha: wgpu::TextureView,
    chunk_layers: wgpu::Buffer,
    samplers: TerrainSamplers,
}

struct TessellationStageOne {
    patch_mesh: PatchMesh,
    instance_buffer: wgpu::Buffer,
    textures: TessellationTextures,
}

struct TessellationStageTwo {
    first: TessellationStageOne,
    tess_params_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    camera: CameraResources,
    terrain_size: Vec3,
    texture_layout: wgpu::BindGroupLayout,
    lighting_buffer: wgpu::Buffer,
}

struct TessellationAuxiliary {
    shadow: crate::shadow::ShadowResources,
    blackmap: wgpu::TextureView,
    unexplored: wgpu::TextureView,
    light: wgpu::TextureView,
    local_lights: LocalLightBuffer,
    local_shadow: wgpu::TextureView,
    light_volume_color: wgpu::TextureView,
    light_volume_vector: wgpu::TextureView,
}

fn create_patch_mesh(vertices_per_axis: u32) -> PatchMesh {
    let denominator = (vertices_per_axis - 1)
        .to_f32()
        .expect("patch vertex count must fit f32");
    let vertex_capacity = vertices_per_axis
        .checked_mul(vertices_per_axis)
        .and_then(|count| usize::try_from(count).ok())
        .expect("patch vertex count must fit usize");
    let mut vertices = Vec::with_capacity(vertex_capacity);
    for z in 0..vertices_per_axis {
        for x in 0..vertices_per_axis {
            vertices.push([
                x.to_f32().expect("patch X coordinate must fit f32") / denominator,
                z.to_f32().expect("patch Z coordinate must fit f32") / denominator,
            ]);
        }
    }

    let mut indices = Vec::new();
    for z in 0..vertices_per_axis - 1 {
        for x in 0..vertices_per_axis - 1 {
            let top_left = z * vertices_per_axis + x;
            let top_right = top_left + 1;
            let bottom_left = (z + 1) * vertices_per_axis + x;
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
    PatchMesh { vertices, indices }
}

fn tessellation_factor(level: u8) -> u32 {
    const MAX_BASE_FACTOR: u32 = 8;
    let exponent = 3_u32.saturating_sub(u32::from(level).min(3));
    (1_u32 << exponent).min(MAX_BASE_FACTOR) * 2
}

fn patch_level(levels: &[u8], patches_x: u32, x: u32, z: u32) -> u8 {
    let index = z
        .checked_mul(patches_x)
        .and_then(|row| row.checked_add(x))
        .and_then(|index| usize::try_from(index).ok());
    index
        .and_then(|index| levels.get(index).copied())
        .unwrap_or(0)
}

fn world_patch_level(levels: &[u8], patches_x: u32, world_x: u32, world_z: u32) -> u8 {
    // Match the position/basis resource conversion: viewer (x, z) addresses
    // XTD source (z, x).
    patch_level(levels, patches_x, world_z, world_x)
}

fn create_patch_instances(raw_data: &RawXtdData, patches_x: u32, patches_z: u32) -> Vec<[u32; 8]> {
    let levels = raw_data
        .tessellation
        .as_ref()
        .filter(|tessellation| {
            tessellation.patches_x == patches_x && tessellation.patches_z == patches_z
        })
        .map_or(&[][..], |tessellation| tessellation.levels.as_slice());
    if levels.is_empty() {
        log::warn!("No matching XTD tessellation metadata; using level 0 for every patch");
    }

    let last_x = patches_x - 1;
    let last_z = patches_z - 1;
    let mut instances = Vec::with_capacity(
        patches_x
            .checked_mul(patches_z)
            .and_then(|count| usize::try_from(count).ok())
            .expect("terrain patch count must fit usize"),
    );
    let mut factor_counts = [0_u32; 4];
    for x in 0..patches_x {
        for z in 0..patches_z {
            let center_level = world_patch_level(levels, patches_x, x, z);
            let center = tessellation_factor(center_level);
            let left_x = x.checked_sub(1).unwrap_or(last_x);
            let top_z = z.checked_sub(1).unwrap_or(last_z);
            let left = center.max(tessellation_factor(world_patch_level(
                levels, patches_x, left_x, z,
            )));
            let top = center.max(tessellation_factor(world_patch_level(
                levels, patches_x, x, top_z,
            )));
            let right = center.max(tessellation_factor(world_patch_level(
                levels,
                patches_x,
                (x + 1).min(last_x),
                z,
            )));
            let bottom = center.max(tessellation_factor(world_patch_level(
                levels,
                patches_x,
                x,
                (z + 1).min(last_z),
            )));
            let inside_x = center.max(top.min(bottom));
            let inside_z = center.max(left.min(right));
            let patch_index = x
                .checked_mul(patches_z)
                .and_then(|row| row.checked_add(z))
                .expect("terrain patch index must fit u32");
            instances.push([patch_index, left, top, right, bottom, inside_x, inside_z, 0]);
            factor_counts[usize::from(center_level.min(3))] += 1;
        }
    }
    log::info!(
        "XTD patch tessellation levels: 0={} 1={} 2={} 3={} (factors 16/8/4/2)",
        factor_counts[0],
        factor_counts[1],
        factor_counts[2],
        factor_counts[3]
    );
    instances
}

fn create_instance_buffer(
    device: &wgpu::Device,
    raw_data: &RawXtdData,
    patches_x: u32,
    patches_z: u32,
) -> wgpu::Buffer {
    let instances = create_patch_instances(raw_data, patches_x, patches_z);
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Tess Instance Buffer"),
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn downsample_rgb10a2(values: &[u32], width: u32) -> Vec<u32> {
    let next_width = (width / 2).max(1);
    let next_len = next_width
        .checked_mul(next_width)
        .and_then(|count| usize::try_from(count).ok())
        .expect("packed terrain mip size must fit usize");
    let mut next = Vec::with_capacity(next_len);
    for y in 0..next_width {
        for x in 0..next_width {
            let mut red = 0_u32;
            let mut green = 0_u32;
            let mut blue = 0_u32;
            let mut alpha = 0_u32;
            for offset_y in 0..2 {
                for offset_x in 0..2 {
                    let source_x = (x * 2 + offset_x).min(width - 1);
                    let source_y = (y * 2 + offset_y).min(width - 1);
                    let index = source_y
                        .checked_mul(width)
                        .and_then(|row| row.checked_add(source_x))
                        .and_then(|index| usize::try_from(index).ok())
                        .expect("packed terrain mip index must fit usize");
                    let packed = values[index];
                    red += packed & 0x3ff;
                    green += (packed >> 10) & 0x3ff;
                    blue += (packed >> 20) & 0x3ff;
                    alpha += (packed >> 30) & 0x3;
                }
            }
            let red = (red + 2) / 4;
            let green = (green + 2) / 4;
            let blue = (blue + 2) / 4;
            let alpha = (alpha + 2) / 4;
            next.push(red | (green << 10) | (blue << 20) | (alpha << 30));
        }
    }
    next
}

fn create_rgb10a2_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    values: &[u32],
    with_position_mip: bool,
) -> wgpu::Texture {
    let mip_level_count = if with_position_mip && width > 1 { 2 } else { 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height: width,
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgb10a2Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(values),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(width),
        },
        wgpu::Extent3d {
            width,
            height: width,
            depth_or_array_layers: 1,
        },
    );
    if mip_level_count > 1 {
        let mip = downsample_rgb10a2(values, width);
        let mip_width = width / 2;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 1,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&mip),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mip_width * 4),
                rows_per_image: Some(mip_width),
            },
            wgpu::Extent3d {
                width: mip_width,
                height: mip_width,
                depth_or_array_layers: 1,
            },
        );
    }
    texture
}

fn create_mask_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    mask_name: &str,
    num_verts: u32,
    source: Option<(&[u8], u32, u32)>,
) -> wgpu::TextureView {
    let (width, height, values) = source.map_or_else(
        || {
            let width = num_verts;
            let height = (num_verts / 2).max(1);
            let length = width
                .checked_mul(height)
                .and_then(|count| usize::try_from(count).ok())
                .expect("terrain mask size must fit usize");
            log::warn!("No {mask_name} data available, using fully-lit fallback values");
            (width, height, vec![255; length])
        },
        |(values, width, height)| {
            log::info!(
                "Using half-resolution {mask_name} texture: {width}x{height} ({} bytes)",
                values.len()
            );
            (width, height, values.to_vec())
        },
    );
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &values,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_dynamic_alpha_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    num_verts: u32,
) -> wgpu::TextureView {
    let width = num_verts.div_ceil(32);
    let texel_count = width
        .checked_mul(num_verts)
        .and_then(|count| usize::try_from(count).ok())
        .expect("dynamic alpha texture size must fit usize");
    let words = vec![u32::MAX; texel_count];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Dynamic Terrain Alpha Bitmask"),
        size: wgpu::Extent3d {
            width,
            height: num_verts,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&words),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(num_verts),
        },
        wgpu::Extent3d {
            width,
            height: num_verts,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_gpu_texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let vertex = wgpu::ShaderStages::VERTEX;
    let fragment = wgpu::ShaderStages::FRAGMENT;
    let both = vertex | fragment;
    let filterable = wgpu::TextureSampleType::Float { filterable: true };
    let storage = wgpu::BufferBindingType::Storage { read_only: true };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("GPU Tess Texture Bind Group Layout"),
        entries: &[
            buffer_layout_entry(0, both, wgpu::BufferBindingType::Uniform),
            texture_layout_entry(1, vertex, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(2, both, wgpu::TextureViewDimension::D2, filterable),
            sampler_layout_entry(3, vertex),
            sampler_layout_entry(4, both),
            buffer_layout_entry(5, fragment, wgpu::BufferBindingType::Uniform),
            texture_layout_entry(6, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(7, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(14, fragment, wgpu::TextureViewDimension::D2, filterable),
            buffer_layout_entry(15, both, wgpu::BufferBindingType::Uniform),
            texture_layout_entry(
                16,
                fragment,
                wgpu::TextureViewDimension::D2Array,
                filterable,
            ),
            texture_layout_entry(17, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(18, fragment, wgpu::TextureViewDimension::D2, filterable),
            buffer_layout_entry(19, fragment, storage),
            sampler_layout_entry(20, fragment),
            texture_layout_entry(21, vertex, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(
                22,
                fragment,
                wgpu::TextureViewDimension::D2,
                wgpu::TextureSampleType::Uint,
            ),
            texture_layout_entry(23, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(24, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(
                25,
                fragment,
                wgpu::TextureViewDimension::D2Array,
                filterable,
            ),
            texture_layout_entry(26, fragment, wgpu::TextureViewDimension::D3, filterable),
            texture_layout_entry(27, fragment, wgpu::TextureViewDimension::D3, filterable),
        ],
    })
}

fn create_gpu_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GPU Tessellation Shader"),
        source: wgpu::ShaderSource::Wgsl(GPU_TESS_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("GPU Tess Pipeline Layout"),
        bind_group_layouts: &[camera_layout, texture_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GPU Tessellation Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                },
                wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 0,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32x4,
                            offset: 4,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32x2,
                            offset: 20,
                            shader_location: 3,
                        },
                    ],
                },
            ],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn create_placeholder_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    pixel: [u8; 4],
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
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &pixel,
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_placeholder_array_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    layers: u32,
    pixel: [u8; 4],
) -> wgpu::TextureView {
    let layer_count = usize::try_from(layers).expect("placeholder layer count must fit usize");
    let pixels = pixel.repeat(layer_count);
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &pixels,
    );
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

fn create_placeholder_volume_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    pixel: [u8; 4],
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
        &pixel,
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_light_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: Option<(&[u8], u32, u32)>,
) -> wgpu::TextureView {
    let Some((values, width, height)) = source else {
        return create_placeholder_view(device, queue, "Placeholder Light", [128, 128, 128, 255]);
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Light Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        values,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    log::info!("Created light texture: {width}x{height}");
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

struct ShadowResourceBindings<'a> {
    position: &'a wgpu::TextureView,
    position_sampler: &'a wgpu::Sampler,
    alpha: &'a wgpu::TextureView,
    alpha_sampler: &'a wgpu::Sampler,
    dynamic_alpha: &'a wgpu::TextureView,
    camera_layout: &'a wgpu::BindGroupLayout,
    num_patches: u32,
}

fn create_shadow_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    raw_data: &RawXtdData,
    bindings: &ShadowResourceBindings<'_>,
) -> crate::shadow::ShadowResources {
    let vertex_layouts = [
        wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        },
        wgpu::VertexBufferLayout {
            array_stride: 32,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 0,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32x4,
                    offset: 4,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32x2,
                    offset: 20,
                    shader_location: 3,
                },
            ],
        },
    ];
    let mut shadow =
        crate::shadow::ShadowResources::new(device, bindings.camera_layout, &vertex_layouts);
    let patch_count = bindings
        .num_patches
        .to_f32()
        .expect("tessellation patch count must fit f32");
    shadow.setup_params(
        device,
        queue,
        &crate::shadow::ShadowSetup {
            position_texture_view: bindings.position,
            position_sampler: bindings.position_sampler,
            alpha_texture_view: bindings.alpha,
            alpha_sampler: bindings.alpha_sampler,
            dynamic_alpha_view: bindings.dynamic_alpha,
            terrain_info: [
                raw_data
                    .num_verts_per_axis
                    .to_f32()
                    .expect("terrain vertex count must fit f32"),
                raw_data.tile_scale,
                patch_count,
                patch_count,
            ],
            mid: raw_data.mid,
            range: raw_data.range,
        },
    );
    shadow
}

struct GpuTessBindings<'a> {
    tess_params: &'a wgpu::Buffer,
    position: &'a wgpu::TextureView,
    normal: &'a wgpu::TextureView,
    position_sampler: &'a wgpu::Sampler,
    terrain_sampler: &'a wgpu::Sampler,
    params: &'a wgpu::Buffer,
    ao: &'a wgpu::TextureView,
    alpha: &'a wgpu::TextureView,
    composited_albedo: &'a wgpu::TextureView,
    lighting: &'a wgpu::Buffer,
    shadow: &'a wgpu::TextureView,
    blackmap: &'a wgpu::TextureView,
    unexplored: &'a wgpu::TextureView,
    local_lights: &'a wgpu::Buffer,
    lighting_sampler: &'a wgpu::Sampler,
    light: &'a wgpu::TextureView,
    dynamic_alpha: &'a wgpu::TextureView,
    composited_normal: &'a wgpu::TextureView,
    composited_specular: &'a wgpu::TextureView,
    local_shadow: &'a wgpu::TextureView,
    light_volume_color: &'a wgpu::TextureView,
    light_volume_vector: &'a wgpu::TextureView,
}

fn create_gpu_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    bindings: &GpuTessBindings<'_>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("GPU Tess Texture Bind Group"),
        layout,
        entries: &[
            buffer_entry(0, bindings.tess_params),
            texture_entry(1, bindings.position),
            texture_entry(2, bindings.normal),
            sampler_entry(3, bindings.position_sampler),
            sampler_entry(4, bindings.terrain_sampler),
            buffer_entry(5, bindings.params),
            texture_entry(6, bindings.ao),
            texture_entry(7, bindings.alpha),
            texture_entry(14, bindings.composited_albedo),
            buffer_entry(15, bindings.lighting),
            texture_entry(16, bindings.shadow),
            texture_entry(17, bindings.blackmap),
            texture_entry(18, bindings.unexplored),
            buffer_entry(19, bindings.local_lights),
            sampler_entry(20, bindings.lighting_sampler),
            texture_entry(21, bindings.light),
            texture_entry(22, bindings.dynamic_alpha),
            texture_entry(23, bindings.composited_normal),
            texture_entry(24, bindings.composited_specular),
            texture_entry(25, bindings.local_shadow),
            texture_entry(26, bindings.light_volume_color),
            texture_entry(27, bindings.light_volume_vector),
        ],
    })
}

fn expand_patch_mesh(mesh: &PatchMesh) -> Vec<[f32; 2]> {
    mesh.indices
        .iter()
        .map(|&index| {
            mesh.vertices[usize::try_from(index).expect("patch vertex index must fit usize")]
        })
        .collect()
}

fn create_expanded_patch_buffer(
    device: &wgpu::Device,
    patch_mesh: &PatchMesh,
) -> (wgpu::Buffer, u32, usize) {
    let vertices = expand_patch_mesh(patch_mesh);
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Tess Expanded Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let count =
        u32::try_from(vertices.len()).expect("expanded tessellation vertex count must fit u32");
    (buffer, count, vertices.len())
}

fn log_tessellation_resources(config: TessellationBuildConfig, vertices_per_patch: usize) {
    let triangle_count = (vertices_per_patch / 3)
        .checked_mul(
            usize::try_from(config.total_patches).expect("tessellation patch count must fit usize"),
        )
        .expect("tessellation triangle count must fit usize");
    log::info!(
        "GPU tessellation resources created: {} patches, {vertices_per_patch} vertices per patch, {triangle_count} carrier triangles",
        config.total_patches,
    );
}

impl TerrainViewer {
    fn create_tessellation_textures(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        albedo: Option<&AlbedoData>,
    ) -> TessellationTextures {
        let num_verts = raw_data.num_verts_per_axis;
        let world_positions = xtd_packed_to_world(&raw_data.packed_positions, num_verts);
        let world_normals = xtd_packed_to_world(&raw_data.packed_normals, num_verts);
        let position_texture = create_rgb10a2_texture(
            device,
            queue,
            "Position Texture",
            num_verts,
            &world_positions,
            true,
        );
        let position = position_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let position_for_shadow =
            position_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let normal_texture = create_rgb10a2_texture(
            device,
            queue,
            "Normal Texture",
            num_verts,
            &world_normals,
            false,
        );
        let normal = normal_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let ao = create_mask_texture(
            device,
            queue,
            "AO Texture (Half Resolution)",
            "AO",
            num_verts,
            raw_data
                .ao_data
                .as_ref()
                .map(|data| (data.values.as_slice(), data.width, data.height)),
        );
        let alpha = create_mask_texture(
            device,
            queue,
            "Alpha Texture (Half Resolution)",
            "Alpha",
            num_verts,
            raw_data
                .alpha_data
                .as_ref()
                .map(|data| (data.values.as_slice(), data.width, data.height)),
        );
        let (_, normal_map_array) = self.create_normal_map_array(device, queue);
        let (_, specular_map_array) = self.create_specular_map_array(device, queue);
        let (_, terrain_array) = self.create_terrain_texture_array(device, queue, albedo);
        let (_, alpha_atlas) = self.create_alpha_atlas(device, queue);
        let (_, alpha_atlas_hi) = self.create_alpha_atlas_hi(device, queue);
        let dynamic_alpha = create_dynamic_alpha_texture(device, queue, num_verts);
        TessellationTextures {
            position,
            position_for_shadow,
            normal,
            ao,
            alpha,
            normal_map_array,
            specular_map_array,
            terrain_array,
            alpha_atlas,
            alpha_atlas_hi,
            dynamic_alpha,
            chunk_layers: self.create_chunk_layers_buffer(device),
            samplers: create_terrain_samplers(device),
        }
    }

    /// Create GPU resources for GPU tessellation mode.
    /// Uses instanced patch rendering with vertex shader displacement.
    pub(crate) fn create_gpu_tessellation_resources(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        albedo: Option<&AlbedoData>,
        surface_size: [u32; 2],
    ) {
        const CELLS_PER_PATCH: u32 = 16;
        const VERTICES_PER_PATCH: u32 = CELLS_PER_PATCH + 1;
        assert!(
            raw_data.num_verts_per_axis > 0,
            "terrain must contain at least one packed sample",
        );
        // The oracle treats the packed texture width as the logical cell count.
        // A 1024-wide terrain is therefore 64 patches of 16 cells.  The 1025th
        // edge vertex samples UV 1.0, which clamps back to packed texel 1023.
        let patches_per_axis = raw_data.num_verts_per_axis.div_ceil(CELLS_PER_PATCH);
        log::info!(
            "Creating GPU tessellation resources: {patches_per_axis}x{patches_per_axis} patches, {VERTICES_PER_PATCH}x{VERTICES_PER_PATCH} verts per patch"
        );
        let total_patches = patches_per_axis
            .checked_mul(patches_per_axis)
            .expect("tessellation patch count must fit u32");
        let config = TessellationBuildConfig {
            surface_size,
            num_patches: patches_per_axis,
            total_patches,
        };
        let first = TessellationStageOne {
            patch_mesh: create_patch_mesh(VERTICES_PER_PATCH),
            instance_buffer: create_instance_buffer(
                device,
                raw_data,
                patches_per_axis,
                patches_per_axis,
            ),
            textures: self.create_tessellation_textures(device, queue, raw_data, albedo),
        };
        self.create_gpu_tessellation_resources_part2(device, queue, raw_data, config, first);
    }

    fn create_gpu_tessellation_resources_part2(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        config: TessellationBuildConfig,
        first: TessellationStageOne,
    ) {
        let patch_count = config
            .num_patches
            .to_f32()
            .expect("tessellation patch count must fit f32");
        let tess_params = GpuTessParams {
            mid: [
                raw_data.mid[2],
                raw_data.mid[1],
                raw_data.mid[0],
                NORMALIZED_TERRAIN_Y_OFFSET,
            ],
            range: [raw_data.range[2], raw_data.range[1], raw_data.range[0], 0.0],
            terrain_info: [
                raw_data
                    .num_verts_per_axis
                    .to_f32()
                    .expect("terrain vertex count must fit f32"),
                raw_data.tile_scale,
                patch_count,
                patch_count,
            ],
            world_min: [
                raw_data.world_min[2],
                raw_data.world_min[1],
                raw_data.world_min[0],
                0.0,
            ],
            world_max: [
                raw_data.world_max[2],
                raw_data.world_max[1],
                raw_data.world_max[0],
                0.0,
            ],
        };
        let tess_params_buffer = create_uniform_buffer(device, "Tess Params Buffer", &tess_params);
        let terrain_size = self
            .scene
            .as_ref()
            .map_or(Vec3::new(1024.0, 100.0, 1024.0), |scene| scene.mesh.size());
        let params = TerrainParams {
            terrain_size: [terrain_size.x, terrain_size.z],
            chunk_count: [16.0, 16.0],
            texture_tile_scale: 32.0,
            debug_mode: self.debug_mode.to_f32().expect("debug mode must fit f32"),
            bump_power: self.bump_power,
            padding: 0.0,
        };
        let params_buffer = create_uniform_buffer(device, "Terrain Params Buffer", &params);
        let camera = create_camera_resources(device);
        let texture_layout = create_gpu_texture_layout(device);
        let texture_scales = self.create_texture_scales_buffer(device);
        self.init_compositor(
            device,
            queue,
            &CompositorInputs {
                terrain_array: &first.textures.terrain_array,
                normal_array: &first.textures.normal_map_array,
                specular_array: &first.textures.specular_map_array,
                alpha_atlas: &first.textures.alpha_atlas,
                alpha_atlas_hi: &first.textures.alpha_atlas_hi,
                chunk_layers: &first.textures.chunk_layers,
                texture_scales: &texture_scales,
                terrain_sampler: &first.textures.samplers.terrain,
                alpha_sampler: &first.textures.samplers.alpha,
            },
        );
        self.calculate_chunk_centers();
        let lighting_buffer =
            create_uniform_buffer(device, "Lighting Params Buffer", &LightingParams::default());
        let second = TessellationStageTwo {
            first,
            tess_params_buffer,
            params_buffer,
            camera,
            terrain_size,
            texture_layout,
            lighting_buffer,
        };
        self.create_gpu_tessellation_resources_part3(device, queue, raw_data, config, second);
    }

    fn create_tessellation_auxiliary(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        shadow_bindings: &ShadowResourceBindings<'_>,
    ) -> TessellationAuxiliary {
        let shadow = create_shadow_resources(device, queue, raw_data, shadow_bindings);
        log::info!("Shadow resources initialized");
        let blackmap = create_placeholder_view(device, queue, "Placeholder Blackmap", [0, 0, 0, 0]);
        let unexplored =
            create_placeholder_view(device, queue, "Placeholder Unexplored", [0, 0, 0, 0]);
        let lighting_source = self
            .scene
            .as_ref()
            .and_then(|scene| scene.lighting_data.as_ref())
            .map(|data| (data.values.as_slice(), data.width, data.height));
        TessellationAuxiliary {
            shadow,
            blackmap,
            unexplored,
            light: create_light_view(device, queue, lighting_source),
            local_lights: LocalLightBuffer::empty(device),
            local_shadow: create_placeholder_array_view(
                device,
                queue,
                "Placeholder Local Shadow Map",
                5,
                [255; 4],
            ),
            light_volume_color: create_placeholder_volume_view(
                device,
                queue,
                "Placeholder Light Volume Color",
                [0, 0, 0, 0],
            ),
            light_volume_vector: create_placeholder_volume_view(
                device,
                queue,
                "Placeholder Light Volume Vector",
                [128, 128, 128, 255],
            ),
        }
    }

    fn create_gpu_tessellation_resources_part3(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        config: TessellationBuildConfig,
        second: TessellationStageTwo,
    ) {
        let TessellationStageTwo {
            first,
            tess_params_buffer,
            params_buffer,
            camera,
            terrain_size,
            texture_layout,
            lighting_buffer,
        } = second;
        let TessellationStageOne {
            patch_mesh,
            instance_buffer,
            textures,
        } = first;
        let auxiliary = self.create_tessellation_auxiliary(
            device,
            queue,
            raw_data,
            &ShadowResourceBindings {
                position: &textures.position_for_shadow,
                position_sampler: &textures.samplers.position,
                alpha: &textures.alpha,
                alpha_sampler: &textures.samplers.alpha,
                dynamic_alpha: &textures.dynamic_alpha,
                camera_layout: &camera.layout,
                num_patches: config.num_patches,
            },
        );
        let compositor = self
            .compositor
            .as_ref()
            .expect("compositor was initialized in tessellation stage two");
        let composited_albedo = compositor.albedo_atlas_view();
        let composited_normal = compositor.normal_atlas_view();
        let composited_specular = compositor.specular_atlas_view();
        let texture_bind_group = create_gpu_texture_bind_group(
            device,
            &texture_layout,
            &GpuTessBindings {
                tess_params: &tess_params_buffer,
                position: &textures.position,
                normal: &textures.normal,
                position_sampler: &textures.samplers.position,
                terrain_sampler: &textures.samplers.terrain,
                params: &params_buffer,
                ao: &textures.ao,
                alpha: &textures.alpha,
                composited_albedo,
                lighting: &lighting_buffer,
                shadow: &auxiliary.shadow.shadow_view,
                blackmap: &auxiliary.blackmap,
                unexplored: &auxiliary.unexplored,
                local_lights: auxiliary.local_lights.buffer(),
                lighting_sampler: &textures.samplers.lighting,
                light: &auxiliary.light,
                dynamic_alpha: &textures.dynamic_alpha,
                composited_normal,
                composited_specular,
                local_shadow: &auxiliary.local_shadow,
                light_volume_color: &auxiliary.light_volume_color,
                light_volume_vector: &auxiliary.light_volume_vector,
            },
        );
        let pipeline =
            create_gpu_pipeline(device, self.scene_format, &camera.layout, &texture_layout);
        let [width, height] = config.surface_size;
        let (depth_texture, depth_view) = create_depth_texture(device, width, height);
        let (expanded_vertex_buffer, vertex_count, expanded_vertex_count) =
            create_expanded_patch_buffer(device, &patch_mesh);
        self.shadow_resources = Some(auxiliary.shadow);
        self.gpu = Some(GpuResources {
            pipeline,
            vertex_buffer: expanded_vertex_buffer,
            index_buffer: instance_buffer,
            index_count: vertex_count,
            camera_buffer: camera.buffer,
            camera_bind_group_layout: camera.layout,
            camera_bind_group: camera.bind_group,
            texture_bind_group,
            position_texture_view: textures.position.clone(),
            depth_texture,
            depth_view,
            params_buffer,
            lighting_buffer: Some(lighting_buffer),
            local_lights: auxiliary.local_lights.clone(),
            terrain_size: [terrain_size.x, terrain_size.z],
            tile_scale: raw_data.tile_scale,
            num_patch_instances: config.total_patches,
        });
        log_tessellation_resources(config, expanded_vertex_count);
        self.init_tessellation_surface_features(
            device,
            queue,
            &auxiliary.blackmap,
            &auxiliary.unexplored,
            auxiliary.local_lights.buffer(),
        );
    }

    fn init_tessellation_surface_features(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        blackmap: &wgpu::TextureView,
        unexplored: &wgpu::TextureView,
        local_lights: &wgpu::Buffer,
    ) {
        let foliage_shadow = self
            .shadow_resources
            .as_ref()
            .expect("shadow resources were stored above")
            .shadow_view
            .clone();
        let foliage_world = crate::foliage::FoliageWorldBindings {
            shadow: &foliage_shadow,
            blackmap,
            unexplored,
            local_lights,
        };
        self.init_foliage_resources(device, queue, Some(&foliage_world));
        self.init_road_resources(device, queue);
    }
}
