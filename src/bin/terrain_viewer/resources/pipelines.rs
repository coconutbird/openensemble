//! Pipeline and bind group creation for terrain rendering.
//!
//! Contains the main GPU resource initialization functions that create
//! render pipelines, bind group layouts, and wire everything together.

use num_traits::ToPrimitive;
use render::gpu::{
    buffer_entry, buffer_layout_entry, filtering_sampler_layout_entry as sampler_layout_entry,
    sampler_entry, texture_entry, texture_layout_entry,
};
use render::terrain::{CompositeBindings, CompositingConfig, CompositorResources};
use render::wgpu;
use wgpu::util::DeviceExt;

use crate::types::TerrainChunkGrid;
use crate::viewer::TerrainViewer;

mod tessellation;

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
        chunk_grid: TerrainChunkGrid,
    ) {
        let config = CompositingConfig::for_chunk_grid(chunk_grid.width(), chunk_grid.height())
            .expect("decoded terrain chunk grid must produce a valid compositor atlas");
        log::info!(
            "Initializing GPU compositor: {}×{} atlas ({} chunks)",
            config.atlas_width,
            config.atlas_height,
            config.total_chunks()
        );
        let compositor = CompositorResources::new(device, config);
        let (_, decal_alpha_view) = self.create_decal_alpha_atlas(device, queue, chunk_grid);
        let (_, decal_alpha_hi_view) = self.create_decal_alpha_atlas_hi(device, queue, chunk_grid);
        let (decal_diffuse_view, decal_opacity_view) =
            self.create_decal_texture_arrays(device, queue);
        let decal_layers = self.create_chunk_decal_layers_buffer(device, chunk_grid);
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

    /// Calculates map-sized chunk center positions from the terrain bounds.
    pub(crate) fn calculate_chunk_centers(&mut self, chunk_grid: TerrainChunkGrid) {
        let Some(scene) = &self.scene else {
            return;
        };
        let terrain = &scene.mesh;

        let world_min = terrain.world_min;
        let world_max = terrain.world_max;
        let column_count = chunk_grid.width();
        let row_count = chunk_grid.height();
        let [column_divisor, row_divisor] = chunk_grid.dimensions_f32();

        let chunk_width = (world_max[0] - world_min[0]) / column_divisor;
        let chunk_depth = (world_max[2] - world_min[2]) / row_divisor;
        let chunk_height = (world_max[1] - world_min[1]) / 2.0; // Average Y for center

        self.chunk_centers.clear();
        for cz in 0..row_count {
            for cx in 0..column_count {
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

        let shadow_camera_bind_groups = self
            .shadow_resources
            .as_ref()
            .map(|shadow| {
                (0..shadow.cascade_count())
                    .map(|cascade| shadow.cascade_camera_bind_group(cascade).clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut foliage_resources = crate::foliage::FoliageResources::new(
            device,
            self.scene_format,
            &gpu.camera_bind_group_layout,
            &gpu.camera_bind_group,
            &shadow_camera_bind_groups,
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
                camera_bind_group: &gpu.camera_bind_group,
                surface_format: self.scene_format,
                raw_terrain,
                shadow_view: shadow_view.as_ref(),
                batches: &batch_inputs,
                bump_power: self.bump_power,
            },
        ));
    }
}
