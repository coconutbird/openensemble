//! Oracle-aligned terrain road rendering.

use num_traits::ToPrimitive;
use render::terrain::{LightingParams, ROADS_SHADER, RawXtdData};
use render::wgpu;
use render::wgpu::util::DeviceExt;

/// Per-material road geometry and textures.
pub struct RoadBatch {
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    material_bind_group: wgpu::BindGroup,
}

/// GPU resources shared by every road material.
pub struct RoadResources {
    pipeline: wgpu::RenderPipeline,
    params_buffer: wgpu::Buffer,
    world_bind_group: wgpu::BindGroup,
    batches: Vec<RoadBatch>,
    params: RoadParamsUniform,
}

/// Borrowed data for one road material.
pub struct RoadBatchInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub uvs: &'a [[f32; 2]],
    pub albedo_pixels: &'a [u8],
    pub normal_pixels: &'a [u8],
    pub specular_pixels: &'a [u8],
    pub texture_size: [u32; 2],
}

/// Inputs used to create all road GPU resources.
pub struct RoadResourceInput<'a> {
    pub camera_bind_group_layout: &'a wgpu::BindGroupLayout,
    pub surface_format: wgpu::TextureFormat,
    pub raw_terrain: &'a RawXtdData,
    pub shadow_view: Option<&'a wgpu::TextureView>,
    pub batches: &'a [RoadBatchInput<'a>],
    pub bump_power: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct RoadParamsUniform {
    terrain_values: [f32; 4],
    position_mid: [f32; 4],
    position_range: [f32; 4],
    camera_position: [f32; 4],
    dir_light_vector: [f32; 4],
    dir_light_color: [f32; 4],
    sh_fill_ar: [f32; 4],
    sh_fill_ag: [f32; 4],
    sh_fill_ab: [f32; 4],
    sh_fill_br: [f32; 4],
    sh_fill_bg: [f32; 4],
    sh_fill_bb: [f32; 4],
    sh_fill_c: [f32; 4],
    fog_color: [f32; 4],
    fog_params: [f32; 4],
    planar_fog_color: [f32; 4],
    planar_fog_params: [f32; 4],
    material_params: [f32; 4],
    shadow_vp_col0: [f32; 4],
    shadow_vp_col1: [f32; 4],
    shadow_vp_col2: [f32; 4],
    shadow_vp_col3: [f32; 4],
    shadow_params: [f32; 4],
}

impl RoadParamsUniform {
    fn new(raw: &RawXtdData, bump_power: f32) -> Self {
        let lighting = LightingParams::default();
        let mut params = Self {
            terrain_values: [
                1.0 / raw
                    .num_verts_per_axis
                    .to_f32()
                    .expect("terrain vertex count must fit f32"),
                raw.tile_scale,
                0.0,
                0.0,
            ],
            position_mid: [raw.mid[0], raw.mid[1], raw.mid[2], 0.0],
            position_range: [raw.range[0], raw.range[1], raw.range[2], 0.0],
            camera_position: [0.0; 4],
            dir_light_vector: [0.0; 4],
            dir_light_color: [0.0; 4],
            sh_fill_ar: [0.0; 4],
            sh_fill_ag: [0.0; 4],
            sh_fill_ab: [0.0; 4],
            sh_fill_br: [0.0; 4],
            sh_fill_bg: [0.0; 4],
            sh_fill_bb: [0.0; 4],
            sh_fill_c: [0.0; 4],
            fog_color: [0.0; 4],
            fog_params: [0.0; 4],
            planar_fog_color: [0.0; 4],
            planar_fog_params: [0.0; 4],
            material_params: [bump_power, lighting.local_light_params[1], 0.0, 0.0],
            shadow_vp_col0: [0.0; 4],
            shadow_vp_col1: [0.0; 4],
            shadow_vp_col2: [0.0; 4],
            shadow_vp_col3: [0.0; 4],
            shadow_params: [0.0; 4],
        };
        params.apply_lighting(&lighting, bump_power);
        params
    }

    fn apply_lighting(&mut self, lighting: &LightingParams, bump_power: f32) {
        self.camera_position = lighting.world_camera_pos;
        self.dir_light_vector = lighting.dir_light_vec;
        self.dir_light_color = lighting.dir_light_color;
        self.sh_fill_ar = lighting.sh_fill_ar;
        self.sh_fill_ag = lighting.sh_fill_ag;
        self.sh_fill_ab = lighting.sh_fill_ab;
        self.sh_fill_br = lighting.sh_fill_br;
        self.sh_fill_bg = lighting.sh_fill_bg;
        self.sh_fill_bb = lighting.sh_fill_bb;
        self.sh_fill_c = lighting.sh_fill_c;
        self.fog_color = lighting.fog_color;
        self.fog_params = lighting.fog_params;
        self.planar_fog_color = lighting.planar_fog_color;
        self.planar_fog_params = lighting.planar_fog_params;
        self.material_params = [bump_power, lighting.local_light_params[1], 0.0, 0.0];
        self.shadow_vp_col0 = lighting.shadow_vp_col0;
        self.shadow_vp_col1 = lighting.shadow_vp_col1;
        self.shadow_vp_col2 = lighting.shadow_vp_col2;
        self.shadow_vp_col3 = lighting.shadow_vp_col3;
        self.shadow_params = lighting.shadow_params;
    }
}

fn create_road_vertex_buffer(
    device: &wgpu::Device,
    positions: &[[f32; 3]],
    uvs: &[[f32; 2]],
) -> wgpu::Buffer {
    assert_eq!(
        positions.len(),
        uvs.len(),
        "road positions and UVs must align"
    );
    let mut vertex_data = Vec::with_capacity(positions.len() * 5);
    for (position, uv) in positions.iter().zip(uvs) {
        vertex_data.extend_from_slice(position);
        vertex_data.extend_from_slice(uv);
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Road Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertex_data),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

struct TextureUpload<'a> {
    label: &'a str,
    pixels: &'a [u8],
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

fn create_material_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    upload: &TextureUpload<'_>,
) -> wgpu::TextureView {
    let [width, height] = upload.size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(upload.label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: upload.format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        upload.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_packed_terrain_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    words: &[u32],
    size: u32,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgb10a2Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        bytemuck::cast_slice(words),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * 4),
            rows_per_image: Some(size),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_fallback_shadow_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Road Fallback Shadow Array"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 4,
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
        &[255; 16],
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

fn texture_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: dimension,
            multisampled: false,
        },
        count: None,
    }
}

fn create_world_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Road World Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            texture_layout_entry(
                1,
                wgpu::ShaderStages::VERTEX,
                wgpu::TextureViewDimension::D2,
            ),
            texture_layout_entry(
                2,
                wgpu::ShaderStages::VERTEX,
                wgpu::TextureViewDimension::D2,
            ),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            texture_layout_entry(
                4,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureViewDimension::D2Array,
            ),
        ],
    })
}

fn create_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Road Material Layout"),
        entries: &[
            texture_layout_entry(
                0,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureViewDimension::D2,
            ),
            texture_layout_entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureViewDimension::D2,
            ),
            texture_layout_entry(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureViewDimension::D2,
            ),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    camera_layout: &wgpu::BindGroupLayout,
    world_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Oracle Terrain Road Shader"),
        source: wgpu::ShaderSource::Wgsl(ROADS_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Road Pipeline Layout"),
        bind_group_layouts: &[camera_layout, world_layout, material_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Road Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 20,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 12,
                        shader_location: 1,
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
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: -2,
                slope_scale: -1.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn create_material_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    input: &RoadBatchInput<'_>,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let albedo = create_material_texture(
        device,
        queue,
        &TextureUpload {
            label: "Road Albedo",
            pixels: input.albedo_pixels,
            size: input.texture_size,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
        },
    );
    let normal = create_material_texture(
        device,
        queue,
        &TextureUpload {
            label: "Road Normal",
            pixels: input.normal_pixels,
            size: input.texture_size,
            format: wgpu::TextureFormat::Rgba8Unorm,
        },
    );
    let specular = create_material_texture(
        device,
        queue,
        &TextureUpload {
            label: "Road Specular",
            pixels: input.specular_pixels,
            size: input.texture_size,
            format: wgpu::TextureFormat::Rgba8Unorm,
        },
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Road Material Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&albedo),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&normal),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&specular),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Create road GPU resources for every material batch.
#[must_use]
pub fn create_road_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    input: &RoadResourceInput<'_>,
) -> RoadResources {
    let params = RoadParamsUniform::new(input.raw_terrain, input.bump_power);
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Road Params Buffer"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let terrain_position = create_packed_terrain_texture(
        device,
        queue,
        "Road Terrain Position",
        &input.raw_terrain.packed_positions,
        input.raw_terrain.num_verts_per_axis,
    );
    let terrain_basis = create_packed_terrain_texture(
        device,
        queue,
        "Road Terrain Basis",
        &input.raw_terrain.packed_normals,
        input.raw_terrain.num_verts_per_axis,
    );
    let terrain_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Road Terrain Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let fallback_shadow;
    let shadow_view = if let Some(view) = input.shadow_view {
        view
    } else {
        fallback_shadow = create_fallback_shadow_view(device, queue);
        &fallback_shadow
    };
    let world_layout = create_world_layout(device);
    let world_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Road World Bind Group"),
        layout: &world_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&terrain_position),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&terrain_basis),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&terrain_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
        ],
    });

    let material_layout = create_material_layout(device);
    let material_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Road Material Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let batches = input
        .batches
        .iter()
        .map(|batch| RoadBatch {
            vertex_buffer: create_road_vertex_buffer(device, batch.positions, batch.uvs),
            vertex_count: u32::try_from(batch.positions.len())
                .expect("road vertex count must fit u32"),
            material_bind_group: create_material_bind_group(
                device,
                queue,
                &material_layout,
                batch,
                &material_sampler,
            ),
        })
        .collect();
    let pipeline = create_pipeline(
        device,
        input.camera_bind_group_layout,
        &world_layout,
        &material_layout,
        input.surface_format,
    );
    RoadResources {
        pipeline,
        params_buffer,
        world_bind_group,
        batches,
        params,
    }
}

impl RoadResources {
    /// Update the shared road lighting and material constants.
    pub fn update_frame(
        &mut self,
        queue: &wgpu::Queue,
        lighting: &LightingParams,
        bump_power: f32,
    ) {
        self.params.apply_lighting(lighting, bump_power);
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&self.params));
    }
}

/// Render every road material batch.
pub fn render_roads(
    render_pass: &mut wgpu::RenderPass<'_>,
    roads: &RoadResources,
    camera_bind_group: &wgpu::BindGroup,
) {
    render_pass.set_pipeline(&roads.pipeline);
    render_pass.set_bind_group(0, camera_bind_group, &[]);
    render_pass.set_bind_group(1, &roads.world_bind_group, &[]);
    for batch in &roads.batches {
        render_pass.set_bind_group(2, &batch.material_bind_group, &[]);
        render_pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
        render_pass.draw(0..batch.vertex_count, 0..1);
    }
}
