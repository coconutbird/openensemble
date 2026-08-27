//! Pipeline and bind group creation for terrain rendering.
//!
//! Contains the main GPU resource initialization functions that create
//! render pipelines, bind group layouts, and wire everything together.

use glam::Vec3;
use num_traits::ToPrimitive;
use render::terrain::{
    CompositingConfig, CompositorResources, GPU_TESS_SHADER, GpuTessParams, LightingParams,
    TERRAIN_SHADER, TerrainParams,
};
use render::wgpu;
use wgpu::util::DeviceExt;

use crate::gpu::create_depth_texture;
use crate::types::{AlbedoData, CpuTerrainData, GpuResources, RawXtdData};
use crate::viewer::TerrainViewer;

struct CameraResources {
    buffer: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

struct TerrainSamplers {
    terrain: wgpu::Sampler,
    alpha: wgpu::Sampler,
}

struct CompositorInputs<'a> {
    terrain_array: &'a wgpu::TextureView,
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
    TerrainSamplers { terrain, alpha }
}

fn create_cpu_texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let fragment = wgpu::ShaderStages::FRAGMENT;
    let filterable = wgpu::TextureSampleType::Float { filterable: true };
    let storage = wgpu::BufferBindingType::Storage { read_only: true };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Texture Bind Group Layout"),
        entries: &[
            texture_layout_entry(0, fragment, wgpu::TextureViewDimension::D2Array, filterable),
            sampler_layout_entry(1, fragment),
            texture_layout_entry(2, fragment, wgpu::TextureViewDimension::D2Array, filterable),
            buffer_layout_entry(3, fragment, storage),
            buffer_layout_entry(4, fragment, wgpu::BufferBindingType::Uniform),
            sampler_layout_entry(5, fragment),
            texture_layout_entry(6, fragment, wgpu::TextureViewDimension::D2, filterable),
            buffer_layout_entry(7, fragment, storage),
        ],
    })
}

struct CpuTextureBindings<'a> {
    terrain_array: &'a wgpu::TextureView,
    terrain_sampler: &'a wgpu::Sampler,
    alpha_atlas: &'a wgpu::TextureView,
    chunk_layers: &'a wgpu::Buffer,
    params: &'a wgpu::Buffer,
    alpha_sampler: &'a wgpu::Sampler,
    xtt_albedo: &'a wgpu::TextureView,
    texture_scales: &'a wgpu::Buffer,
}

struct CpuTerrainTextures {
    terrain_array: wgpu::TextureView,
    alpha_atlas: wgpu::TextureView,
    alpha_atlas_hi: wgpu::TextureView,
    xtt_albedo: wgpu::TextureView,
    chunk_layers: wgpu::Buffer,
    texture_scales: wgpu::Buffer,
    samplers: TerrainSamplers,
}

fn create_cpu_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    bindings: &CpuTextureBindings<'_>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Texture Bind Group"),
        layout,
        entries: &[
            texture_entry(0, bindings.terrain_array),
            sampler_entry(1, bindings.terrain_sampler),
            texture_entry(2, bindings.alpha_atlas),
            buffer_entry(3, bindings.chunk_layers),
            buffer_entry(4, bindings.params),
            sampler_entry(5, bindings.alpha_sampler),
            texture_entry(6, bindings.xtt_albedo),
            buffer_entry(7, bindings.texture_scales),
        ],
    })
}

fn create_cpu_geometry_buffers(
    device: &wgpu::Device,
    data: &CpuTerrainData<'_>,
) -> (wgpu::Buffer, wgpu::Buffer) {
    assert_eq!(data.positions.len(), data.normals.len());
    assert_eq!(data.positions.len(), data.uvs.len());
    let mut vertices = Vec::with_capacity(data.positions.len() * 8);
    for ((position, normal), uv) in data.positions.iter().zip(data.normals).zip(data.uvs) {
        vertices.extend_from_slice(position);
        vertices.extend_from_slice(normal);
        vertices.extend_from_slice(uv);
    }
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Terrain Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Terrain Index Buffer"),
        contents: bytemuck::cast_slice(data.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    (vertex_buffer, index_buffer)
}

fn create_cpu_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Terrain Shader"),
        source: wgpu::ShaderSource::Wgsl(TERRAIN_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Terrain Pipeline Layout"),
        bind_group_layouts: &[camera_layout, texture_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Terrain Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 12,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 24,
                        shader_location: 2,
                    },
                ],
            }],
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
        let decal_layers = self.create_chunk_decal_layers_buffer(device);
        let decal_instances = self.create_decal_instances_buffer(device);
        let decal_scales = self.create_decal_uv_scales_buffer(device);
        let bind_group = compositor.create_bind_group(
            device,
            inputs.terrain_array,
            inputs.alpha_atlas,
            inputs.alpha_atlas_hi,
            inputs.chunk_layers,
            inputs.texture_scales,
            inputs.terrain_sampler,
            inputs.alpha_sampler,
            &decal_alpha_view,
            &decal_layers,
            &decal_instances,
            &decal_scales,
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
    pub(crate) fn init_foliage_resources(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
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
            self.surface_format,
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
                foliage_resources.create_params_bind_group(device, queue, raw_data);
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

        let road = &scene.road_chunks[0];
        if road.positions.is_empty() {
            log::warn!("Road '{}' has no vertices", road.texture_name);
            return;
        }

        // Load road textures (take source temporarily since we need &mut)
        let Some(mut source) = self.asset_source.take() else {
            log::warn!("Cannot load road textures: no asset source");
            return;
        };

        let textures = render::terrain::load_road_textures(&mut source, &road.texture_name);
        self.asset_source = Some(source);
        let Some(textures) = textures else {
            log::warn!("Failed to load road textures for '{}'", road.texture_name);
            return;
        };

        let camera_bgl = &gpu.camera_bind_group_layout;
        self.road_resources = Some(crate::roads::create_road_resources(
            device,
            queue,
            &crate::roads::RoadResourceInput {
                camera_bind_group_layout: camera_bgl,
                surface_format: self.surface_format,
                positions: &road.positions,
                uvs: &road.uvs,
                albedo_pixels: &textures.albedo_pixels,
                texture_size: [textures.width, textures.height],
            },
        ));
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
    xtt_albedo: wgpu::TextureView,
    normal_map_array: wgpu::TextureView,
    terrain_array: wgpu::TextureView,
    alpha_atlas: wgpu::TextureView,
    alpha_atlas_hi: wgpu::TextureView,
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
    texture_scales: wgpu::Buffer,
    lighting_buffer: wgpu::Buffer,
}

struct TessellationAuxiliary {
    shadow: crate::shadow::ShadowResources,
    blackmap: wgpu::TextureView,
    unexplored: wgpu::TextureView,
    light: wgpu::TextureView,
    local_lights: wgpu::Buffer,
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

fn create_instance_buffer(device: &wgpu::Device, total_patches: u32) -> wgpu::Buffer {
    let instances: Vec<u32> = (0..total_patches).collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Tess Instance Buffer"),
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn create_uint_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    values: &[u32],
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height: width,
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
            texture_layout_entry(
                1,
                vertex,
                wgpu::TextureViewDimension::D2,
                wgpu::TextureSampleType::Uint,
            ),
            texture_layout_entry(
                2,
                both,
                wgpu::TextureViewDimension::D2,
                wgpu::TextureSampleType::Uint,
            ),
            texture_layout_entry(3, fragment, wgpu::TextureViewDimension::D2, filterable),
            sampler_layout_entry(4, both),
            buffer_layout_entry(5, fragment, wgpu::BufferBindingType::Uniform),
            texture_layout_entry(6, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(7, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(8, fragment, wgpu::TextureViewDimension::D2Array, filterable),
            texture_layout_entry(9, fragment, wgpu::TextureViewDimension::D2Array, filterable),
            texture_layout_entry(
                10,
                fragment,
                wgpu::TextureViewDimension::D2Array,
                filterable,
            ),
            buffer_layout_entry(11, fragment, storage),
            sampler_layout_entry(12, fragment),
            buffer_layout_entry(13, fragment, storage),
            texture_layout_entry(14, fragment, wgpu::TextureViewDimension::D2, filterable),
            buffer_layout_entry(15, both, wgpu::BufferBindingType::Uniform),
            texture_layout_entry(16, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(17, fragment, wgpu::TextureViewDimension::D2, filterable),
            texture_layout_entry(18, fragment, wgpu::TextureViewDimension::D2, filterable),
            buffer_layout_entry(19, fragment, storage),
            texture_layout_entry(
                20,
                fragment,
                wgpu::TextureViewDimension::D2Array,
                filterable,
            ),
            texture_layout_entry(21, vertex, wgpu::TextureViewDimension::D2, filterable),
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
                    array_stride: 4,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Uint32,
                        offset: 0,
                        shader_location: 1,
                    }],
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

fn create_local_lights_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Local Lights Buffer"),
        contents: &[0; 64],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}

fn create_shadow_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    raw_data: &RawXtdData,
    position_view: &wgpu::TextureView,
    camera_layout: &wgpu::BindGroupLayout,
    num_patches: u32,
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
            array_stride: 4,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 0,
                shader_location: 1,
            }],
        },
    ];
    let mut shadow = crate::shadow::ShadowResources::new(device, camera_layout, &vertex_layouts);
    let patch_count = num_patches
        .to_f32()
        .expect("tessellation patch count must fit f32");
    shadow.setup_params(
        device,
        queue,
        position_view,
        [
            raw_data
                .num_verts_per_axis
                .to_f32()
                .expect("terrain vertex count must fit f32"),
            raw_data.tile_scale,
            patch_count,
            patch_count,
        ],
        raw_data.mid,
        raw_data.range,
    );
    shadow
}

struct GpuTessBindings<'a> {
    tess_params: &'a wgpu::Buffer,
    position: &'a wgpu::TextureView,
    normal: &'a wgpu::TextureView,
    xtt_albedo: &'a wgpu::TextureView,
    terrain_sampler: &'a wgpu::Sampler,
    params: &'a wgpu::Buffer,
    ao: &'a wgpu::TextureView,
    alpha: &'a wgpu::TextureView,
    normal_map_array: &'a wgpu::TextureView,
    terrain_array: &'a wgpu::TextureView,
    alpha_atlas: &'a wgpu::TextureView,
    chunk_layers: &'a wgpu::Buffer,
    alpha_sampler: &'a wgpu::Sampler,
    texture_scales: &'a wgpu::Buffer,
    composited_albedo: &'a wgpu::TextureView,
    lighting: &'a wgpu::Buffer,
    shadow: &'a wgpu::TextureView,
    blackmap: &'a wgpu::TextureView,
    unexplored: &'a wgpu::TextureView,
    local_lights: &'a wgpu::Buffer,
    alpha_atlas_hi: &'a wgpu::TextureView,
    light: &'a wgpu::TextureView,
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
            texture_entry(3, bindings.xtt_albedo),
            sampler_entry(4, bindings.terrain_sampler),
            buffer_entry(5, bindings.params),
            texture_entry(6, bindings.ao),
            texture_entry(7, bindings.alpha),
            texture_entry(8, bindings.normal_map_array),
            texture_entry(9, bindings.terrain_array),
            texture_entry(10, bindings.alpha_atlas),
            buffer_entry(11, bindings.chunk_layers),
            sampler_entry(12, bindings.alpha_sampler),
            buffer_entry(13, bindings.texture_scales),
            texture_entry(14, bindings.composited_albedo),
            buffer_entry(15, bindings.lighting),
            texture_entry(16, bindings.shadow),
            texture_entry(17, bindings.blackmap),
            texture_entry(18, bindings.unexplored),
            buffer_entry(19, bindings.local_lights),
            texture_entry(20, bindings.alpha_atlas_hi),
            texture_entry(21, bindings.light),
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

impl TerrainViewer {
    fn create_cpu_terrain_textures(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        albedo: Option<&AlbedoData>,
    ) -> CpuTerrainTextures {
        let (_, terrain_array) = self.create_terrain_texture_array(device, queue, albedo);
        let (_, alpha_atlas) = self.create_alpha_atlas(device, queue);
        let (_, alpha_atlas_hi) = self.create_alpha_atlas_hi(device, queue);
        let (_, xtt_albedo) = Self::create_xtt_albedo_texture(device, queue, albedo);
        CpuTerrainTextures {
            terrain_array,
            alpha_atlas,
            alpha_atlas_hi,
            xtt_albedo,
            chunk_layers: self.create_chunk_layers_buffer(device),
            texture_scales: self.create_texture_scales_buffer(device),
            samplers: create_terrain_samplers(device),
        }
    }

    fn terrain_metrics(&self) -> (Vec3, f32) {
        self.scene
            .as_ref()
            .map_or((Vec3::new(1024.0, 100.0, 1024.0), 1.0), |scene| {
                (scene.mesh.size(), scene.mesh.tile_scale)
            })
    }

    /// Create GPU resources for regular terrain rendering (CPU tessellation mode).
    pub(crate) fn create_gpu_resources_from_data(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &CpuTerrainData<'_>,
    ) {
        let (vertex_buffer, index_buffer) = create_cpu_geometry_buffers(device, data);
        let camera = create_camera_resources(device);
        let textures = self.create_cpu_terrain_textures(device, queue, data.albedo);
        let (terrain_size, tile_scale) = self.terrain_metrics();
        let params = TerrainParams {
            terrain_size: [terrain_size.x, terrain_size.z],
            chunk_count: [16.0, 16.0],
            texture_tile_scale: tile_scale,
            debug_mode: self.debug_mode.to_f32().expect("debug mode must fit f32"),
            bump_power: self.bump_power,
            padding: 0.0,
        };
        let params_buffer = create_uniform_buffer(device, "Terrain Params Buffer", &params);
        let texture_layout = create_cpu_texture_layout(device);
        self.init_compositor(
            device,
            queue,
            &CompositorInputs {
                terrain_array: &textures.terrain_array,
                alpha_atlas: &textures.alpha_atlas,
                alpha_atlas_hi: &textures.alpha_atlas_hi,
                chunk_layers: &textures.chunk_layers,
                texture_scales: &textures.texture_scales,
                terrain_sampler: &textures.samplers.terrain,
                alpha_sampler: &textures.samplers.alpha,
            },
        );
        self.calculate_chunk_centers();
        let texture_bind_group = create_cpu_texture_bind_group(
            device,
            &texture_layout,
            &CpuTextureBindings {
                terrain_array: &textures.terrain_array,
                terrain_sampler: &textures.samplers.terrain,
                alpha_atlas: &textures.alpha_atlas,
                chunk_layers: &textures.chunk_layers,
                params: &params_buffer,
                alpha_sampler: &textures.samplers.alpha,
                xtt_albedo: &textures.xtt_albedo,
                texture_scales: &textures.texture_scales,
            },
        );
        let pipeline =
            create_cpu_pipeline(device, self.surface_format, &camera.layout, &texture_layout);
        let [width, height] = data.surface_size;
        let (depth_texture, depth_view) = create_depth_texture(device, width, height);
        let index_count =
            u32::try_from(data.indices.len()).expect("terrain index count must fit u32");
        self.gpu = Some(GpuResources {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count,
            camera_buffer: camera.buffer,
            camera_bind_group_layout: camera.layout,
            camera_bind_group: camera.bind_group,
            texture_bind_group,
            depth_texture,
            depth_view,
            params_buffer,
            lighting_buffer: None,
            terrain_size: [terrain_size.x, terrain_size.z],
            tile_scale,
            use_gpu_tessellation: false,
            num_patch_instances: 0,
        });
        log::info!(
            "GPU resources created: {} vertices, {} indices, {} terrain textures",
            data.positions.len(),
            data.indices.len(),
            self.scene
                .as_ref()
                .map_or(0, |scene| scene.terrain_textures.len())
        );
        self.init_foliage_resources(device, queue);
        self.init_road_resources(device, queue);
    }
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
        let position_texture = create_uint_texture(
            device,
            queue,
            "Position Texture",
            num_verts,
            &raw_data.packed_positions,
        );
        let position = position_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let position_for_shadow =
            position_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let normal_texture = create_uint_texture(
            device,
            queue,
            "Normal Texture",
            num_verts,
            &raw_data.packed_normals,
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
        let (_, xtt_albedo) = Self::create_xtt_albedo_texture(device, queue, albedo);
        let (_, normal_map_array) = self.create_normal_map_array(device, queue);
        let (_, terrain_array) = self.create_terrain_texture_array(device, queue, albedo);
        let (_, alpha_atlas) = self.create_alpha_atlas(device, queue);
        let (_, alpha_atlas_hi) = self.create_alpha_atlas_hi(device, queue);
        TessellationTextures {
            position,
            position_for_shadow,
            normal,
            ao,
            alpha,
            xtt_albedo,
            normal_map_array,
            terrain_array,
            alpha_atlas,
            alpha_atlas_hi,
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
        const PATCHES_PER_AXIS: u32 = 64;
        const VERTICES_PER_PATCH: u32 = 16;
        log::info!(
            "Creating GPU tessellation resources: {PATCHES_PER_AXIS}x{PATCHES_PER_AXIS} patches, {VERTICES_PER_PATCH}x{VERTICES_PER_PATCH} verts per patch"
        );
        let total_patches = PATCHES_PER_AXIS
            .checked_mul(PATCHES_PER_AXIS)
            .expect("tessellation patch count must fit u32");
        let config = TessellationBuildConfig {
            surface_size,
            num_patches: PATCHES_PER_AXIS,
            total_patches,
        };
        let first = TessellationStageOne {
            patch_mesh: create_patch_mesh(VERTICES_PER_PATCH),
            instance_buffer: create_instance_buffer(device, total_patches),
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
            mid: [raw_data.mid[0], raw_data.mid[1], raw_data.mid[2], 0.0],
            range: [raw_data.range[0], raw_data.range[1], raw_data.range[2], 0.0],
            terrain_info: [
                raw_data
                    .num_verts_per_axis
                    .to_f32()
                    .expect("terrain vertex count must fit f32"),
                raw_data.tile_scale,
                patch_count,
                patch_count,
            ],
            world_min: [0.0; 4],
            world_max: [0.0; 4],
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
            texture_scales,
            lighting_buffer,
        };
        self.create_gpu_tessellation_resources_part3(device, queue, raw_data, config, second);
    }

    fn create_tessellation_auxiliary(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        position_for_shadow: &wgpu::TextureView,
        camera_layout: &wgpu::BindGroupLayout,
        num_patches: u32,
    ) -> TessellationAuxiliary {
        let shadow = create_shadow_resources(
            device,
            queue,
            raw_data,
            position_for_shadow,
            camera_layout,
            num_patches,
        );
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
            local_lights: create_local_lights_buffer(device),
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
            texture_scales,
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
            &textures.position_for_shadow,
            &camera.layout,
            config.num_patches,
        );
        let composited_albedo = self
            .compositor
            .as_ref()
            .expect("compositor was initialized in tessellation stage two")
            .albedo_atlas_view();
        let texture_bind_group = create_gpu_texture_bind_group(
            device,
            &texture_layout,
            &GpuTessBindings {
                tess_params: &tess_params_buffer,
                position: &textures.position,
                normal: &textures.normal,
                xtt_albedo: &textures.xtt_albedo,
                terrain_sampler: &textures.samplers.terrain,
                params: &params_buffer,
                ao: &textures.ao,
                alpha: &textures.alpha,
                normal_map_array: &textures.normal_map_array,
                terrain_array: &textures.terrain_array,
                alpha_atlas: &textures.alpha_atlas,
                chunk_layers: &textures.chunk_layers,
                alpha_sampler: &textures.samplers.alpha,
                texture_scales: &texture_scales,
                composited_albedo,
                lighting: &lighting_buffer,
                shadow: &auxiliary.shadow.shadow_view,
                blackmap: &auxiliary.blackmap,
                unexplored: &auxiliary.unexplored,
                local_lights: &auxiliary.local_lights,
                alpha_atlas_hi: &textures.alpha_atlas_hi,
                light: &auxiliary.light,
            },
        );
        let pipeline =
            create_gpu_pipeline(device, self.surface_format, &camera.layout, &texture_layout);
        let [width, height] = config.surface_size;
        let (depth_texture, depth_view) = create_depth_texture(device, width, height);
        let expanded_vertices = expand_patch_mesh(&patch_mesh);
        let expanded_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Expanded Vertex Buffer"),
            contents: bytemuck::cast_slice(&expanded_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let vertex_count = u32::try_from(expanded_vertices.len())
            .expect("expanded tessellation vertex count must fit u32");
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
            depth_texture,
            depth_view,
            params_buffer,
            lighting_buffer: Some(lighting_buffer),
            terrain_size: [terrain_size.x, terrain_size.z],
            tile_scale: raw_data.tile_scale,
            use_gpu_tessellation: true,
            num_patch_instances: config.total_patches,
        });
        let triangle_count = (expanded_vertices.len() / 3)
            .checked_mul(
                usize::try_from(config.total_patches)
                    .expect("tessellation patch count must fit usize"),
            )
            .expect("tessellation triangle count must fit usize");
        log::info!(
            "GPU tessellation resources created: {} patches, {} vertices per patch, {triangle_count} total triangles",
            config.total_patches,
            expanded_vertices.len(),
        );
        self.init_foliage_resources(device, queue);
        self.init_road_resources(device, queue);
    }
}
