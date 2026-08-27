use glam::Vec3;
use num_traits::ToPrimitive;
use render::lighting::LocalLightBuffer;
use render::terrain::{GpuTessParams, LightingParams, NORMALIZED_TERRAIN_Y_OFFSET, TerrainParams};
use render::wgpu;
use wgpu::util::DeviceExt;

use super::{
    CameraResources, CompositorInputs, TerrainSamplers, create_camera_resources,
    create_terrain_samplers, create_uniform_buffer,
};
use crate::gpu::{create_depth_texture, xtd_packed_to_world};
use crate::types::{AlbedoData, GpuResources, RawXtdData, TerrainChunkGrid};
use crate::viewer::TerrainViewer;

mod gpu;

use gpu::{
    GpuTessBindings, ShadowResourceBindings, create_dynamic_alpha_texture,
    create_expanded_patch_buffer, create_gpu_pipeline, create_gpu_texture_bind_group,
    create_gpu_texture_layout, create_light_view, create_mask_texture,
    create_placeholder_array_view, create_placeholder_view, create_placeholder_volume_view,
    create_rgb10a2_texture, create_shadow_resources, log_tessellation_resources,
};

#[cfg(test)]
mod tests {
    use super::gpu::downsample_rgb10a2;
    use super::{create_patch_instances, tessellation_factor, world_patch_level};
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
    chunk_grid: TerrainChunkGrid,
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

impl TerrainViewer {
    fn create_tessellation_textures(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        albedo: Option<&AlbedoData>,
        chunk_grid: TerrainChunkGrid,
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
        let (_, alpha_atlas) = self.create_alpha_atlas(device, queue, chunk_grid);
        let (_, alpha_atlas_hi) = self.create_alpha_atlas_hi(device, queue, chunk_grid);
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
            chunk_layers: self.create_chunk_layers_buffer(device, chunk_grid),
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
        let chunk_grid = TerrainChunkGrid::from_terrain_dimension(raw_data.num_verts_per_axis)
            .expect("terrain dimension must be a non-zero multiple of 64 cells");
        if let Some(scene) = &self.scene
            && scene.chunk_splat_data.len() != chunk_grid.total_chunks()
        {
            log::warn!(
                "Decoded {} XTT chunks for a {}×{} XTD terrain grid ({} expected)",
                scene.chunk_splat_data.len(),
                chunk_grid.width(),
                chunk_grid.height(),
                chunk_grid.total_chunks(),
            );
        }
        let config = TessellationBuildConfig {
            surface_size,
            num_patches: patches_per_axis,
            total_patches,
            chunk_grid,
        };
        let first = TessellationStageOne {
            patch_mesh: create_patch_mesh(VERTICES_PER_PATCH),
            instance_buffer: create_instance_buffer(
                device,
                raw_data,
                patches_per_axis,
                patches_per_axis,
            ),
            textures: self
                .create_tessellation_textures(device, queue, raw_data, albedo, chunk_grid),
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
            chunk_count: config.chunk_grid.dimensions_f32(),
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
            config.chunk_grid,
        );
        self.calculate_chunk_centers(config.chunk_grid);
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
        let textures = &second.first.textures;
        let mut auxiliary = self.create_tessellation_auxiliary(
            device,
            queue,
            raw_data,
            &ShadowResourceBindings {
                position: &textures.position_for_shadow,
                position_sampler: &textures.samplers.position,
                alpha: &textures.alpha,
                alpha_sampler: &textures.samplers.alpha,
                dynamic_alpha: &textures.dynamic_alpha,
                camera_layout: &second.camera.layout,
                num_patches: config.num_patches,
            },
        );
        let texture_bind_group =
            self.create_tessellation_texture_bind_group(device, &second, &auxiliary);
        let TessellationStageTwo {
            first,
            tess_params_buffer: _,
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
        let pipeline =
            create_gpu_pipeline(device, self.scene_format, &camera.layout, &texture_layout);
        let [width, height] = config.surface_size;
        let (depth_texture, depth_view) = create_depth_texture(device, width, height);
        let (expanded_vertex_buffer, vertex_count, expanded_vertex_count) =
            create_expanded_patch_buffer(device, &patch_mesh);
        let (sv, si) = (&expanded_vertex_buffer, &instance_buffer);
        let shadow = &mut auxiliary.shadow;
        shadow.set_terrain_geometry((sv, si, vertex_count, config.total_patches));
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
            chunk_grid: config.chunk_grid,
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

    fn create_tessellation_texture_bind_group(
        &self,
        device: &wgpu::Device,
        second: &TessellationStageTwo,
        auxiliary: &TessellationAuxiliary,
    ) -> wgpu::BindGroup {
        let textures = &second.first.textures;
        let compositor = self
            .compositor
            .as_ref()
            .expect("compositor was initialized in tessellation stage two");
        let composited_albedo = compositor.albedo_atlas_view();
        let composited_normal = compositor.normal_atlas_view();
        let composited_specular = compositor.specular_atlas_view();
        create_gpu_texture_bind_group(
            device,
            &second.texture_layout,
            &GpuTessBindings {
                tess_params: &second.tess_params_buffer,
                position: &textures.position,
                normal: &textures.normal,
                position_sampler: &textures.samplers.position,
                terrain_sampler: &textures.samplers.terrain,
                params: &second.params_buffer,
                ao: &textures.ao,
                alpha: &textures.alpha,
                composited_albedo,
                lighting: &second.lighting_buffer,
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
        )
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
