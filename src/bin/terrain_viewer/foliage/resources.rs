//! GPU resources for foliage rendering.

use super::FoliageConfig;
#[allow(unused_imports)]
use crate::types::FoliageSet;
use render::terrain::FOLIAGE_SHADER;
use render::wgpu;

/// A single foliage draw call (one per QN chunk × set pair).
pub struct FoliageDrawCall {
    /// Dynamic offset into chunk_info_buffer for this draw.
    pub dynamic_offset: u32,
    /// Which foliage set to render (index into set_resources).
    pub set_index: usize,
    /// Number of vertices per blade for this set.
    pub num_verts_per_blade: u32,
    /// Number of active blades in this draw call (from index buffer parsing).
    pub num_active_blades: u32,
}

/// GPU resources for foliage rendering.
pub struct FoliageResources {
    /// Render pipeline for foliage.
    pub pipeline: wgpu::RenderPipeline,
    /// Uniform buffer for foliage parameters (global — lighting, fog, etc).
    pub params_buffer: wgpu::Buffer,
    /// Dynamic uniform buffer for per-chunk data (chunk offsets).
    pub chunk_info_buffer: Option<wgpu::Buffer>,
    /// Bind group layout for foliage params + terrain textures.
    pub params_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group layout for foliage material textures + blade geometry.
    pub material_bind_group_layout: wgpu::BindGroupLayout,
    /// Per-set resources (textures, bind groups).
    pub set_resources: Vec<FoliageSetResources>,
    /// Params bind group (terrain textures + dynamic chunk info).
    pub params_bind_group: Option<wgpu::BindGroup>,
    /// Pre-built draw calls from QN chunk data.
    pub draw_calls: Vec<FoliageDrawCall>,
    /// Minimum uniform buffer offset alignment (for dynamic uniform).
    pub min_offset_alignment: u32,
    /// Current configuration.
    pub config: FoliageConfig,
    /// Blade map texture: compact list of (grid_position, blade_type) per active blade.
    /// Each texel is Rg32Uint: r = grid_position (0..4095), g = blade_type.
    /// Draw calls index into this via blade_data_offset in ChunkInfo.
    pub blade_map_texture: Option<wgpu::Texture>,
    pub blade_map_view: Option<wgpu::TextureView>,
}

/// Per-foliage-set GPU resources.
pub struct FoliageSetResources {
    /// Albedo texture.
    pub _albedo_texture: wgpu::Texture,
    pub _albedo_view: wgpu::TextureView,
    /// Opacity texture.
    pub _opacity_texture: wgpu::Texture,
    pub _opacity_view: wgpu::TextureView,
    /// Blade positions texture (xyz = pos, w = u).
    pub _blade_positions_texture: wgpu::Texture,
    pub _blade_positions_view: wgpu::TextureView,
    /// Blade normals texture (xyz = normal, w = v).
    pub _blade_normals_texture: wgpu::Texture,
    pub _blade_normals_view: wgpu::TextureView,
    /// Material bind group.
    pub material_bind_group: wgpu::BindGroup,
    /// Number of blade types in this set.
    pub num_blade_types: u32,
    /// Number of vertices per blade.
    pub num_verts_per_blade: u32,
}

/// Foliage uniform parameters (must match shader struct).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FoliageParamsUniform {
    /// terrain_info: num_verts_per_axis, tile_scale, chunk_offset_x, chunk_offset_z
    pub terrain_info: [f32; 4],
    /// world_min: x, y, z, padding
    pub world_min: [f32; 4],
    /// world_range: x, y, z, padding
    pub world_range: [f32; 4],
    /// foliage_info: num_verts_per_blade, rcp_num_blades, fade_start, fade_end
    pub foliage_info: [f32; 4],
    /// camera_pos: x, y, z, time
    pub camera_pos_time: [f32; 4],
    /// dir_light_vec: x, y, z, backside_shadow_scalar
    pub dir_light_vec: [f32; 4],
    /// dir_light_color: r, g, b, padding
    pub dir_light_color: [f32; 4],
    /// fog_params: density^2, start^2, unused, unused
    pub fog_params: [f32; 4],
    /// fog_color: r, g, b, a
    pub fog_color: [f32; 4],
    /// planar_fog_params: enabled, start_y, density^2, unused
    pub planar_fog_params: [f32; 4],
    /// planar_fog_color: r, g, b, a
    pub planar_fog_color: [f32; 4],
    /// SH fill lighting coefficients (7 vec4s)
    pub sh_fill_ar: [f32; 4],
    pub sh_fill_ag: [f32; 4],
    pub sh_fill_ab: [f32; 4],
    pub sh_fill_br: [f32; 4],
    pub sh_fill_bg: [f32; 4],
    pub sh_fill_bb: [f32; 4],
    pub sh_fill_c: [f32; 4],
    // Shadow params (8 vec4s)
    pub shadow_vp_col0: [f32; 4],
    pub shadow_vp_col1: [f32; 4],
    pub shadow_vp_col2: [f32; 4],
    pub shadow_vp_col3: [f32; 4],
    pub shadow_params: [f32; 4], // x = csm_scale, y = num_passes, z = enabled
    // Blackmap params (3 vec4s)
    pub blackmap_params0: [f32; 4], // rgb = bg_color, w = fog_scalar
    pub blackmap_params1: [f32; 4], // x = unexplored_scalar, yz = bounds_lo_xz, w = enabled
    pub blackmap_params2: [f32; 4], // x = pad, yz = bounds_hi_xz, w = bounds_falloff
}

/// Per-chunk uniform data (must match shader ChunkInfo struct).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ChunkInfoUniform {
    /// Chunk offset: mMinXVert, mMinZVert from quad node
    pub chunk_offset: [f32; 2],
    /// Number of vertices per blade (varies per set)
    pub num_verts_per_blade: f32,
    /// Offset into blade map texture for this draw call's blade data.
    pub blade_data_offset: f32,
}

impl FoliageResources {
    /// Create foliage rendering resources.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let min_offset_alignment = device.limits().min_uniform_buffer_offset_alignment;

        // Create bind group layouts
        // Group 1: global params + per-chunk dynamic uniform + terrain textures
        let params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Foliage Params Bind Group Layout"),
                entries: &[
                    // binding 0: Global params uniform (static)
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
                    // binding 1: Per-chunk info (dynamic offset)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                                ChunkInfoUniform,
                            >()
                                as u64),
                        },
                        count: None,
                    },
                    // binding 2: Heightmap texture (R32Float - not filterable)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 3: Heightmap sampler (non-filtering)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    // binding 4: Shadow map
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 5: Blackmap
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 6: Unexplored mask
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 7: Blade map texture (Rg32Uint — grid_position + blade_type per active blade)
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Uint,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        // Group 2: per-set material textures + blade geometry
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Foliage Material Bind Group Layout"),
                entries: &[
                    // binding 0: Albedo texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 1: Opacity texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 2: Foliage sampler (filtering)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 3: Blade positions texture (Rgba32Float)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 4: Blade normals texture (Rgba32Float)
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 5: Blade sampler (non-filtering for float32)
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                ],
            });

        // Create params buffer
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foliage Params Buffer"),
            size: std::mem::size_of::<FoliageParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create render pipeline
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Foliage Shader"),
            source: wgpu::ShaderSource::Wgsl(FOLIAGE_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Foliage Pipeline Layout"),
            bind_group_layouts: &[
                camera_bind_group_layout,
                &params_bind_group_layout,
                &material_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Foliage Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[], // No vertex buffers - geometry from textures
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    // Game uses alpha blending with discard at 0.6666 as optimization
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Two-sided rendering
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
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

        Self {
            pipeline,
            params_buffer,
            chunk_info_buffer: None,
            params_bind_group_layout,
            material_bind_group_layout,
            set_resources: Vec::new(),
            params_bind_group: None,
            draw_calls: Vec::new(),
            min_offset_alignment,
            config: FoliageConfig::default(),
            blade_map_texture: None,
            blade_map_view: None,
        }
    }

    /// Create GPU resources for a single foliage set.
    pub fn create_set_resources(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        set: &crate::types::FoliageSet,
    ) -> Option<FoliageSetResources> {
        #[allow(unused_imports)]
        use wgpu::util::DeviceExt;

        // Skip if no albedo texture
        if set.albedo_pixels.is_empty() {
            log::warn!("Skipping foliage set '{}': no albedo texture", set.name);
            return None;
        }

        // Create albedo texture
        let albedo_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("Foliage Albedo: {}", set.name)),
            size: wgpu::Extent3d {
                width: set.albedo_width,
                height: set.albedo_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &albedo_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &set.albedo_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(set.albedo_width * 4),
                rows_per_image: Some(set.albedo_height),
            },
            wgpu::Extent3d {
                width: set.albedo_width,
                height: set.albedo_height,
                depth_or_array_layers: 1,
            },
        );
        let albedo_view = albedo_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create opacity texture (or use albedo alpha if not available)
        let (opacity_texture, opacity_view) = if !set.opacity_pixels.is_empty() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("Foliage Opacity: {}", set.name)),
                size: wgpu::Extent3d {
                    width: set.opacity_width,
                    height: set.opacity_height,
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
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &set.opacity_pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(set.opacity_width * 4),
                    rows_per_image: Some(set.opacity_height),
                },
                wgpu::Extent3d {
                    width: set.opacity_width,
                    height: set.opacity_height,
                    depth_or_array_layers: 1,
                },
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        } else {
            // Use albedo as opacity fallback (alpha channel)
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("Foliage Opacity (fallback): {}", set.name)),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
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
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &[255u8, 255, 255, 255],
                wgpu::TexelCopyBufferLayout::default(),
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };

        // Create blade geometry textures
        // For now, generate a simple default blade if not loaded
        let (blade_positions, blade_normals, num_blade_types, num_verts) = if !set
            .blade_positions
            .is_empty()
        {
            log::info!(
                "Foliage '{}': using loaded blade geometry ({} types, {} verts/blade, {} total)",
                set.name,
                set.num_blade_types,
                set.num_verts_per_blade,
                set.blade_positions.len()
            );
            (
                set.blade_positions.clone(),
                set.blade_normals.clone(),
                set.num_blade_types,
                set.num_verts_per_blade,
            )
        } else {
            // Infer blade type count from texture aspect ratio:
            // each blade type occupies a square region (height x height) in the atlas
            let inferred_types = if set.albedo_height > 0 {
                (set.albedo_width / set.albedo_height).max(1)
            } else {
                1
            };
            log::info!(
                "Foliage '{}': inferred {} blade types from {}x{} texture",
                set.name,
                inferred_types,
                set.albedo_width,
                set.albedo_height,
            );
            Self::generate_default_blade_geometry(inferred_types)
        };

        let blade_positions_texture = Self::create_blade_texture(
            device,
            queue,
            &blade_positions,
            &format!("Foliage Blade Positions: {}", set.name),
            num_verts * num_blade_types,
        );
        let blade_positions_view =
            blade_positions_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let blade_normals_texture = Self::create_blade_texture(
            device,
            queue,
            &blade_normals,
            &format!("Foliage Blade Normals: {}", set.name),
            num_verts * num_blade_types,
        );
        let blade_normals_view =
            blade_normals_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create material sampler — game uses WRAP address mode for foliage textures
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Foliage Material Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Blade sampler (non-filtering for Rgba32Float textures)
        let blade_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Foliage Blade Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create material bind group (includes blade geometry textures)
        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Foliage Material Bind Group: {}", set.name)),
            layout: &self.material_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&opacity_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&blade_positions_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&blade_normals_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&blade_sampler),
                },
            ],
        });

        Some(FoliageSetResources {
            _albedo_texture: albedo_texture,
            _albedo_view: albedo_view,
            _opacity_texture: opacity_texture,
            _opacity_view: opacity_view,
            _blade_positions_texture: blade_positions_texture,
            _blade_positions_view: blade_positions_view,
            _blade_normals_texture: blade_normals_texture,
            _blade_normals_view: blade_normals_view,
            material_bind_group,
            num_blade_types,
            num_verts_per_blade: num_verts,
        })
    }

    /// Generate default grass blade geometry (simple quad strip) for N blade types.
    ///
    /// Each blade type gets UVs covering its horizontal slice of the atlas texture.
    /// For a texture with N types, type i covers U range [i/N, (i+1)/N].
    fn generate_default_blade_geometry(
        num_blade_types: u32,
    ) -> (Vec<[f32; 4]>, Vec<[f32; 4]>, u32, u32) {
        let num_verts_per_blade = 10u32;

        let total_verts = (num_verts_per_blade * num_blade_types) as usize;
        let mut positions = Vec::with_capacity(total_verts);
        let mut normals = Vec::with_capacity(total_verts);

        // Simple grass blade: 5 quads stacked vertically
        // Width tapers from base to top
        // Note: game blade geometry is stored at world scale (no shader scaling).
        // Typical game blades are ~1.5 units tall, ~0.3 wide.
        let blade_height = 1.5;
        let base_width = 0.3;

        for blade_type in 0..num_blade_types {
            // UV range for this blade type in the atlas
            let u_min = blade_type as f32 / num_blade_types as f32;
            let u_max = (blade_type + 1) as f32 / num_blade_types as f32;

            for i in 0..5 {
                let t = i as f32 / 4.0;
                let y = t * blade_height;
                let width = base_width * (1.0 - t * 0.8); // Taper to 20% at top

                // Left vertex
                positions.push([-width, y, 0.0, u_min]); // x, y, z, u
                normals.push([0.0, 0.0, 1.0, 1.0 - t]); // nx, ny, nz, v

                // Right vertex
                positions.push([width, y, 0.0, u_max]); // x, y, z, u
                normals.push([0.0, 0.0, 1.0, 1.0 - t]); // nx, ny, nz, v
            }
        }

        (positions, normals, num_blade_types, num_verts_per_blade)
    }

    /// Create a 1D texture for blade vertex data.
    fn create_blade_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[[f32; 4]],
        label: &str,
        width: u32,
    ) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        if !data.is_empty() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 16), // 4 floats * 4 bytes
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: width.max(1),
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }

        texture
    }

    /// Create the params bind group with heightmap and chunk info dynamic buffer.
    ///
    /// This requires terrain data to create the heightmap texture.
    pub fn create_params_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_data: &crate::types::RawXtdData,
    ) {
        if self.set_resources.is_empty() {
            log::warn!("Cannot create params bind group: no foliage sets loaded");
            return;
        }

        // Create heightmap texture from terrain position data
        let num_verts = terrain_data.num_verts_per_axis;
        let heightmap_texture =
            Self::create_heightmap_texture(device, queue, terrain_data, num_verts);
        let heightmap_view = heightmap_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let heightmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Foliage Heightmap Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Write initial params uniform
        let first_set = &self.set_resources[0];
        let params = FoliageParamsUniform {
            terrain_info: [
                num_verts as f32,
                terrain_data.tile_scale,
                0.0, // unused (chunk offset now in ChunkInfo)
                0.0,
            ],
            world_min: [
                terrain_data.mid[0] - terrain_data.range[0],
                terrain_data.mid[1] - terrain_data.range[1],
                terrain_data.mid[2] - terrain_data.range[2],
                0.0,
            ],
            world_range: [
                terrain_data.range[0] * 2.0,
                terrain_data.range[1] * 2.0,
                terrain_data.range[2] * 2.0,
                0.0,
            ],
            foliage_info: [
                first_set.num_verts_per_blade as f32,
                1.0 / 64.0,
                self.config.fade_start_distance,
                self.config.max_render_distance,
            ],
            camera_pos_time: [0.0, 100.0, 0.0, 0.0],
            dir_light_vec: [0.4472, 0.8944, 0.0, 1.0],
            dir_light_color: [1.0, 1.0, 1.0, 0.0],
            fog_params: [0.0, 0.0, 0.0, 0.0],
            fog_color: [0.7, 0.8, 0.9, 1.0],
            planar_fog_params: [0.0, 0.0, 0.0, 0.0],
            planar_fog_color: [0.7, 0.8, 0.9, 1.0],
            sh_fill_ar: [0.0, 0.0, 0.0, 0.3],
            sh_fill_ag: [0.0, 0.0, 0.0, 0.3],
            sh_fill_ab: [0.0, 0.0, 0.0, 0.3],
            sh_fill_br: [0.0; 4],
            sh_fill_bg: [0.0; 4],
            sh_fill_bb: [0.0; 4],
            sh_fill_c: [0.0; 4],
            shadow_vp_col0: [1.0, 0.0, 0.0, 0.0],
            shadow_vp_col1: [0.0, 1.0, 0.0, 0.0],
            shadow_vp_col2: [0.0, 0.0, 1.0, 0.0],
            shadow_vp_col3: [0.0, 0.0, 0.0, 1.0],
            shadow_params: [1.0, 1.0, 0.0, 0.0],
            blackmap_params0: [0.0, 0.0, 0.0, 0.5],
            blackmap_params1: [0.3, 0.0, 0.0, 0.0],
            blackmap_params2: [0.0, 1024.0, 1024.0, 0.01],
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        // Use existing chunk_info_buffer (from build_draw_calls) or create a minimal one
        if self.chunk_info_buffer.is_none() {
            let aligned_size = Self::align_up(
                std::mem::size_of::<ChunkInfoUniform>() as u32,
                self.min_offset_alignment,
            );
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Foliage Chunk Info Buffer (initial)"),
                size: aligned_size as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let default_chunk = ChunkInfoUniform {
                chunk_offset: [0.0, 0.0],
                num_verts_per_blade: first_set.num_verts_per_blade as f32,
                blade_data_offset: 0.0,
            };
            queue.write_buffer(&buf, 0, bytemuck::bytes_of(&default_chunk));
            self.chunk_info_buffer = Some(buf);
        }
        let chunk_info_buffer = self.chunk_info_buffer.as_ref().unwrap();

        // Create placeholder shadow/blackmap textures (1x1 dummy)
        let dummy_tex = Self::create_dummy_texture(device, queue, "Shadow", &[255, 255, 255, 255]);
        let shadow_view = dummy_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let dummy_black_tex = Self::create_dummy_texture(device, queue, "Blackmap", &[0, 0, 0, 0]);
        let blackmap_view = dummy_black_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let unexplored_view = dummy_black_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Use existing blade map or create a dummy 1x1
        let blade_map_view = if let Some(view) = &self.blade_map_view {
            view
        } else {
            // Create a dummy blade map texture
            let dummy_blade_map = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Foliage Dummy Blade Map"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg32Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &dummy_blade_map,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &[0u8; 8], // 2 × u32
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(8),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            let view = dummy_blade_map.create_view(&wgpu::TextureViewDescriptor::default());
            self.blade_map_texture = Some(dummy_blade_map);
            self.blade_map_view = Some(view);
            self.blade_map_view.as_ref().unwrap()
        };

        // Create the params bind group (includes blade map texture)
        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Foliage Params Bind Group"),
            layout: &self.params_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &chunk_info_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<ChunkInfoUniform>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&heightmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&heightmap_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&blackmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&unexplored_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(blade_map_view),
                },
            ],
        });

        self.params_bind_group = Some(params_bind_group);
        log::info!(
            "Created foliage params bind group with {}x{} heightmap",
            num_verts,
            num_verts
        );
    }

    /// Create a 1x1 dummy texture with the given RGBA pixel.
    fn create_dummy_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        pixel: &[u8; 4],
    ) -> wgpu::Texture {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("Foliage Dummy {} Texture", label)),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
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
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        tex
    }

    /// Align `value` up to the next multiple of `alignment`.
    fn align_up(value: u32, alignment: u32) -> u32 {
        (value + alignment - 1) & !(alignment - 1)
    }

    /// Parse a QN chunk's index buffer for one set, extracting (grid_position, blade_type) pairs.
    ///
    /// Index buffer format (from XTT_FoliageExport.cs):
    /// Each 32-bit entry: upper 16 bits = blade_type, lower 16 bits = localIndex * numVertsPerBlade + vert
    /// Entries for one blade are followed by a 0xFFFF strip reset marker.
    fn parse_index_buffer(
        ib_data: &[u8],
        num_verts_per_blade: u32,
    ) -> Vec<[u32; 2]> {
        let mut blade_entries = Vec::new();

        if ib_data.len() < 4 || num_verts_per_blade == 0 {
            return blade_entries;
        }

        // Index buffer is big-endian 32-bit integers (Xbox 360 format)
        let num_indices = ib_data.len() / 4;
        let mut i = 0;

        while i < num_indices {
            let packed = u32::from_be_bytes([
                ib_data[i * 4],
                ib_data[i * 4 + 1],
                ib_data[i * 4 + 2],
                ib_data[i * 4 + 3],
            ]);

            // Skip strip reset markers (0xFFFF in lower 16 bits or full 0x0000FFFF)
            if (packed & 0xFFFF) == 0xFFFF {
                i += 1;
                continue;
            }

            // unpack_4_16: blade_type = upper 16 bits, index_part = lower 16 bits
            let blade_type = packed >> 16;
            let index_part = packed & 0xFFFF;

            // index_part = localIndex * numVertsPerBlade + vertexInBlade
            let local_index = index_part / num_verts_per_blade;
            let vert_in_blade = index_part % num_verts_per_blade;

            // Only record on first vertex of each blade to avoid duplicates
            if vert_in_blade == 0 {
                blade_entries.push([local_index, blade_type]);
            }

            i += 1;
        }

        blade_entries
    }

    /// Build draw calls and chunk info buffer from QN chunk data.
    ///
    /// Parses index buffers to determine which blades are active and their types.
    /// Creates a blade map texture for the shader to look up blade placement.
    pub fn build_draw_calls(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        qn_chunks: &[crate::types::FoliageQNChunk],
        num_verts_per_axis: u32,
    ) {
        self.draw_calls.clear();

        if qn_chunks.is_empty() || self.set_resources.is_empty() {
            log::info!("No QN chunks or sets — no foliage draw calls");
            return;
        }

        let chunks_per_axis = num_verts_per_axis / 64;
        let aligned_slot = Self::align_up(
            std::mem::size_of::<ChunkInfoUniform>() as u32,
            self.min_offset_alignment,
        );

        // First pass: parse all index buffers to build blade data and count totals
        struct DrawInfo {
            grid_x: u32,
            grid_z: u32,
            set_idx: usize,
            num_verts_per_blade: u32,
            blades: Vec<[u32; 2]>, // (grid_position, blade_type)
        }
        let mut draw_infos: Vec<DrawInfo> = Vec::new();
        let mut total_blades = 0u32;

        for (qn_i, qn) in qn_chunks.iter().enumerate() {
            // Original game uses X-major grid: index = gridX * numXChunks + gridZ
            // So gridX = index / N, gridZ = index % N.
            // This gives mMinXVert = gridX * 64 → world X, mMinZVert = gridZ * 64 → world Z.
            let grid_x = qn.qn_parent_index / chunks_per_axis;
            let grid_z = qn.qn_parent_index % chunks_per_axis;

            if qn_i < 5 {
                log::debug!(
                    "  QN[{}] parent_idx={}, grid=({},{}), sets={}, set_indices={:?}",
                    qn_i, qn.qn_parent_index, grid_x, grid_z, qn.num_sets, qn.set_indices
                );
            }

            for set_slot in 0..qn.num_sets as usize {
                let set_idx = qn.set_indices[set_slot] as usize;
                if set_idx >= self.set_resources.len() {
                    continue;
                }

                let set_res = &self.set_resources[set_idx];
                let nvpb = set_res.num_verts_per_blade;

                // Parse index buffer for this set in this chunk
                let blades = if set_slot < qn.index_buffers.len() {
                    Self::parse_index_buffer(&qn.index_buffers[set_slot], nvpb)
                } else {
                    Vec::new()
                };

                if blades.is_empty() {
                    continue;
                }

                total_blades += blades.len() as u32;
                draw_infos.push(DrawInfo {
                    grid_x,
                    grid_z,
                    set_idx,
                    num_verts_per_blade: nvpb,
                    blades,
                });
            }
        }

        if draw_infos.is_empty() {
            log::info!("No valid foliage draw calls generated from index buffers");
            return;
        }

        // Build blade map texture data and chunk info buffer
        let buffer_size = (draw_infos.len() as u32) * aligned_slot;
        let mut chunk_data = vec![0u8; buffer_size as usize];
        // Blade map: each entry is [grid_position, blade_type] as u32 pair
        let mut blade_map_data: Vec<[u32; 2]> = Vec::with_capacity(total_blades as usize);
        let mut blade_data_offset = 0u32;

        for (draw_idx, info) in draw_infos.iter().enumerate() {
            let min_x_vert = (info.grid_x * 64) as f32;
            let min_z_vert = (info.grid_z * 64) as f32;
            let offset = (draw_idx as u32 * aligned_slot) as usize;

            let chunk_info = ChunkInfoUniform {
                chunk_offset: [min_x_vert, min_z_vert],
                num_verts_per_blade: info.num_verts_per_blade as f32,
                blade_data_offset: blade_data_offset as f32,
            };

            let bytes = bytemuck::bytes_of(&chunk_info);
            chunk_data[offset..offset + bytes.len()].copy_from_slice(bytes);

            let num_active = info.blades.len() as u32;
            blade_map_data.extend_from_slice(&info.blades);

            self.draw_calls.push(FoliageDrawCall {
                dynamic_offset: draw_idx as u32 * aligned_slot,
                set_index: info.set_idx,
                num_verts_per_blade: info.num_verts_per_blade,
                num_active_blades: num_active,
            });

            blade_data_offset += num_active;
        }

        // Create chunk info buffer
        let chunk_info_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foliage Chunk Info Buffer"),
            size: buffer_size as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&chunk_info_buffer, 0, &chunk_data);
        self.chunk_info_buffer = Some(chunk_info_buffer);

        // Create blade map texture (Rg32Uint, 2D with width up to 8192)
        let total_entries = blade_map_data.len() as u32;
        let tex_width = total_entries.min(8192).max(1);
        let tex_height = ((total_entries + tex_width - 1) / tex_width).max(1);
        // Pad to fill the texture
        blade_map_data.resize((tex_width * tex_height) as usize, [0, 0]);

        let blade_map_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Foliage Blade Map"),
            size: wgpu::Extent3d {
                width: tex_width,
                height: tex_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &blade_map_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&blade_map_data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tex_width * 8), // 2 × u32 = 8 bytes per texel
                rows_per_image: Some(tex_height),
            },
            wgpu::Extent3d {
                width: tex_width,
                height: tex_height,
                depth_or_array_layers: 1,
            },
        );
        let blade_map_view =
            blade_map_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.blade_map_texture = Some(blade_map_texture);
        self.blade_map_view = Some(blade_map_view);

        log::info!(
            "Built {} foliage draw calls from {} QN chunks ({} total blades, blade map {}x{}, grid {}x{})",
            self.draw_calls.len(),
            qn_chunks.len(),
            total_entries,
            tex_width,
            tex_height,
            chunks_per_axis,
            chunks_per_axis,
        );
    }

    /// Update camera position in foliage params (call each frame).
    pub fn update_camera(&self, queue: &wgpu::Queue, camera_pos: [f32; 3], time: f32) {
        // Update only the camera_pos_time field (5th vec4 in struct)
        // camera_pos_time is at offset 64 (4 × vec4 before it)
        let camera_pos_time = [camera_pos[0], camera_pos[1], camera_pos[2], time];
        queue.write_buffer(
            &self.params_buffer,
            64, // offset to camera_pos_time field
            bytemuck::bytes_of(&camera_pos_time),
        );
    }

    /// Create a heightmap texture from terrain position data.
    ///
    /// Stores full XYZ displacement (matching original `getTerrainDataAtPos`
    /// which returns all three components, not just Y height).
    fn create_heightmap_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_data: &crate::types::RawXtdData,
        num_verts: u32,
    ) -> wgpu::Texture {
        // Decode packed positions to get XYZ displacement values
        let mut positions: Vec<[f32; 4]> =
            Vec::with_capacity((num_verts * num_verts) as usize);

        let mid = terrain_data.mid;
        let range = terrain_data.range;

        let mut min_height = f32::MAX;
        let mut max_height = f32::MIN;

        for packed in &terrain_data.packed_positions {
            // R10G10B10A2 format: 10 bits each for X, Y, Z
            let x_raw = (packed & 0x3FF) as f32 / 1023.0;
            let y_raw = ((packed >> 10) & 0x3FF) as f32 / 1023.0;
            let z_raw = ((packed >> 20) & 0x3FF) as f32 / 1023.0;

            // Decode to world displacement: norm * range - mid (matches game bytecode)
            let x = x_raw * range[0] - mid[0];
            let y = y_raw * range[1] - mid[1];
            let z = z_raw * range[2] - mid[2];

            min_height = min_height.min(y);
            max_height = max_height.max(y);

            positions.push([x, y, z, 0.0]);
        }

        log::info!(
            "Heightmap: {} positions, height range [{:.1}, {:.1}], mid={:.1}, range_y={:.1}",
            positions.len(),
            min_height,
            max_height,
            mid[1],
            range[1]
        );

        // Create Rgba32Float texture for full XYZ displacement
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Foliage Heightmap"),
            size: wgpu::Extent3d {
                width: num_verts,
                height: num_verts,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
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
            bytemuck::cast_slice(&positions),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(num_verts * 16), // 4 floats × 4 bytes per f32
                rows_per_image: Some(num_verts),
            },
            wgpu::Extent3d {
                width: num_verts,
                height: num_verts,
                depth_or_array_layers: 1,
            },
        );

        texture
    }
}
