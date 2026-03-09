//! GPU resources for foliage rendering.

use super::FoliageConfig;
#[allow(unused_imports)]
use crate::types::FoliageSet;
use render::terrain::FOLIAGE_SHADER;
use render::wgpu;

/// GPU resources for foliage rendering.
pub struct FoliageResources {
    /// Render pipeline for foliage.
    pub pipeline: wgpu::RenderPipeline,
    /// Uniform buffer for foliage parameters.
    pub params_buffer: wgpu::Buffer,
    /// Bind group layout for foliage params and geometry textures.
    pub params_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group layout for foliage material textures.
    pub material_bind_group_layout: wgpu::BindGroupLayout,
    /// Per-set resources (textures, bind groups).
    pub set_resources: Vec<FoliageSetResources>,
    /// Params bind group (geometry textures + heightmap).
    pub params_bind_group: Option<wgpu::BindGroup>,
    /// Current configuration.
    pub config: FoliageConfig,
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
    pub blade_positions_view: wgpu::TextureView,
    /// Blade normals texture (xyz = normal, w = v).
    pub _blade_normals_texture: wgpu::Texture,
    pub blade_normals_view: wgpu::TextureView,
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

impl FoliageResources {
    /// Create foliage rendering resources.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        // Create bind group layouts
        let params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Foliage Params Bind Group Layout"),
                entries: &[
                    // Params uniform
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
                    // Blade positions texture (Rgba32Float - not filterable)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // Blade normals texture (Rgba32Float - not filterable)
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
                    // Blade sampler (non-filtering for float32 textures)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    // Heightmap texture (R32Float - not filterable)
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
                    // Heightmap sampler (non-filtering for float32 texture)
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    // Shadow map texture (binding 6)
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
                    // Blackmap texture (binding 7)
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // Unexplored mask texture (binding 8)
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Foliage Material Bind Group Layout"),
                entries: &[
                    // Albedo texture
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
                    // Opacity texture
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
                    // Foliage sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
            params_bind_group_layout,
            material_bind_group_layout,
            set_resources: Vec::new(),
            params_bind_group: None,
            config: FoliageConfig::default(),
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
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
        let (blade_positions, blade_normals, num_blade_types, num_verts) =
            if !set.blade_positions.is_empty() {
                (
                    set.blade_positions.clone(),
                    set.blade_normals.clone(),
                    set.num_blade_types,
                    set.num_verts_per_blade,
                )
            } else {
                // Generate default grass blade geometry
                Self::generate_default_blade_geometry()
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

        // Create material sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Foliage Material Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create material bind group
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
            ],
        });

        Some(FoliageSetResources {
            _albedo_texture: albedo_texture,
            _albedo_view: albedo_view,
            _opacity_texture: opacity_texture,
            _opacity_view: opacity_view,
            _blade_positions_texture: blade_positions_texture,
            blade_positions_view,
            _blade_normals_texture: blade_normals_texture,
            blade_normals_view,
            material_bind_group,
            num_blade_types,
            num_verts_per_blade: num_verts,
        })
    }

    /// Generate default grass blade geometry (simple quad strip).
    fn generate_default_blade_geometry() -> (Vec<[f32; 4]>, Vec<[f32; 4]>, u32, u32) {
        let num_verts = 10u32;
        let num_blade_types = 1u32;

        let mut positions = Vec::with_capacity(num_verts as usize);
        let mut normals = Vec::with_capacity(num_verts as usize);

        // Simple grass blade: 5 quads stacked vertically
        // Width tapers from base to top
        let blade_height = 1.5;
        let base_width = 0.1;

        for i in 0..5 {
            let t = i as f32 / 4.0;
            let y = t * blade_height;
            let width = base_width * (1.0 - t * 0.8); // Taper to 20% at top
            let _u = t;

            // Left vertex
            positions.push([-width, y, 0.0, 0.0]); // x, y, z, u
            normals.push([0.0, 0.0, 1.0, 1.0 - t]); // nx, ny, nz, v

            // Right vertex
            positions.push([width, y, 0.0, 1.0]); // x, y, z, u
            normals.push([0.0, 0.0, 1.0, 1.0 - t]); // nx, ny, nz, v
        }

        (positions, normals, num_blade_types, num_verts)
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

    /// Create the params bind group with heightmap and blade geometry textures.
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

        // Create samplers
        let blade_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Foliage Blade Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let heightmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Foliage Heightmap Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest, // R32Float doesn't support linear filtering
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Use blade textures from first set
        let first_set = &self.set_resources[0];

        // Write initial params uniform (camera_pos will be updated per frame)
        let params = FoliageParamsUniform {
            terrain_info: [
                num_verts as f32,
                terrain_data.tile_scale,
                0.0, // chunk_offset_x - will be updated per chunk
                0.0, // chunk_offset_z
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
                1.0 / 64.0, // rcp_num_blades (64x64 grid per chunk)
                self.config.fade_start_distance,
                self.config.max_render_distance,
            ],
            camera_pos_time: [0.0, 100.0, 0.0, 0.0], // Initial camera pos, time
            // Default directional light (sun-like, from upper-right)
            dir_light_vec: [0.4472, 0.8944, 0.0, 1.0], // xyz = normalized light dir, w = backside_shadow_scalar
            dir_light_color: [1.0, 1.0, 1.0, 0.0],
            // Fog defaults (disabled)
            fog_params: [0.0, 0.0, 0.0, 0.0],
            fog_color: [0.7, 0.8, 0.9, 1.0],
            planar_fog_params: [0.0, 0.0, 0.0, 0.0], // x=0 means disabled
            planar_fog_color: [0.7, 0.8, 0.9, 1.0],
            // SH fill lighting defaults (uniform ambient ~0.3)
            sh_fill_ar: [0.0, 0.0, 0.0, 0.3],
            sh_fill_ag: [0.0, 0.0, 0.0, 0.3],
            sh_fill_ab: [0.0, 0.0, 0.0, 0.3],
            sh_fill_br: [0.0; 4],
            sh_fill_bg: [0.0; 4],
            sh_fill_bb: [0.0; 4],
            sh_fill_c: [0.0; 4],
            // Shadow disabled by default
            shadow_vp_col0: [1.0, 0.0, 0.0, 0.0],
            shadow_vp_col1: [0.0, 1.0, 0.0, 0.0],
            shadow_vp_col2: [0.0, 0.0, 1.0, 0.0],
            shadow_vp_col3: [0.0, 0.0, 0.0, 1.0],
            shadow_params: [1.0, 1.0, 0.0, 0.0], // z=0 => disabled
            // Blackmap disabled by default
            blackmap_params0: [0.0, 0.0, 0.0, 0.5],
            blackmap_params1: [0.3, 0.0, 0.0, 0.0], // w=0 => disabled
            blackmap_params2: [0.0, 1024.0, 1024.0, 0.01],
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        // Create placeholder shadow/blackmap textures (1x1 dummy)
        let dummy_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Foliage Dummy Shadow/Blackmap Texture"),
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
        // White pixel for shadow map (fully lit)
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dummy_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 255, 255],
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
        let shadow_view = dummy_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let dummy_black_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Foliage Dummy Blackmap Texture"),
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
                texture: &dummy_black_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8, 0, 0, 0], // Black / zero alpha (fully visible)
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
        let blackmap_view = dummy_black_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let unexplored_view = dummy_black_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Create the params bind group
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
                    resource: wgpu::BindingResource::TextureView(&first_set.blade_positions_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&first_set.blade_normals_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&blade_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&heightmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&heightmap_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&blackmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&unexplored_view),
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
    fn create_heightmap_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_data: &crate::types::RawXtdData,
        num_verts: u32,
    ) -> wgpu::Texture {
        // Decode packed positions to get Y (height) values
        let mut heights: Vec<f32> = Vec::with_capacity((num_verts * num_verts) as usize);

        let mid = terrain_data.mid;
        let range = terrain_data.range;

        let mut min_height = f32::MAX;
        let mut max_height = f32::MIN;

        for packed in &terrain_data.packed_positions {
            // R10G10B10A2 format: 10 bits each for X, Y, Z
            let _x_raw = (packed & 0x3FF) as f32 / 1023.0;
            let y_raw = ((packed >> 10) & 0x3FF) as f32 / 1023.0;
            let _z_raw = ((packed >> 20) & 0x3FF) as f32 / 1023.0;

            // Decode to world position: norm * range - mid (matches game bytecode)
            let y = y_raw * range[1] - mid[1];

            min_height = min_height.min(y);
            max_height = max_height.max(y);

            heights.push(y);
        }

        log::info!(
            "Heightmap: {} heights, range [{:.1}, {:.1}], mid={:.1}, range_y={:.1}",
            heights.len(),
            min_height,
            max_height,
            mid[1],
            range[1]
        );

        // Create R32Float texture for heightmap
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
            format: wgpu::TextureFormat::R32Float,
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
            bytemuck::cast_slice(&heights),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(num_verts * 4), // 4 bytes per f32
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
