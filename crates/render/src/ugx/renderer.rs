use std::mem;

use glam::Mat4;
use wgpu::util::DeviceExt;

use super::model::{BlendMode, Image, Material, Model, Vertex};
use crate::terrain::{LightingParams, generate_mipmaps, mip_level_count};

const SHADER: &str = include_str!("shader.wgsl");
const MATERIAL_FLAG_DIFFUSE: u32 = 1 << 0;
const MATERIAL_FLAG_NORMAL: u32 = 1 << 1;
const MATERIAL_FLAG_GLOSS: u32 = 1 << 2;
const MATERIAL_FLAG_OPACITY: u32 = 1 << 3;
const MATERIAL_FLAG_XFORM: u32 = 1 << 4;
const MATERIAL_FLAG_EMISSIVE: u32 = 1 << 5;
const MATERIAL_FLAG_AO: u32 = 1 << 6;
const MATERIAL_FLAG_COLOR_GLOSS: u32 = 1 << 7;
const MATERIAL_FLAG_TWO_SIDED: u32 = 1 << 8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneUniform {
    view_projection: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
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
    ao_params: [f32; 4],
}

impl SceneUniform {
    fn new(model: Mat4) -> Self {
        Self::from_frame(Mat4::IDENTITY, model, &LightingParams::default())
    }

    fn from_frame(view_projection: Mat4, model: Mat4, lighting: &LightingParams) -> Self {
        Self {
            view_projection: view_projection.to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            camera_position: lighting.world_camera_pos,
            dir_light_vector: lighting.dir_light_vec,
            dir_light_color: lighting.dir_light_color,
            sh_fill_ar: lighting.sh_fill_ar,
            sh_fill_ag: lighting.sh_fill_ag,
            sh_fill_ab: lighting.sh_fill_ab,
            sh_fill_br: lighting.sh_fill_br,
            sh_fill_bg: lighting.sh_fill_bg,
            sh_fill_bb: lighting.sh_fill_bb,
            sh_fill_c: lighting.sh_fill_c,
            fog_color: lighting.fog_color,
            fog_params: lighting.fog_params,
            planar_fog_color: lighting.planar_fog_color,
            planar_fog_params: lighting.planar_fog_params,
            ao_params: lighting.ao_params,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
    tint: [f32; 4],
    specular: [f32; 4],
    params: [f32; 4],
    flags: [u32; 4],
    channels0: [u32; 4],
    channels1: [u32; 4],
}

impl MaterialUniform {
    fn from_material(material: &Material) -> Self {
        let mut flags = 0;
        for (enabled, flag) in [
            (material.diffuse.is_some(), MATERIAL_FLAG_DIFFUSE),
            (material.normal.is_some(), MATERIAL_FLAG_NORMAL),
            (material.gloss.is_some(), MATERIAL_FLAG_GLOSS),
            (material.opacity_map.is_some(), MATERIAL_FLAG_OPACITY),
            (material.xform.is_some(), MATERIAL_FLAG_XFORM),
            (material.emissive.is_some(), MATERIAL_FLAG_EMISSIVE),
            (material.ao.is_some(), MATERIAL_FLAG_AO),
            (material.color_gloss, MATERIAL_FLAG_COLOR_GLOSS),
            (material.two_sided, MATERIAL_FLAG_TWO_SIDED),
        ] {
            if enabled {
                flags |= flag;
            }
        }
        let alpha_reference = if material.blend == BlendMode::AlphaTest {
            0.5
        } else {
            0.0
        };
        let emissive_hdr_scale = material
            .emissive
            .as_ref()
            .map_or(1.0, |image| image.hdr_scale.max(1.0));
        Self {
            tint: [1.0; 4],
            specular: [
                material.specular_color[0],
                material.specular_color[1],
                material.specular_color[2],
                material.specular_power,
            ],
            params: [material.opacity, alpha_reference, emissive_hdr_scale, 1.0],
            flags: [flags, 0, 0, 0],
            channels0: [
                material.channels.diffuse,
                material.channels.normal,
                material.channels.gloss,
                material.channels.opacity,
            ],
            channels1: [
                material.channels.xform,
                material.channels.emissive,
                material.channels.ao,
                0,
            ],
        }
    }
}

struct GpuMaterial {
    bind_group: wgpu::BindGroup,
    blend: BlendMode,
    pipeline_index: usize,
}

struct GpuSection {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    material_index: usize,
}

/// GPU resources used to draw one decoded [`Model`].
pub struct Renderer {
    scene_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    joint_buffer: wgpu::Buffer,
    materials: Vec<GpuMaterial>,
    sections: Vec<GpuSection>,
    pipelines: Vec<wgpu::RenderPipeline>,
    model_transform: Mat4,
    joint_count: usize,
}

impl Renderer {
    /// Uploads a decoded model and creates all legacy material pipelines.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        model: &Model,
        model_transform: Mat4,
    ) -> Self {
        let scene_uniform = SceneUniform::new(model_transform);
        let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UGX Scene Uniform"),
            contents: bytemuck::bytes_of(&scene_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let identity_joints = vec![Mat4::IDENTITY.to_cols_array_2d(); model.joint_count];
        let joint_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UGX Matrix Palette"),
            contents: bytemuck::cast_slice(&identity_joints),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let scene_layout = create_scene_layout(device);
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("UGX Scene Bind Group"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: joint_buffer.as_entire_binding(),
                },
            ],
        });

        let material_layout = create_material_layout(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("UGX Material Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let materials: Vec<GpuMaterial> = model
            .materials
            .iter()
            .map(|material| {
                let bind_group =
                    create_material_bind_group(device, queue, &material_layout, &sampler, material);
                GpuMaterial {
                    bind_group,
                    blend: material.blend,
                    pipeline_index: pipeline_index(material.blend, material.two_sided),
                }
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("UGX Parametric Shader Translation"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipelines = create_pipelines(
            device,
            surface_format,
            &shader,
            &scene_layout,
            &material_layout,
        );
        let mut sections = model
            .sections
            .iter()
            .filter(|section| !section.vertices.is_empty() && !section.indices.is_empty())
            .map(|section| GpuSection {
                vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("UGX Section Vertices"),
                    contents: bytemuck::cast_slice(&section.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("UGX Section Indices"),
                    contents: bytemuck::cast_slice(&section.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                index_count: section.index_count,
                material_index: section.material_index,
            })
            .collect::<Vec<_>>();
        sections.sort_by_key(|section| materials[section.material_index].blend.rank());

        Self {
            scene_buffer,
            scene_bind_group,
            joint_buffer,
            materials,
            sections,
            pipelines,
            model_transform,
            joint_count: model.joint_count,
        }
    }

    /// Updates the camera, transform, lighting, and fog constants for a frame.
    pub fn update_frame(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        model_transform: Mat4,
        lighting: &LightingParams,
    ) {
        self.model_transform = model_transform;
        let uniform = SceneUniform::from_frame(view_projection, model_transform, lighting);
        queue.write_buffer(&self.scene_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Replaces the skinning palette used by the vertex shader.
    ///
    /// The default palette is identity, which is the correct bind-pose matrix
    /// (`current_world * inverse_bind`) for UGX vertices stored in model space.
    ///
    /// # Panics
    ///
    /// Panics if the supplied palette does not contain exactly the model's
    /// allocated joint count.
    pub fn update_joints(&self, queue: &wgpu::Queue, matrices: &[Mat4]) {
        assert_eq!(
            matrices.len(),
            self.joint_count,
            "UGX joint palette length must remain constant"
        );
        let columns = matrices
            .iter()
            .map(Mat4::to_cols_array_2d)
            .collect::<Vec<_>>();
        queue.write_buffer(&self.joint_buffer, 0, bytemuck::cast_slice(&columns));
    }

    /// Draws all sections using the oracle blend order.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        for blend in BlendMode::DRAW_ORDER {
            for section in &self.sections {
                let material = &self.materials[section.material_index];
                if material.blend != blend {
                    continue;
                }
                pass.set_pipeline(&self.pipelines[material.pipeline_index]);
                pass.set_bind_group(1, &material.bind_group, &[]);
                pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
                pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..section.index_count, 0, 0..1);
            }
        }
    }

    /// Returns the current model-to-world transform.
    #[must_use]
    pub fn model_transform(&self) -> Mat4 {
        self.model_transform
    }
}

fn create_scene_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UGX Scene Layout"),
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
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn texture_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn create_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }];
    entries.extend((1..=7).map(texture_layout_entry));
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 8,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UGX Material Layout"),
        entries: &entries,
    })
}

fn pipeline_index(blend: BlendMode, two_sided: bool) -> usize {
    usize::from(blend.rank()) * 2 + usize::from(two_sided)
}

fn create_pipelines(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    scene_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
) -> Vec<wgpu::RenderPipeline> {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UGX Pipeline Layout"),
        bind_group_layouts: &[scene_layout, material_layout],
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

fn create_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    blend: BlendMode,
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
        label: Some("UGX Legacy Material Pipeline"),
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
            depth_write_enabled: matches!(blend, BlendMode::Opaque | BlendMode::AlphaTest),
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
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
    sampler: &wgpu::Sampler,
    material: &Material,
) -> wgpu::BindGroup {
    let uniform = MaterialUniform::from_material(material);
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("UGX Material Uniform: {}", material.name)),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let diffuse = create_texture_view(
        device,
        queue,
        &format!("UGX Diffuse: {}", material.name),
        material.diffuse.as_ref(),
        [255, 255, 255, 255],
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    let normal = create_texture_view(
        device,
        queue,
        &format!("UGX Normal: {}", material.name),
        material.normal.as_ref(),
        [128, 128, 255, 255],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let gloss = create_texture_view(
        device,
        queue,
        &format!("UGX Gloss: {}", material.name),
        material.gloss.as_ref(),
        [255; 4],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let opacity = create_texture_view(
        device,
        queue,
        &format!("UGX Opacity: {}", material.name),
        material.opacity_map.as_ref(),
        [255; 4],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let xform = create_texture_view(
        device,
        queue,
        &format!("UGX XForm: {}", material.name),
        material.xform.as_ref(),
        [0, 0, 0, 255],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let emissive = create_texture_view(
        device,
        queue,
        &format!("UGX Emissive: {}", material.name),
        material.emissive.as_ref(),
        [0, 0, 0, 0],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let ao = create_texture_view(
        device,
        queue,
        &format!("UGX AO: {}", material.name),
        material.ao.as_ref(),
        [255; 4],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("UGX Material Bind Group: {}", material.name)),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            texture_entry(1, &diffuse),
            texture_entry(2, &normal),
            texture_entry(3, &gloss),
            texture_entry(4, &opacity),
            texture_entry(5, &xform),
            texture_entry(6, &emissive),
            texture_entry(7, &ao),
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn create_texture_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    image: Option<&Image>,
    fallback: [u8; 4],
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let (width, height, pixels) = image.map_or((1, 1, fallback.as_slice()), |image| {
        (image.width, image.height, image.pixels.as_slice())
    });
    let mip_count = mip_level_count(width, height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mips = generate_mipmaps(pixels, width, height);
    let mut mip_width = width;
    let mut mip_height = height;
    for (level, mip) in mips.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: u32::try_from(level).expect("UGX mip level must fit u32"),
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            mip,
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
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

#[cfg(test)]
mod tests {
    use super::pipeline_index;
    use crate::ugx::BlendMode;

    #[test]
    fn pipeline_indices_keep_blends_and_culling_distinct() {
        assert_eq!(pipeline_index(BlendMode::Opaque, false), 0);
        assert_eq!(pipeline_index(BlendMode::Opaque, true), 1);
        assert_eq!(pipeline_index(BlendMode::Additive, false), 6);
        assert_eq!(pipeline_index(BlendMode::Additive, true), 7);
    }
}
