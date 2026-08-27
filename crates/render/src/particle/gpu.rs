use std::collections::HashMap;
use std::mem;

use wgpu::util::DeviceExt;

use crate::gpu::texture_entry;

use super::{
    PackedParticleInstance, PackedParticleMaterial, ParticleBlendMode, ParticleError,
    ParticleImage, ParticleMaterial, ParticleSceneTextures, ParticleTextureArray,
};

pub(super) fn create_scene_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Particle Scene Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

pub(super) fn create_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = (0..4)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        })
        .collect::<Vec<_>>();
    entries.extend([
        wgpu::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 5,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 8,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ]);
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Particle Material Layout"),
        entries: &entries,
    })
}

pub(super) fn create_material_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    material: &ParticleMaterial,
    scene_textures: ParticleSceneTextures<'_>,
) -> Result<wgpu::BindGroup, ParticleError> {
    let fallback = ParticleImage::from_rgba(1, 1, vec![255; 4], 1.0)?;
    let fallback_array = ParticleTextureArray::new(vec![fallback])?;
    let views = material
        .diffuse
        .iter()
        .map(|texture| {
            create_texture_array_view(device, queue, texture.as_ref().unwrap_or(&fallback_array))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let intensity = create_texture_array_view(
        device,
        queue,
        material.intensity.as_ref().unwrap_or(&fallback_array),
    )?;
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Particle Texture Sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let fallback_light_volume = create_fallback_light_volume(device, queue);
    let light_volume = scene_textures
        .light_volume
        .unwrap_or(&fallback_light_volume);
    let light_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Particle Light Volume Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let packed = PackedParticleMaterial::from_material(material);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Particle Material Uniform"),
        contents: bytemuck::bytes_of(&packed),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Particle Material Bind Group"),
        layout,
        entries: &[
            texture_entry(0, &views[0]),
            texture_entry(1, &views[1]),
            texture_entry(2, &views[2]),
            texture_entry(3, &intensity),
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            texture_entry(5, scene_textures.depth),
            texture_entry(6, light_volume),
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(&light_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: uniform.as_entire_binding(),
            },
        ],
    }))
}

fn create_texture_array_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    array: &ParticleTextureArray,
) -> Result<wgpu::TextureView, ParticleError> {
    let first = array
        .layers
        .first()
        .ok_or(ParticleError::EmptyTextureArray)?;
    let layer_count =
        u32::try_from(array.layers.len()).map_err(|_| ParticleError::TooManyTextureLayers {
            actual: array.layers.len(),
        })?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Particle Texture Array"),
        size: wgpu::Extent3d {
            width: first.width,
            height: first.height,
            depth_or_array_layers: layer_count,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, image) in array.layers.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: u32::try_from(layer).map_err(|_| ParticleError::TooManyTextureLayers {
                        actual: array.layers.len(),
                    })?,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
    }
    Ok(texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Particle Texture Array View"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    }))
}

fn create_fallback_light_volume(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("Particle Fallback Light Volume"),
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
        &[0, 0, 0, 255],
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(super) fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let byte_size = capacity
        .checked_mul(mem::size_of::<PackedParticleInstance>())
        .and_then(|size| u64::try_from(size).ok())
        .unwrap_or(u64::MAX);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Particle Instance Buffer"),
        size: byte_size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn particle_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 11] = wgpu::vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
        9 => Uint32x4,
        10 => Uint32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<PackedParticleInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

pub(super) fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    fragment_entry: &str,
    blend: Option<wgpu::BlendState>,
    label: &str,
) -> wgpu::RenderPipeline {
    let constants = HashMap::new();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[particle_vertex_layout()],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                zero_initialize_workgroup_memory: false,
            },
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                zero_initialize_workgroup_memory: false,
            },
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
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

pub(super) fn color_blend_state(mode: ParticleBlendMode) -> Option<wgpu::BlendState> {
    match mode {
        ParticleBlendMode::Alpha => Some(wgpu::BlendState::ALPHA_BLENDING),
        ParticleBlendMode::Additive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        }),
        ParticleBlendMode::PremultipliedAlpha => {
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
        }
        ParticleBlendMode::Subtractive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::ReverseSubtract,
            },
            alpha: wgpu::BlendComponent::OVER,
        }),
        ParticleBlendMode::Distortion => None,
    }
}
