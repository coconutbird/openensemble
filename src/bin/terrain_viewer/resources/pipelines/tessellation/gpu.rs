use num_traits::ToPrimitive;
use render::terrain::GPU_TESS_SHADER;
use render::wgpu;
use wgpu::util::DeviceExt;

use super::{PatchMesh, TessellationBuildConfig};
use crate::types::RawXtdData;

use super::super::{
    buffer_entry, buffer_layout_entry, sampler_entry, sampler_layout_entry, texture_entry,
    texture_layout_entry,
};

pub(super) fn downsample_rgb10a2(values: &[u32], width: u32) -> Vec<u32> {
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

pub(super) fn create_rgb10a2_texture(
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

pub(super) fn create_mask_texture(
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

pub(super) fn create_dynamic_alpha_texture(
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

pub(super) fn create_gpu_texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let vertex = wgpu::ShaderStages::VERTEX;
    let fragment = wgpu::ShaderStages::FRAGMENT;
    let both = vertex | fragment;
    let filterable = wgpu::TextureSampleType::Float { filterable: true };
    let storage = wgpu::BufferBindingType::Storage { read_only: true };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("GPU Tess Texture Bind Group Layout"),
        entries: &[
            buffer_layout_entry(0, both, wgpu::BufferBindingType::Uniform),
            texture_layout_entry(1, vertex, filterable, wgpu::TextureViewDimension::D2),
            texture_layout_entry(2, both, filterable, wgpu::TextureViewDimension::D2),
            sampler_layout_entry(3, vertex),
            sampler_layout_entry(4, both),
            buffer_layout_entry(5, fragment, wgpu::BufferBindingType::Uniform),
            texture_layout_entry(6, fragment, filterable, wgpu::TextureViewDimension::D2),
            texture_layout_entry(7, both, filterable, wgpu::TextureViewDimension::D2),
            texture_layout_entry(14, fragment, filterable, wgpu::TextureViewDimension::D2),
            buffer_layout_entry(15, both, wgpu::BufferBindingType::Uniform),
            texture_layout_entry(
                16,
                fragment,
                filterable,
                wgpu::TextureViewDimension::D2Array,
            ),
            texture_layout_entry(17, fragment, filterable, wgpu::TextureViewDimension::D2),
            texture_layout_entry(18, fragment, filterable, wgpu::TextureViewDimension::D2),
            buffer_layout_entry(19, fragment, storage),
            sampler_layout_entry(20, fragment),
            texture_layout_entry(21, vertex, filterable, wgpu::TextureViewDimension::D2),
            texture_layout_entry(
                22,
                fragment,
                wgpu::TextureSampleType::Uint,
                wgpu::TextureViewDimension::D2,
            ),
            texture_layout_entry(23, fragment, filterable, wgpu::TextureViewDimension::D2),
            texture_layout_entry(24, fragment, filterable, wgpu::TextureViewDimension::D2),
            texture_layout_entry(
                25,
                fragment,
                filterable,
                wgpu::TextureViewDimension::D2Array,
            ),
            texture_layout_entry(26, fragment, filterable, wgpu::TextureViewDimension::D3),
            texture_layout_entry(27, fragment, filterable, wgpu::TextureViewDimension::D3),
        ],
    })
}

pub(super) fn create_gpu_pipeline(
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

pub(super) fn create_placeholder_view(
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

pub(super) fn create_placeholder_array_view(
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

pub(super) fn create_placeholder_volume_view(
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

pub(super) fn create_light_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: Option<(&[u8], u32, u32)>,
) -> wgpu::TextureView {
    let Some((pixels, width, height)) = source else {
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
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width.saturating_mul(4)),
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

pub(super) struct ShadowResourceBindings<'a> {
    pub(super) position: &'a wgpu::TextureView,
    pub(super) position_sampler: &'a wgpu::Sampler,
    pub(super) alpha: &'a wgpu::TextureView,
    pub(super) alpha_sampler: &'a wgpu::Sampler,
    pub(super) dynamic_alpha: &'a wgpu::TextureView,
    pub(super) camera_layout: &'a wgpu::BindGroupLayout,
    pub(super) num_patches: u32,
}

pub(super) fn create_shadow_resources(
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

pub(super) struct GpuTessBindings<'a> {
    pub(super) tess_params: &'a wgpu::Buffer,
    pub(super) position: &'a wgpu::TextureView,
    pub(super) normal: &'a wgpu::TextureView,
    pub(super) position_sampler: &'a wgpu::Sampler,
    pub(super) terrain_sampler: &'a wgpu::Sampler,
    pub(super) params: &'a wgpu::Buffer,
    pub(super) ao: &'a wgpu::TextureView,
    pub(super) alpha: &'a wgpu::TextureView,
    pub(super) composited_albedo: &'a wgpu::TextureView,
    pub(super) lighting: &'a wgpu::Buffer,
    pub(super) shadow: &'a wgpu::TextureView,
    pub(super) blackmap: &'a wgpu::TextureView,
    pub(super) unexplored: &'a wgpu::TextureView,
    pub(super) local_lights: &'a wgpu::Buffer,
    pub(super) lighting_sampler: &'a wgpu::Sampler,
    pub(super) light: &'a wgpu::TextureView,
    pub(super) dynamic_alpha: &'a wgpu::TextureView,
    pub(super) composited_normal: &'a wgpu::TextureView,
    pub(super) composited_specular: &'a wgpu::TextureView,
    pub(super) local_shadow: &'a wgpu::TextureView,
    pub(super) light_volume_color: &'a wgpu::TextureView,
    pub(super) light_volume_vector: &'a wgpu::TextureView,
}

pub(super) fn create_gpu_texture_bind_group(
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

pub(super) fn create_expanded_patch_buffer(
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

pub(super) fn log_tessellation_resources(
    config: TessellationBuildConfig,
    vertices_per_patch: usize,
) {
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
