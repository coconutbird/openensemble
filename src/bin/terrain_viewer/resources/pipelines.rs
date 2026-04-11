//! Pipeline and bind group creation for terrain rendering.
//!
//! Contains the main GPU resource initialization functions that create
//! render pipelines, bind group layouts, and wire everything together.

use glam::Vec3;
use render::terrain::{
    CompositingConfig, CompositorResources, GPU_TESS_SHADER, GpuTessParams, LightingParams,
    TERRAIN_SHADER, TerrainParams,
};
use render::wgpu;

use crate::gpu::create_depth_texture;
use crate::types::{AlbedoData, GpuResources, RawXtdData};
use crate::viewer::TerrainViewer;

impl TerrainViewer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn init_compositor(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_array_view: &wgpu::TextureView,
        alpha_atlas_view: &wgpu::TextureView,
        alpha_atlas_hi_view: &wgpu::TextureView,
        chunk_layers_buffer: &wgpu::Buffer,
        texture_scales_buffer: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        alpha_sampler: &wgpu::Sampler,
    ) {
        let config = CompositingConfig::default();
        log::info!(
            "Initializing GPU compositor: {}×{} atlas ({} chunks)",
            config.atlas_width,
            config.atlas_height,
            config.total_chunks()
        );

        // Create compositor resources (atlas textures, pipeline, bind group layout)
        let compositor = CompositorResources::new(device, config);

        // Create decal resources
        let (_decal_alpha_atlas, decal_alpha_atlas_view) =
            self.create_decal_alpha_atlas(device, queue);
        let chunk_decal_layers_buffer = self.create_chunk_decal_layers_buffer(device);
        let decal_instances_buffer = self.create_decal_instances_buffer(device);
        let decal_uv_scales_buffer = self.create_decal_uv_scales_buffer(device);

        // Create bind group with actual terrain textures + decal resources
        let bind_group = compositor.create_bind_group(
            device,
            terrain_array_view,
            alpha_atlas_view,
            alpha_atlas_hi_view,
            chunk_layers_buffer,
            texture_scales_buffer,
            sampler,
            alpha_sampler,
            &decal_alpha_atlas_view,
            &chunk_decal_layers_buffer,
            &decal_instances_buffer,
            &decal_uv_scales_buffer,
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
        let chunks_x = 16u32;
        let chunks_z = 16u32;

        let chunk_width = (world_max[0] - world_min[0]) / chunks_x as f32;
        let chunk_depth = (world_max[2] - world_min[2]) / chunks_z as f32;
        let chunk_height = (world_max[1] - world_min[1]) / 2.0; // Average Y for center

        self.chunk_centers.clear();
        for cz in 0..chunks_z {
            for cx in 0..chunks_x {
                let center_x = world_min[0] + (cx as f32 + 0.5) * chunk_width;
                let center_y = world_min[1] + chunk_height; // Approximate center Y
                let center_z = world_min[2] + (cz as f32 + 0.5) * chunk_depth;
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
            camera_bgl,
            self.surface_format,
            &road.positions,
            &road.uvs,
            &textures.albedo_pixels,
            textures.width,
            textures.height,
        ));
    }
}

impl TerrainViewer {
    /// Create GPU resources for regular terrain rendering (CPU tessellation mode).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_gpu_resources_from_data(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        albedo: Option<AlbedoData>,
        width: u32,
        height: u32,
    ) {
        use wgpu::util::DeviceExt;

        // Create interleaved vertex data: [pos, normal, uv, pos, normal, uv, ...]
        // 3 + 3 + 2 = 8 floats per vertex
        let mut vertex_data = Vec::with_capacity(positions.len() * 8);
        for i in 0..positions.len() {
            vertex_data.extend_from_slice(&positions[i]);
            vertex_data.extend_from_slice(&normals[i]);
            vertex_data.extend_from_slice(&uvs[i]);
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Uniform Buffer"),
            size: 64, // mat4x4<f32>
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Create terrain texture array from loaded textures
        let (_terrain_array, terrain_array_view) =
            self.create_terrain_texture_array(device, queue, &albedo);

        // Create alpha atlases from chunk splat data
        let (_alpha_atlas, alpha_atlas_view) = self.create_alpha_atlas(device, queue);
        let (_alpha_atlas_hi, alpha_atlas_hi_view) = self.create_alpha_atlas_hi(device, queue);

        // Create XTT albedo texture (original pre-composited from game export)
        let (_xtt_albedo_texture, xtt_albedo_view) =
            self.create_xtt_albedo_texture(device, queue, &albedo);

        // Create chunk layers storage buffer
        let chunk_layers_buffer = self.create_chunk_layers_buffer(device);

        // Create terrain params uniform
        let (terrain_size, tile_scale) = if let Some(scene) = &self.scene {
            (scene.mesh.size(), scene.mesh.tile_scale)
        } else {
            (Vec3::new(1024.0, 100.0, 1024.0), 1.0)
        };

        let params = TerrainParams {
            terrain_size: [terrain_size.x, terrain_size.z],
            chunk_count: [16.0, 16.0],
            texture_tile_scale: tile_scale,
            debug_mode: self.debug_mode as f32,
            bump_power: self.bump_power,
            _padding: 0.0,
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Params Buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Terrain Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat, // Repeat for tiling
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });

        // Separate sampler for alpha atlas - use Linear filtering like the game does
        // The game uses: MinFilter = LINEAR; MagFilter = LINEAR;
        // This gives smooth blending between textures within each chunk
        let alpha_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Alpha Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest, // No mipmaps on alpha atlas
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    // binding 0: terrain texture array
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 1: sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 2: alpha texture array (filterable for linear sampling like the game)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 3: chunk layers storage buffer
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 4: terrain params uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 5: alpha atlas sampler (linear filtering like the game)
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 6: XTT albedo (original pre-composited from game export)
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 7: Texture scales buffer (per-texture u_scale/v_scale)
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Create texture scales buffer
        let texture_scales_buffer = self.create_texture_scales_buffer(device);

        // Initialize GPU compositor (for pre-baked terrain textures)
        self.init_compositor(
            device,
            queue,
            &terrain_array_view,
            &alpha_atlas_view,
            &alpha_atlas_hi_view,
            &chunk_layers_buffer,
            &texture_scales_buffer,
            &sampler,
            &alpha_sampler,
        );

        // Calculate chunk centers for LOD calculations
        self.calculate_chunk_centers();

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&terrain_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&alpha_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: chunk_layers_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&alpha_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&xtt_albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: texture_scales_buffer.as_entire_binding(),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain Shader"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terrain Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32, // 8 floats * 4 bytes
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0, // position
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1, // normal
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2, // uv
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.surface_format,
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
        });

        let (depth_texture, depth_view) = create_depth_texture(device, width, height);

        self.gpu = Some(GpuResources {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            camera_buffer,
            camera_bind_group_layout,
            camera_bind_group,
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
            positions.len(),
            indices.len(),
            self.scene.as_ref().map_or(0, |s| s.terrain_textures.len())
        );

        // Initialize foliage resources if we have foliage data
        self.init_foliage_resources(device, queue);

        // Initialize road resources if we have road data
        self.init_road_resources(device, queue);
    }
}

impl TerrainViewer {
    /// Create GPU resources for GPU tessellation mode.
    /// Uses instanced patch rendering with vertex shader displacement.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_gpu_tessellation_resources(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        albedo: Option<AlbedoData>,
        width: u32,
        height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let num_verts = raw_data.num_verts_per_axis;
        let num_patches = 64u32; // 64x64 patches like original game
        let verts_per_patch = 16u32; // 16x16 vertices per patch for subdivision

        log::info!(
            "Creating GPU tessellation resources: {}x{} patches, {}x{} verts per patch",
            num_patches,
            num_patches,
            verts_per_patch,
            verts_per_patch
        );

        // Create subdivided patch mesh template
        // Each patch has verts_per_patch x verts_per_patch vertices
        // with local UVs from [0, 1]
        let mut patch_vertices: Vec<[f32; 2]> = Vec::new();
        let mut patch_indices: Vec<u32> = Vec::new();

        for z in 0..verts_per_patch {
            for x in 0..verts_per_patch {
                let u = x as f32 / (verts_per_patch - 1) as f32;
                let v = z as f32 / (verts_per_patch - 1) as f32;
                patch_vertices.push([u, v]);
            }
        }

        // Generate indices for patch triangles
        for z in 0..(verts_per_patch - 1) {
            for x in 0..(verts_per_patch - 1) {
                let top_left = z * verts_per_patch + x;
                let top_right = top_left + 1;
                let bottom_left = (z + 1) * verts_per_patch + x;
                let bottom_right = bottom_left + 1;

                // Two triangles per quad
                patch_indices.push(top_left);
                patch_indices.push(bottom_left);
                patch_indices.push(top_right);

                patch_indices.push(top_right);
                patch_indices.push(bottom_left);
                patch_indices.push(bottom_right);
            }
        }

        // Create instance data (patch indices)
        let total_patches = num_patches * num_patches;
        let instance_data: Vec<u32> = (0..total_patches).collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Patch Vertex Buffer"),
            contents: bytemuck::cast_slice(&patch_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Patch Index Buffer"),
            contents: bytemuck::cast_slice(&patch_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Create position texture (R32Uint format)
        let position_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Position Texture"),
            size: wgpu::Extent3d {
                width: num_verts,
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
                texture: &position_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&raw_data.packed_positions),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(num_verts * 4),
                rows_per_image: Some(num_verts),
            },
            wgpu::Extent3d {
                width: num_verts,
                height: num_verts,
                depth_or_array_layers: 1,
            },
        );

        let position_view = position_texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Second view of position texture for shadow pass
        let position_view_for_shadow =
            position_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create normal texture (R32Uint format)
        let normal_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Normal Texture"),
            size: wgpu::Extent3d {
                width: num_verts,
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
                texture: &normal_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&raw_data.packed_normals),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(num_verts * 4),
                rows_per_image: Some(num_verts),
            },
            wgpu::Extent3d {
                width: num_verts,
                height: num_verts,
                depth_or_array_layers: 1,
            },
        );

        let normal_view = normal_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create AO texture (R8Unorm format, half resolution)
        // Based on IDA RE: AO is stored at 1024×512 for a 1024×1024 terrain
        // (full width, half height)
        // The game samples with bilinear filtering via gVertSampler_ao_Texture
        let (ao_width, ao_height, ao_values) = raw_data.ao_data.as_ref().map_or_else(
            || {
                log::warn!(
                    "No AO data available, using default fully-lit values at half resolution"
                );
                let w = num_verts; // full width
                let h = num_verts / 2; // half height
                (w, h, vec![255u8; (w * h) as usize])
            },
            |ao| {
                log::info!(
                    "Using half-resolution AO texture: {}x{} ({} bytes)",
                    ao.width,
                    ao.height,
                    ao.values.len()
                );
                (ao.width, ao.height, ao.values.clone())
            },
        );

        let ao_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("AO Texture (Half Resolution)"),
            size: wgpu::Extent3d {
                width: ao_width,
                height: ao_height,
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
                texture: &ao_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &ao_values,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ao_width),
                rows_per_image: Some(ao_height),
            },
            wgpu::Extent3d {
                width: ao_width,
                height: ao_height,
                depth_or_array_layers: 1,
            },
        );

        let ao_view = ao_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create Alpha texture (same format/dimensions as AO - terrain holes/transparency)
        let (alpha_width, alpha_height, alpha_values) = raw_data.alpha_data.as_ref().map_or_else(
            || {
                log::warn!(
                    "No Alpha data available, using default fully-opaque values at half resolution"
                );
                let w = num_verts; // full width
                let h = num_verts / 2; // half height
                (w, h, vec![255u8; (w * h) as usize])
            },
            |alpha| {
                log::info!(
                    "Using half-resolution Alpha texture: {}x{} ({} bytes)",
                    alpha.width,
                    alpha.height,
                    alpha.values.len()
                );
                (alpha.width, alpha.height, alpha.values.clone())
            },
        );

        let alpha_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Alpha Texture (Half Resolution)"),
            size: wgpu::Extent3d {
                width: alpha_width,
                height: alpha_height,
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
                texture: &alpha_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &alpha_values,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(alpha_width),
                rows_per_image: Some(alpha_height),
            },
            wgpu::Extent3d {
                width: alpha_width,
                height: alpha_height,
                depth_or_array_layers: 1,
            },
        );

        let alpha_view = alpha_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create XTT albedo texture
        let (_xtt_albedo_texture, xtt_albedo_view) =
            self.create_xtt_albedo_texture(device, queue, &albedo);

        // Create normal map texture array
        let (_normal_map_array, normal_map_array_view) =
            self.create_normal_map_array(device, queue);

        // Create terrain texture array (for splatting)
        let (_terrain_array, terrain_array_view) =
            self.create_terrain_texture_array(device, queue, &albedo);

        // Create alpha atlases from chunk splat data (for texture splatting)
        // Lo atlas: layers 1-4 in RGBA, Hi atlas: layers 5-7 in RGB
        let (_alpha_atlas, alpha_atlas_view) = self.create_alpha_atlas(device, queue);
        let (_alpha_atlas_hi, alpha_atlas_hi_view) = self.create_alpha_atlas_hi(device, queue);

        // Create chunk layers storage buffer (for texture splatting)
        let chunk_layers_buffer = self.create_chunk_layers_buffer(device);

        // Create alpha sampler (linear filtering like the game for smooth blending)
        let alpha_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Alpha Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Continue with tessellation params and pipeline setup
        self.create_gpu_tessellation_resources_part2(
            device,
            queue,
            raw_data,
            width,
            height,
            num_patches,
            total_patches,
            patch_vertices,
            patch_indices,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            position_view,
            position_view_for_shadow,
            normal_view,
            ao_view,
            alpha_view,
            xtt_albedo_view,
            normal_map_array_view,
            terrain_array_view,
            alpha_atlas_view,
            alpha_atlas_hi_view,
            chunk_layers_buffer,
            alpha_sampler,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn create_gpu_tessellation_resources_part2(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        width: u32,
        height: u32,
        num_patches: u32,
        total_patches: u32,
        patch_vertices: Vec<[f32; 2]>,
        patch_indices: Vec<u32>,
        vertex_buffer: wgpu::Buffer,
        index_buffer: wgpu::Buffer,
        instance_buffer: wgpu::Buffer,
        position_view: wgpu::TextureView,
        position_view_for_shadow: wgpu::TextureView,
        normal_view: wgpu::TextureView,
        ao_view: wgpu::TextureView,
        alpha_view: wgpu::TextureView,
        xtt_albedo_view: wgpu::TextureView,
        normal_map_array_view: wgpu::TextureView,
        terrain_array_view: wgpu::TextureView,
        alpha_atlas_view: wgpu::TextureView,
        alpha_atlas_hi_view: wgpu::TextureView,
        chunk_layers_buffer: wgpu::Buffer,
        alpha_sampler: wgpu::Sampler,
    ) {
        use wgpu::util::DeviceExt;

        let num_verts = raw_data.num_verts_per_axis;

        // Create tessellation params uniform buffer
        let tess_params = GpuTessParams {
            mid: [raw_data.mid[0], raw_data.mid[1], raw_data.mid[2], 0.0],
            range: [raw_data.range[0], raw_data.range[1], raw_data.range[2], 0.0],
            terrain_info: [
                num_verts as f32,
                raw_data.tile_scale,
                num_patches as f32,
                num_patches as f32,
            ],
            world_min: [0.0, 0.0, 0.0, 0.0], // Not used directly, computed from tile_scale
            world_max: [0.0, 0.0, 0.0, 0.0],
        };

        let tess_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Params Buffer"),
            contents: bytemuck::bytes_of(&tess_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create terrain params buffer for debug mode
        let terrain_size = if let Some(scene) = &self.scene {
            scene.mesh.size()
        } else {
            Vec3::new(1024.0, 100.0, 1024.0)
        };

        let params = TerrainParams {
            terrain_size: [terrain_size.x, terrain_size.z],
            chunk_count: [16.0, 16.0],
            texture_tile_scale: 32.0,
            debug_mode: self.debug_mode as f32,
            bump_power: self.bump_power,
            _padding: 0.0,
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Params Buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create sampler - MUST use Repeat for texture tiling (UVs go 0-16 for 16 chunks)
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
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

        // Camera uniform buffer
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Uniform Buffer"),
            size: 64, // mat4x4<f32>
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Texture bind group layout for GPU tessellation
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GPU Tess Texture Bind Group Layout"),
                entries: &[
                    // binding 0: tess params uniform (needed by both VS and FS for normal sampling)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 1: position texture (R32Uint)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Uint,
                        },
                        count: None,
                    },
                    // binding 2: normal texture (R32Uint) - needed by both VS and FS for normal sampling
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Uint,
                        },
                        count: None,
                    },
                    // binding 3: XTT albedo texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 4: sampler (used in both VS for light texture and PS for terrain)
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 5: terrain params
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 6: AO texture (R8Unorm)
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 7: Alpha texture (R8Unorm) - terrain holes/transparency
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 8: Normal map texture array
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 9: Terrain texture array (for splatting with normal maps)
                    wgpu::BindGroupLayoutEntry {
                        binding: 9,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 10: Alpha texture array (for texture splatting blend weights)
                    wgpu::BindGroupLayoutEntry {
                        binding: 10,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 11: Chunk layers storage buffer (per-chunk texture IDs)
                    wgpu::BindGroupLayoutEntry {
                        binding: 11,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 12: Alpha sampler (linear filtering like the game)
                    wgpu::BindGroupLayoutEntry {
                        binding: 12,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 13: Texture scales buffer (per-texture u_scale/v_scale)
                    wgpu::BindGroupLayoutEntry {
                        binding: 13,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 14: GPU-composited albedo atlas (from compositor)
                    wgpu::BindGroupLayoutEntry {
                        binding: 14,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 15: Lighting params uniform (SH, directional, fog, shadow, blackmap, local lights)
                    wgpu::BindGroupLayoutEntry {
                        binding: 15,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 16: Shadow map texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 16,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 17: Blackmap visibility texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 17,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 18: Blackmap unexplored mask texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 18,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 19: Local lights storage buffer
                    wgpu::BindGroupLayoutEntry {
                        binding: 19,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 20: High alpha texture array (overflow layers 5-7)
                    wgpu::BindGroupLayoutEntry {
                        binding: 20,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 21: Light texture (L8 luminance from XTD)
                    wgpu::BindGroupLayoutEntry {
                        binding: 21,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                ],
            });

        // Create texture scales buffer
        let texture_scales_buffer = self.create_texture_scales_buffer(device);

        // Initialize GPU compositor (for pre-baked terrain textures)
        self.init_compositor(
            device,
            queue,
            &terrain_array_view,
            &alpha_atlas_view,
            &alpha_atlas_hi_view,
            &chunk_layers_buffer,
            &texture_scales_buffer,
            &sampler,
            &alpha_sampler,
        );

        // Calculate chunk centers for LOD calculations
        self.calculate_chunk_centers();

        // Create lighting params buffer
        let lighting_params = LightingParams::default();
        let lighting_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Lighting Params Buffer"),
            contents: bytemuck::bytes_of(&lighting_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Continue with bind group and pipeline creation
        self.create_gpu_tessellation_resources_part3(
            device,
            queue,
            raw_data,
            width,
            height,
            total_patches,
            patch_vertices,
            patch_indices,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            position_view,
            position_view_for_shadow,
            normal_view,
            ao_view,
            alpha_view,
            xtt_albedo_view,
            normal_map_array_view,
            terrain_array_view,
            alpha_atlas_view,
            alpha_atlas_hi_view,
            chunk_layers_buffer,
            alpha_sampler,
            tess_params_buffer,
            params_buffer,
            sampler,
            camera_buffer,
            camera_bind_group_layout,
            camera_bind_group,
            terrain_size,
            texture_bind_group_layout,
            texture_scales_buffer,
            lighting_buffer,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn create_gpu_tessellation_resources_part3(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        width: u32,
        height: u32,
        total_patches: u32,
        patch_vertices: Vec<[f32; 2]>,
        patch_indices: Vec<u32>,
        vertex_buffer: wgpu::Buffer,
        index_buffer: wgpu::Buffer,
        instance_buffer: wgpu::Buffer,
        position_view: wgpu::TextureView,
        position_view_for_shadow: wgpu::TextureView,
        normal_view: wgpu::TextureView,
        ao_view: wgpu::TextureView,
        alpha_view: wgpu::TextureView,
        xtt_albedo_view: wgpu::TextureView,
        normal_map_array_view: wgpu::TextureView,
        terrain_array_view: wgpu::TextureView,
        alpha_atlas_view: wgpu::TextureView,
        alpha_atlas_hi_view: wgpu::TextureView,
        chunk_layers_buffer: wgpu::Buffer,
        alpha_sampler: wgpu::Sampler,
        tess_params_buffer: wgpu::Buffer,
        params_buffer: wgpu::Buffer,
        sampler: wgpu::Sampler,
        camera_buffer: wgpu::Buffer,
        camera_bind_group_layout: wgpu::BindGroupLayout,
        camera_bind_group: wgpu::BindGroup,
        terrain_size: Vec3,
        texture_bind_group_layout: wgpu::BindGroupLayout,
        texture_scales_buffer: wgpu::Buffer,
        lighting_buffer: wgpu::Buffer,
    ) {
        use wgpu::util::DeviceExt;

        // Create placeholder textures for blackmap (1x1, disabled by default)
        let placeholder_texture = |label: &str, data: &[u8; 4]| -> wgpu::TextureView {
            let tex = device.create_texture_with_data(
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
                data,
            );
            tex.create_view(&wgpu::TextureViewDescriptor::default())
        };

        // Create shadow resources and use real shadow map texture
        let vertex_buffer_layouts = &[
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
        let mut shadow = crate::shadow::ShadowResources::new(
            device,
            &camera_bind_group_layout,
            vertex_buffer_layouts,
        );
        let num_verts = raw_data.num_verts_per_axis;
        let num_patches = 64u32;
        shadow.setup_params(
            device,
            queue,
            &position_view_for_shadow,
            [
                num_verts as f32,
                raw_data.tile_scale,
                num_patches as f32,
                num_patches as f32,
            ],
            raw_data.mid,
            raw_data.range,
        );
        let shadow_map_view = &shadow.shadow_view;
        log::info!("Shadow resources initialized");

        // Blackmap: alpha=0 means fully visible (no fog-of-war)
        let blackmap_view = placeholder_texture("Placeholder Blackmap", &[0, 0, 0, 0]);
        // Unexplored: black with alpha=0 (no unexplored overlay)
        let unexplored_view = placeholder_texture("Placeholder Unexplored", &[0, 0, 0, 0]);

        // Light texture (L8 luminance from XTD, HWDE: gVertSampler_light_Texture)
        let light_view = if let Some(scene) = &self.scene
            && let Some(ld) = &scene.lighting_data
        {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Light Texture"),
                size: wgpu::Extent3d {
                    width: ld.width,
                    height: ld.height,
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
                    texture: &tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &ld.values,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ld.width),
                    rows_per_image: Some(ld.height),
                },
                wgpu::Extent3d {
                    width: ld.width,
                    height: ld.height,
                    depth_or_array_layers: 1,
                },
            );
            log::info!("Created light texture: {}x{}", ld.width, ld.height);
            tex.create_view(&wgpu::TextureViewDescriptor::default())
        } else {
            // Placeholder: mid-gray (neutral lighting)
            placeholder_texture("Placeholder Light", &[128, 128, 128, 255])
        };

        // Empty local lights buffer (minimum 16 bytes for storage buffer)
        let local_lights_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Local Lights Buffer"),
            contents: &[0u8; 64], // 4 vec4s minimum (1 dummy light slot)
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU Tess Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: tess_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&position_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&xtt_albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&alpha_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&normal_map_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&terrain_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&alpha_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: chunk_layers_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::Sampler(&alpha_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: texture_scales_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(
                        self.compositor.as_ref().unwrap().albedo_atlas_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: lighting_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: wgpu::BindingResource::TextureView(shadow_map_view),
                },
                wgpu::BindGroupEntry {
                    binding: 17,
                    resource: wgpu::BindingResource::TextureView(&blackmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 18,
                    resource: wgpu::BindingResource::TextureView(&unexplored_view),
                },
                wgpu::BindGroupEntry {
                    binding: 19,
                    resource: local_lights_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 20,
                    resource: wgpu::BindingResource::TextureView(&alpha_atlas_hi_view),
                },
                wgpu::BindGroupEntry {
                    binding: 21,
                    resource: wgpu::BindingResource::TextureView(&light_view),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Tessellation Shader"),
            source: wgpu::ShaderSource::Wgsl(GPU_TESS_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU Tess Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Two vertex buffers: patch vertices (per-vertex) and instance data (per-instance)
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GPU Tessellation Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // Per-vertex: local UV
                    wgpu::VertexBufferLayout {
                        array_stride: 8, // 2 floats
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0, // local_uv
                        }],
                    },
                    // Per-instance: patch index
                    wgpu::VertexBufferLayout {
                        array_stride: 4, // 1 u32
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 0,
                            shader_location: 1, // patch_index
                        }],
                    },
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.surface_format,
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
        });

        let (depth_texture, depth_view) = create_depth_texture(device, width, height);

        // Create non-indexed patch vertices (expanded triangles)
        // This is less efficient but simpler - each triangle has its own vertices
        let mut expanded_vertices: Vec<[f32; 2]> = Vec::new();
        for idx in &patch_indices {
            expanded_vertices.push(patch_vertices[*idx as usize]);
        }

        let expanded_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Expanded Vertex Buffer"),
            contents: bytemuck::cast_slice(&expanded_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Now we have:
        // - expanded_vertex_buffer: patch triangle vertices (non-indexed)
        // - instance_buffer: patch indices for instancing

        self.shadow_resources = Some(shadow);

        self.gpu = Some(GpuResources {
            pipeline,
            vertex_buffer: expanded_vertex_buffer,
            index_buffer: instance_buffer,
            index_count: expanded_vertices.len() as u32, // vertex count for draw()
            camera_buffer,
            camera_bind_group_layout,
            camera_bind_group,
            texture_bind_group,
            depth_texture,
            depth_view,
            params_buffer,
            lighting_buffer: Some(lighting_buffer),
            terrain_size: [terrain_size.x, terrain_size.z],
            tile_scale: raw_data.tile_scale,
            use_gpu_tessellation: true,
            num_patch_instances: total_patches,
        });

        log::info!(
            "GPU tessellation resources created: {} patches, {} vertices per patch, {} total triangles",
            total_patches,
            expanded_vertices.len(),
            (expanded_vertices.len() / 3) * total_patches as usize
        );

        // Suppress unused variable warnings
        let _ = index_buffer;
        let _ = vertex_buffer;

        // Initialize foliage resources if we have foliage data
        self.init_foliage_resources(device, queue);

        // Initialize road resources if we have road data
        self.init_road_resources(device, queue);
    }
}
