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

/// Create road GPU resources.
#[allow(clippy::too_many_arguments)]
pub fn create_road_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    camera_bind_group_layout: &wgpu::BindGroupLayout,
    surface_format: wgpu::TextureFormat,
    positions: &[[f32; 3]],
    uvs: &[[f32; 2]],
    albedo_pixels: &[u8],
    tex_width: u32,
    tex_height: u32,
) -> RoadResources {
    // Interleave vertex data: [pos.x, pos.y, pos.z, uv.x, uv.y]
    let mut vertex_data = Vec::with_capacity(positions.len() * 5);
    for i in 0..positions.len() {
        vertex_data.extend_from_slice(&positions[i]);
        vertex_data.extend_from_slice(&uvs[i]);
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Road Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertex_data),
        usage: wgpu::BufferUsages::VERTEX,
    });

    // Create road albedo texture
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Road Albedo"),
        size: wgpu::Extent3d {
            width: tex_width,
            height: tex_height,
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
        albedo_pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(tex_width * 4),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: tex_width,
            height: tex_height,
            depth_or_array_layers: 1,
        },
    );
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Road Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // Texture bind group layout
    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

    let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Road Texture BG"),
        layout: &texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    // Simple road shader (just textured triangles)
    let shader_source = ROAD_SIMPLE_SHADER;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Road Shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Road Pipeline Layout"),
        bind_group_layouts: &[camera_bind_group_layout, &texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Road Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: (5 * std::mem::size_of::<f32>()) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    // position
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    },
                    // uv
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 12,
                        shader_location: 1,
                    },
                ],
            }],
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
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None, // Roads can be seen from both sides
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                constant: -2, // Slight depth bias to render on top of terrain
                slope_scale: -1.0,
                clamp: 0.0,
            },
        }),
        multisample: Default::default(),
        multiview: None,
        cache: None,
    });

    log::info!(
        "Created road resources: {} vertices, {}x{} texture",
        positions.len(),
        tex_width,
        tex_height
    );

    RoadResources {
        pipeline,
        vertex_buffer,
        vertex_count: positions.len() as u32,
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
const ROAD_SIMPLE_SHADER: &str = r#"
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
"#;
