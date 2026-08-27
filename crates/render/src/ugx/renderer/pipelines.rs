use std::{collections::HashMap, mem};

use crate::gpu::{
    buffer_layout_entry, filtering_sampler_layout_entry,
    texture_layout_entry as shared_texture_layout_entry,
};
use crate::postprocess::DISTORTION_FORMAT;
use crate::ugx::model::{BlendMode, Vertex};

const DIRECTIONAL_SHADOW_CASCADE_SCALES: [f32; 4] = [8.0, 4.0, 2.0, 1.0];

pub(super) fn create_scene_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UGX Scene Layout"),
        entries: &[
            buffer_layout_entry(
                0,
                wgpu::ShaderStages::VERTEX_FRAGMENT,
                wgpu::BufferBindingType::Uniform,
            ),
            shared_texture_layout_entry(
                2,
                wgpu::ShaderStages::VERTEX,
                wgpu::TextureSampleType::Float { filterable: true },
                wgpu::TextureViewDimension::D2,
            ),
            buffer_layout_entry(
                1,
                wgpu::ShaderStages::VERTEX,
                wgpu::BufferBindingType::Storage { read_only: true },
            ),
        ],
    })
}

fn texture_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    shared_texture_layout_entry(
        binding,
        wgpu::ShaderStages::FRAGMENT,
        wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureViewDimension::D2,
    )
}

pub(super) fn create_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = vec![buffer_layout_entry(
        0,
        wgpu::ShaderStages::VERTEX_FRAGMENT,
        wgpu::BufferBindingType::Uniform,
    )];
    entries.extend((1..=7).map(texture_layout_entry));
    entries.push(filtering_sampler_layout_entry(
        8,
        wgpu::ShaderStages::FRAGMENT,
    ));
    entries.push(texture_layout_entry(9));
    entries.push(shared_texture_layout_entry(
        10,
        wgpu::ShaderStages::FRAGMENT,
        wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureViewDimension::Cube,
    ));
    entries.push(filtering_sampler_layout_entry(
        11,
        wgpu::ShaderStages::FRAGMENT,
    ));
    entries.extend((12..=15).map(texture_layout_entry));
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UGX Material Layout"),
        entries: &entries,
    })
}

pub(super) fn create_shadow_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let texture = |binding, view_dimension| {
        shared_texture_layout_entry(
            binding,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::TextureSampleType::Float { filterable: true },
            view_dimension,
        )
    };
    let sampler = |binding| filtering_sampler_layout_entry(binding, wgpu::ShaderStages::FRAGMENT);
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UGX World Lighting Layout"),
        entries: &[
            texture(0, wgpu::TextureViewDimension::D2Array),
            sampler(1),
            buffer_layout_entry(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BufferBindingType::Storage { read_only: true },
            ),
            texture(3, wgpu::TextureViewDimension::D2Array),
            sampler(4),
            texture(5, wgpu::TextureViewDimension::D3),
            texture(6, wgpu::TextureViewDimension::D3),
            sampler(7),
        ],
    })
}

pub(super) fn pipeline_index(blend: BlendMode, two_sided: bool) -> usize {
    usize::from(blend.rank()) * 2 + usize::from(two_sided)
}

pub(super) fn create_pipelines(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    scene_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
    shadow_layout: &wgpu::BindGroupLayout,
) -> Vec<wgpu::RenderPipeline> {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UGX Pipeline Layout"),
        bind_group_layouts: &[scene_layout, material_layout, shadow_layout],
        push_constant_ranges: &[],
    });
    BlendMode::DRAW_ORDER
        .into_iter()
        .flat_map(|blend| {
            [false, true].map(|two_sided| {
                create_pipeline(
                    device,
                    surface_format,
                    shader,
                    &pipeline_layout,
                    blend,
                    two_sided,
                )
            })
        })
        .collect()
}

pub(super) fn create_sky_pipelines(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    scene_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
    shadow_layout: &wgpu::BindGroupLayout,
) -> Vec<wgpu::RenderPipeline> {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UGX Sky Pipeline Layout"),
        bind_group_layouts: &[scene_layout, material_layout, shadow_layout],
        push_constant_ranges: &[],
    });
    BlendMode::DRAW_ORDER
        .into_iter()
        .flat_map(|blend| {
            [false, true].map(|two_sided| {
                create_material_pipeline(
                    device,
                    surface_format,
                    shader,
                    &pipeline_layout,
                    blend,
                    two_sided,
                    MaterialPipelineKind::Sky,
                )
            })
        })
        .collect()
}

pub(super) fn create_distortion_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    scene_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
) -> [wgpu::RenderPipeline; 2] {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UGX Distortion Pipeline Layout"),
        bind_group_layouts: &[scene_layout, material_layout],
        push_constant_ranges: &[],
    });
    [false, true]
        .map(|two_sided| create_distortion_pipeline(device, shader, &pipeline_layout, two_sided))
}

pub(super) fn create_shadow_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    scene_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
) -> Vec<wgpu::RenderPipeline> {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UGX Directional Shadow Pipeline Layout"),
        bind_group_layouts: &[scene_layout, material_layout],
        push_constant_ranges: &[],
    });
    DIRECTIONAL_SHADOW_CASCADE_SCALES
        .into_iter()
        .flat_map(|cascade_scale| {
            [false, true].map(|two_sided| {
                create_shadow_pipeline(device, shader, &pipeline_layout, cascade_scale, two_sided)
            })
        })
        .collect()
}

fn create_shadow_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    cascade_scale: f32,
    two_sided: bool,
) -> wgpu::RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x2,
        5 => Float32x2,
        6 => Float32x2,
        7 => Float32x4,
        8 => Uint32x4,
        9 => Float32x4
    ];
    let constants = HashMap::from([("shadow_cascade_scale".to_owned(), f64::from(cascade_scale))]);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("UGX Directional Shadow Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_shadow"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: u64::try_from(mem::size_of::<Vertex>())
                    .expect("UGX vertex stride must fit u64"),
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRIBUTES,
            }],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_shadow"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rg16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: (!two_sided).then_some(wgpu::Face::Back),
            ..Default::default()
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

fn create_distortion_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    two_sided: bool,
) -> wgpu::RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x2,
        5 => Float32x2,
        6 => Float32x2,
        7 => Float32x4,
        8 => Uint32x4,
        9 => Float32x4
    ];
    let additive = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("UGX Distortion Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: u64::try_from(mem::size_of::<Vertex>())
                    .expect("UGX vertex stride must fit u64"),
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRIBUTES,
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_distortion"),
            targets: &[Some(wgpu::ColorTargetState {
                format: DISTORTION_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: additive,
                    alpha: additive,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: (!two_sided).then_some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    blend: BlendMode,
    two_sided: bool,
) -> wgpu::RenderPipeline {
    create_material_pipeline(
        device,
        surface_format,
        shader,
        layout,
        blend,
        two_sided,
        MaterialPipelineKind::World,
    )
}

#[derive(Clone, Copy)]
enum MaterialPipelineKind {
    World,
    Sky,
}

fn create_material_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    blend: BlendMode,
    two_sided: bool,
    kind: MaterialPipelineKind,
) -> wgpu::RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x2,
        5 => Float32x2,
        6 => Float32x2,
        7 => Float32x4,
        8 => Uint32x4,
        9 => Float32x4
    ];
    let target_blend = match blend {
        BlendMode::Opaque | BlendMode::AlphaTest => None,
        BlendMode::Over => Some(wgpu::BlendState::ALPHA_BLENDING),
        BlendMode::Additive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(match kind {
            MaterialPipelineKind::World => "UGX Legacy Material Pipeline",
            MaterialPipelineKind::Sky => "UGX Sky Material Pipeline",
        }),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(match kind {
                MaterialPipelineKind::World => "vs_main",
                MaterialPipelineKind::Sky => "vs_sky",
            }),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: u64::try_from(mem::size_of::<Vertex>())
                    .expect("UGX vertex stride must fit u64"),
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRIBUTES,
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: target_blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: (!two_sided).then_some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: matches!(kind, MaterialPipelineKind::World)
                && matches!(blend, BlendMode::Opaque | BlendMode::AlphaTest),
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}
