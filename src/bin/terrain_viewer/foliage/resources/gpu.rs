use num_traits::ToPrimitive;
use render::gpu::{
    buffer_layout_entry, buffer_layout_entry_with_options, sampler_layout_entry,
    texture_layout_entry,
};
use render::terrain::{FOLIAGE_SHADER, generate_mipmaps, mip_level_count};
use render::wgpu;

use super::{
    ChunkInfoUniform, FoliageConfig, FoliageParamsUniform, FoliageSet, FoliageSetResources,
};

pub(super) fn create_foliage_bind_group_layouts(
    device: &wgpu::Device,
) -> (wgpu::BindGroupLayout, wgpu::BindGroupLayout) {
    (
        create_foliage_params_layout(device),
        create_foliage_material_layout(device),
    )
}

fn create_foliage_params_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let both_stages = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
    let float_filterable = wgpu::TextureSampleType::Float { filterable: true };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Foliage Params Bind Group Layout"),
        entries: &[
            buffer_layout_entry(0, both_stages, wgpu::BufferBindingType::Uniform),
            buffer_layout_entry_with_options(
                1,
                wgpu::ShaderStages::VERTEX,
                wgpu::BufferBindingType::Uniform,
                true,
                wgpu::BufferSize::new(
                    u64::try_from(std::mem::size_of::<ChunkInfoUniform>())
                        .expect("chunk info size must fit u64"),
                ),
            ),
            texture_layout_entry(
                2,
                wgpu::ShaderStages::VERTEX,
                wgpu::TextureSampleType::Float { filterable: true },
                wgpu::TextureViewDimension::D2,
            ),
            sampler_layout_entry(
                3,
                wgpu::ShaderStages::VERTEX,
                wgpu::SamplerBindingType::Filtering,
            ),
            texture_layout_entry(
                4,
                wgpu::ShaderStages::FRAGMENT,
                float_filterable,
                wgpu::TextureViewDimension::D2Array,
            ),
            texture_layout_entry(
                5,
                wgpu::ShaderStages::FRAGMENT,
                float_filterable,
                wgpu::TextureViewDimension::D2,
            ),
            texture_layout_entry(
                6,
                wgpu::ShaderStages::FRAGMENT,
                float_filterable,
                wgpu::TextureViewDimension::D2,
            ),
            texture_layout_entry(
                7,
                wgpu::ShaderStages::VERTEX,
                wgpu::TextureSampleType::Uint,
                wgpu::TextureViewDimension::D2,
            ),
            buffer_layout_entry(
                8,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BufferBindingType::Storage { read_only: true },
            ),
        ],
    })
}

fn create_foliage_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let float_filterable = wgpu::TextureSampleType::Float { filterable: true };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Foliage Material Bind Group Layout"),
        entries: &[
            texture_layout_entry(
                0,
                wgpu::ShaderStages::FRAGMENT,
                float_filterable,
                wgpu::TextureViewDimension::D2,
            ),
            texture_layout_entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                float_filterable,
                wgpu::TextureViewDimension::D2,
            ),
            sampler_layout_entry(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::SamplerBindingType::Filtering,
            ),
            texture_layout_entry(
                3,
                wgpu::ShaderStages::VERTEX,
                wgpu::TextureSampleType::Float { filterable: false },
                wgpu::TextureViewDimension::D2,
            ),
            texture_layout_entry(
                4,
                wgpu::ShaderStages::VERTEX,
                wgpu::TextureSampleType::Float { filterable: false },
                wgpu::TextureViewDimension::D2,
            ),
            buffer_layout_entry(
                6,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BufferBindingType::Uniform,
            ),
            sampler_layout_entry(
                5,
                wgpu::ShaderStages::VERTEX,
                wgpu::SamplerBindingType::NonFiltering,
            ),
        ],
    })
}

pub(super) fn create_foliage_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    params_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Foliage Shader"),
        source: wgpu::ShaderSource::Wgsl(FOLIAGE_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Foliage Pipeline Layout"),
        bind_group_layouts: &[camera_layout, params_layout, material_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Foliage Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
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
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
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
    })
}

pub(super) fn create_foliage_shadow_pipeline(
    device: &wgpu::Device,
    camera_layout: &wgpu::BindGroupLayout,
    params_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Foliage Shadow Shader"),
        source: wgpu::ShaderSource::Wgsl(FOLIAGE_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Foliage Shadow Pipeline Layout"),
        bind_group_layouts: &[camera_layout, params_layout, material_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Foliage Shadow Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_shadow"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rg16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
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
    })
}

pub(super) fn create_uploaded_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: [u32; 2],
    pixels: &[u8],
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let [width, height] = size;
    let mip_levels = mip_level_count(width, height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mips = generate_mipmaps(pixels, width, height);
    let mut mip_width = width;
    let mut mip_height = height;
    for (mip_level, mip_pixels) in mips.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: u32::try_from(mip_level).expect("foliage mip level must fit u32"),
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            mip_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mip_width * 4),
                rows_per_image: Some(mip_height),
            },
            wgpu::Extent3d {
                width: mip_width,
                height: mip_height,
                depth_or_array_layers: 1,
            },
        );
        mip_width = (mip_width / 2).max(1);
        mip_height = (mip_height / 2).max(1);
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

pub(super) fn create_opacity_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    set: &FoliageSet,
) -> (wgpu::Texture, wgpu::TextureView) {
    if set.opacity_pixels.is_empty() {
        create_uploaded_rgba_texture(
            device,
            queue,
            &format!("Foliage Opacity (fallback): {}", set.name),
            [1, 1],
            &[255; 4],
            wgpu::TextureFormat::Rgba8Unorm,
        )
    } else {
        create_uploaded_rgba_texture(
            device,
            queue,
            &format!("Foliage Opacity: {}", set.name),
            [set.opacity_width, set.opacity_height],
            &set.opacity_pixels,
            wgpu::TextureFormat::Rgba8Unorm,
        )
    }
}

pub(super) fn create_foliage_samplers(device: &wgpu::Device) -> (wgpu::Sampler, wgpu::Sampler) {
    let material = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Foliage Material Sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let blade = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Foliage Blade Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    (material, blade)
}

pub(super) struct FoliageMaterialBindings<'a> {
    pub(super) albedo: &'a wgpu::TextureView,
    pub(super) opacity: &'a wgpu::TextureView,
    pub(super) material_sampler: &'a wgpu::Sampler,
    pub(super) blade_positions: &'a wgpu::TextureView,
    pub(super) blade_normals: &'a wgpu::TextureView,
    pub(super) blade_sampler: &'a wgpu::Sampler,
    pub(super) material_params: &'a wgpu::Buffer,
}

pub(super) fn create_foliage_material_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    set_name: &str,
    bindings: &FoliageMaterialBindings<'_>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("Foliage Material Bind Group: {set_name}")),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(bindings.albedo),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(bindings.opacity),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(bindings.material_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(bindings.blade_positions),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(bindings.blade_normals),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(bindings.blade_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: bindings.material_params.as_entire_binding(),
            },
        ],
    })
}

pub(super) fn initial_foliage_params(
    terrain_data: &crate::types::RawXtdData,
    first_set: &FoliageSetResources,
    config: &FoliageConfig,
) -> FoliageParamsUniform {
    FoliageParamsUniform {
        terrain_info: [
            terrain_data
                .num_verts_per_axis
                .to_f32()
                .expect("terrain vertex count must fit f32"),
            terrain_data.tile_scale,
            0.0,
            0.0,
        ],
        position_mid: [
            terrain_data.mid[2],
            terrain_data.mid[1],
            terrain_data.mid[0],
            0.0,
        ],
        position_range: [
            terrain_data.range[2],
            terrain_data.range[1],
            terrain_data.range[0],
            0.0,
        ],
        foliage_info: [
            first_set
                .num_verts_per_blade
                .to_f32()
                .expect("foliage vertex count must fit f32"),
            1.0 / 64.0,
            config.fade_start_distance,
            config.max_render_distance,
        ],
        camera_pos_time: [0.0, 100.0, 0.0, 0.0],
        dir_light_vec: [0.4472, 0.8944, 0.0, 1.0],
        dir_light_color: [1.0, 1.0, 1.0, 0.0],
        fog_params: [0.0; 4],
        fog_color: [0.7, 0.8, 0.9, 1.0],
        planar_fog_params: [0.0; 4],
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
        shadow_params: [8.0, 3.0, 0.0, 0.0],
        blackmap_params0: [0.0, 0.0, 0.0, 0.5],
        blackmap_params1: [0.3, 0.0, 0.0, 0.0],
        blackmap_params2: [0.0, 1024.0, 1024.0, 0.01],
        local_light_params: [0.0; 4],
        blackmap_uv_scales: [1.0 / 1024.0, 1.0 / 1024.0, 0.0, 0.0],
    }
}

pub(super) struct FoliageParamsBindings<'a> {
    pub(super) params_buffer: &'a wgpu::Buffer,
    pub(super) chunk_buffer: &'a wgpu::Buffer,
    pub(super) heightmap: &'a wgpu::TextureView,
    pub(super) heightmap_sampler: &'a wgpu::Sampler,
    pub(super) shadow: &'a wgpu::TextureView,
    pub(super) blackmap: &'a wgpu::TextureView,
    pub(super) unexplored: &'a wgpu::TextureView,
    pub(super) blade_map: &'a wgpu::TextureView,
    pub(super) local_lights: &'a wgpu::Buffer,
}

pub(super) fn create_foliage_params_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    bindings: &FoliageParamsBindings<'_>,
) -> wgpu::BindGroup {
    let chunk_info_size = wgpu::BufferSize::new(
        u64::try_from(std::mem::size_of::<ChunkInfoUniform>())
            .expect("chunk info size must fit u64"),
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Foliage Params Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: bindings.params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: bindings.chunk_buffer,
                    offset: 0,
                    size: chunk_info_size,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(bindings.heightmap),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(bindings.heightmap_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(bindings.shadow),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(bindings.blackmap),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(bindings.unexplored),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(bindings.blade_map),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: bindings.local_lights.as_entire_binding(),
            },
        ],
    })
}
