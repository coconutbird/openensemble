//! Road rendering for the terrain viewer.
//!
//! Renders pre-tessellated road geometry on top of terrain.
//! Roads are triangle lists with per-vertex position and UV,
//! textured with albedo from DDX files.

use render::wgpu;
use render::wgpu::util::DeviceExt;

/// GPU resources for road rendering.
pub struct RoadResources {
    /// Render pipeline for roads.
    pub pipeline: wgpu::RenderPipeline,
    /// Vertex buffer (interleaved position + UV).
    pub vertex_buffer: wgpu::Buffer,
    /// Number of vertices to draw.
    pub vertex_count: u32,
    /// Bind group for road textures.
    pub texture_bind_group: wgpu::BindGroup,
}

/// Inputs used to create road GPU resources.
pub struct RoadResourceInput<'a> {
    pub camera_bind_group_layout: &'a wgpu::BindGroupLayout,
    pub surface_format: wgpu::TextureFormat,
    pub positions: &'a [[f32; 3]],
    pub uvs: &'a [[f32; 2]],
    pub albedo_pixels: &'a [u8],
    pub texture_size: [u32; 2],
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

fn create_road_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> (wgpu::TextureView, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Road Albedo"),
        size: wgpu::Extent3d {
            width,
            height,
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
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Road Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (view, sampler)
}

fn create_road_texture_bindings(
    device: &wgpu::Device,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Road Texture BGL"),
        entries: &[
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
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Road Texture BG"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (layout, bind_group)
}

fn create_road_pipeline(
    device: &wgpu::Device,
    camera_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Road Shader"),
        source: wgpu::ShaderSource::Wgsl(ROAD_SIMPLE_SHADER.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Road Pipeline Layout"),
        bind_group_layouts: &[camera_layout, texture_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Road Pipeline"),
        layout: Some(&pipeline_layout),
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

/// Create road GPU resources.
pub fn create_road_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    input: &RoadResourceInput<'_>,
) -> RoadResources {
    let [texture_width, texture_height] = input.texture_size;
    let vertex_buffer = create_road_vertex_buffer(device, input.positions, input.uvs);
    let (texture_view, sampler) = create_road_texture(
        device,
        queue,
        input.albedo_pixels,
        texture_width,
        texture_height,
    );
    let (texture_layout, texture_bind_group) =
        create_road_texture_bindings(device, &texture_view, &sampler);
    let pipeline = create_road_pipeline(
        device,
        input.camera_bind_group_layout,
        &texture_layout,
        input.surface_format,
    );
    let vertex_count =
        u32::try_from(input.positions.len()).expect("road vertex count must fit u32");
    log::info!(
        "Created road resources: {vertex_count} vertices, {texture_width}x{texture_height} texture"
    );

    RoadResources {
        pipeline,
        vertex_buffer,
        vertex_count,
        texture_bind_group,
    }
}

/// Render roads in a render pass.
pub fn render_roads(
    render_pass: &mut wgpu::RenderPass<'_>,
    roads: &RoadResources,
    camera_bind_group: &wgpu::BindGroup,
) {
    render_pass.set_pipeline(&roads.pipeline);
    render_pass.set_bind_group(0, camera_bind_group, &[]);
    render_pass.set_bind_group(1, &roads.texture_bind_group, &[]);
    render_pass.set_vertex_buffer(0, roads.vertex_buffer.slice(..));
    render_pass.draw(0..roads.vertex_count, 0..1);
}

/// Simple road shader: textured triangles with camera transform.
const ROAD_SIMPLE_SHADER: &str = r"
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var t_albedo: texture_2d<f32>;
@group(1) @binding(1)
var s_albedo: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(position, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_albedo, s_albedo, in.uv);
    // Discard nearly transparent pixels
    if color.a < 0.1 {
        discard;
    }
    return color;
}
";
