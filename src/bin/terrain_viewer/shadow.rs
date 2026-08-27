//! Cascaded directional shadow-map generation.

use glam::{Mat4, Vec3};
use render::terrain::NORMALIZED_TERRAIN_Y_OFFSET;
use render::{RenderPhase, WorldRenderer, wgpu};

const SHADOW_MAP_SIZE: u32 = 2048;
pub const SHADOW_CASCADE_COUNT: u32 = 4;
const SHADOW_CASCADE_SCALES: [f32; 4] = [8.0, 4.0, 2.0, 1.0];

/// GPU resources for the terrain and foliage shadow passes.
pub struct ShadowResources {
    _shadow_texture: wgpu::Texture,
    /// Array view sampled by oracle-compatible receiver shaders.
    pub shadow_view: wgpu::TextureView,
    cascade_shadow_views: Vec<wgpu::TextureView>,
    _depth_texture: wgpu::Texture,
    cascade_depth_views: Vec<wgpu::TextureView>,
    pub pipeline: wgpu::RenderPipeline,
    light_vp_buffers: Vec<wgpu::Buffer>,
    camera_bind_groups: Vec<wgpu::BindGroup>,
    pub params_buffer: wgpu::Buffer,
    pub params_bind_group_layout: wgpu::BindGroupLayout,
    pub params_bind_group: Option<wgpu::BindGroup>,
    light_vps: Vec<Mat4>,
    base_light_vp: Mat4,
    terrain_geometry: Option<TerrainShadowGeometry>,
}

struct TerrainShadowGeometry {
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    vertex_count: u32,
    instance_count: u32,
}

/// Shadow params uniform; mirrored by `shadow_depth.wesl`.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowParamsUniform {
    pub terrain_info: [f32; 4],
    pub mid: [f32; 4],
    pub range: [f32; 4],
}

pub struct ShadowSetup<'a> {
    pub position_texture_view: &'a wgpu::TextureView,
    pub position_sampler: &'a wgpu::Sampler,
    pub alpha_texture_view: &'a wgpu::TextureView,
    pub alpha_sampler: &'a wgpu::Sampler,
    pub dynamic_alpha_view: &'a wgpu::TextureView,
    pub terrain_info: [f32; 4],
    pub mid: [f32; 3],
    pub range: [f32; 3],
}

struct ShadowTargets {
    color_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    color_layer_views: Vec<wgpu::TextureView>,
    depth_texture: wgpu::Texture,
    depth_layer_views: Vec<wgpu::TextureView>,
}

fn layer_view(texture: &wgpu::Texture, layer: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_array_layer: layer,
        array_layer_count: Some(1),
        ..Default::default()
    })
}

fn create_shadow_targets(device: &wgpu::Device) -> ShadowTargets {
    let size = wgpu::Extent3d {
        width: SHADOW_MAP_SIZE,
        height: SHADOW_MAP_SIZE,
        depth_or_array_layers: SHADOW_CASCADE_COUNT,
    };
    let color_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Cascaded Shadow Map"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let color_layer_views = (0..SHADOW_CASCADE_COUNT)
        .map(|layer| layer_view(&color_texture, layer))
        .collect();

    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Cascaded Shadow Depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_layer_views = (0..SHADOW_CASCADE_COUNT)
        .map(|layer| layer_view(&depth_texture, layer))
        .collect();

    ShadowTargets {
        color_texture,
        color_view,
        color_layer_views,
        depth_texture,
        depth_layer_views,
    }
}

fn create_shadow_cameras(
    device: &wgpu::Device,
    camera_layout: &wgpu::BindGroupLayout,
) -> (Vec<wgpu::Buffer>, Vec<wgpu::BindGroup>) {
    let mut buffers = Vec::with_capacity(SHADOW_CASCADE_COUNT as usize);
    let mut bind_groups = Vec::with_capacity(SHADOW_CASCADE_COUNT as usize);
    for cascade in 0..SHADOW_CASCADE_COUNT {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("Shadow Cascade {cascade} VP")),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Shadow Cascade {cascade} Camera Bind Group")),
            layout: camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        buffers.push(buffer);
        bind_groups.push(bind_group);
    }
    (buffers, bind_groups)
}

fn create_shadow_params(device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::Buffer) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Shadow Params Layout"),
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
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let size = u64::try_from(std::mem::size_of::<ShadowParamsUniform>())
        .expect("shadow params size must fit u64");
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Shadow Params"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (layout, buffer)
}

fn create_shadow_pipeline(
    device: &wgpu::Device,
    camera_layout: &wgpu::BindGroupLayout,
    params_layout: &wgpu::BindGroupLayout,
    vertex_layout: &[wgpu::VertexBufferLayout<'_>],
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Depth Shader"),
        source: wgpu::ShaderSource::Wgsl(render::terrain::SHADOW_DEPTH_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shadow Pipeline Layout"),
        bind_group_layouts: &[camera_layout, params_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Cascaded Shadow Depth Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: vertex_layout,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
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
            cull_mode: None,
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

impl ShadowResources {
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        vertex_buffer_layout: &[wgpu::VertexBufferLayout<'_>],
    ) -> Self {
        let targets = create_shadow_targets(device);
        let (light_vp_buffers, camera_bind_groups) =
            create_shadow_cameras(device, camera_bind_group_layout);
        let (params_bind_group_layout, params_buffer) = create_shadow_params(device);
        let pipeline = create_shadow_pipeline(
            device,
            camera_bind_group_layout,
            &params_bind_group_layout,
            vertex_buffer_layout,
        );

        Self {
            _shadow_texture: targets.color_texture,
            shadow_view: targets.color_view,
            cascade_shadow_views: targets.color_layer_views,
            _depth_texture: targets.depth_texture,
            cascade_depth_views: targets.depth_layer_views,
            pipeline,
            light_vp_buffers,
            camera_bind_groups,
            params_buffer,
            params_bind_group_layout,
            params_bind_group: None,
            light_vps: vec![Mat4::IDENTITY; SHADOW_CASCADE_COUNT as usize],
            base_light_vp: Mat4::IDENTITY,
            terrain_geometry: None,
        }
    }

    pub fn set_terrain_geometry(&mut self, geometry: (&wgpu::Buffer, &wgpu::Buffer, u32, u32)) {
        let (vertex_buffer, instance_buffer, vertex_count, instance_count) = geometry;
        self.terrain_geometry = Some(TerrainShadowGeometry {
            vertex_buffer: vertex_buffer.clone(),
            instance_buffer: instance_buffer.clone(),
            vertex_count,
            instance_count,
        });
    }

    pub fn setup_params(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        setup: &ShadowSetup<'_>,
    ) {
        let params = ShadowParamsUniform {
            terrain_info: setup.terrain_info,
            mid: [
                setup.mid[0],
                setup.mid[1],
                setup.mid[2],
                NORMALIZED_TERRAIN_Y_OFFSET,
            ],
            range: [setup.range[0], setup.range[1], setup.range[2], 0.0],
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        self.params_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow Params Bind Group"),
            layout: &self.params_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(setup.position_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(setup.alpha_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(setup.alpha_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(setup.dynamic_alpha_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(setup.position_sampler),
                },
            ],
        }));
    }

    /// Builds the base (largest) shadow transform and the four scaled cascade transforms.
    pub fn compute_light_vp(
        &mut self,
        light_dir: Vec3,
        terrain_center: Vec3,
        terrain_size: Vec3,
    ) -> Mat4 {
        let light_dir = light_dir.normalize();
        let light_distance = terrain_size.length();
        let light_pos = terrain_center + light_dir * light_distance;
        let view = Mat4::look_at_rh(light_pos, terrain_center, Vec3::Y);
        let half = terrain_size * 0.5;
        let corners = [
            terrain_center + Vec3::new(-half.x, -half.y, -half.z),
            terrain_center + Vec3::new(half.x, -half.y, -half.z),
            terrain_center + Vec3::new(-half.x, half.y, -half.z),
            terrain_center + Vec3::new(half.x, half.y, -half.z),
            terrain_center + Vec3::new(-half.x, -half.y, half.z),
            terrain_center + Vec3::new(half.x, -half.y, half.z),
            terrain_center + Vec3::new(-half.x, half.y, half.z),
            terrain_center + Vec3::new(half.x, half.y, half.z),
        ];

        let mut min_ls = Vec3::splat(f32::MAX);
        let mut max_ls = Vec3::splat(f32::MIN);
        for corner in corners {
            let light_space = view.transform_point3(corner);
            min_ls = min_ls.min(light_space);
            max_ls = max_ls.max(light_space);
        }
        let padding = (max_ls - min_ls) * 0.05;
        min_ls -= padding;
        max_ls += padding;
        let projection =
            Mat4::orthographic_rh(min_ls.x, max_ls.x, min_ls.y, max_ls.y, -max_ls.z, -min_ls.z);
        self.base_light_vp = projection * view;

        for (light_vp, &scale) in self.light_vps.iter_mut().zip(&SHADOW_CASCADE_SCALES) {
            *light_vp = Mat4::from_scale(Vec3::new(scale, scale, 1.0)) * self.base_light_vp;
        }
        self.base_light_vp
    }

    pub fn update_light_vp(&self, queue: &wgpu::Queue) {
        for (buffer, matrix) in self.light_vp_buffers.iter().zip(&self.light_vps) {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&matrix.to_cols_array()));
        }
    }

    #[must_use]
    pub fn cascade_shadow_view(&self, cascade: usize) -> &wgpu::TextureView {
        &self.cascade_shadow_views[cascade]
    }

    #[must_use]
    pub fn cascade_depth_view(&self, cascade: usize) -> &wgpu::TextureView {
        &self.cascade_depth_views[cascade]
    }

    #[must_use]
    pub fn cascade_camera_bind_group(&self, cascade: usize) -> &wgpu::BindGroup {
        &self.camera_bind_groups[cascade]
    }

    #[must_use]
    pub fn cascade_count(&self) -> usize {
        self.cascade_shadow_views.len()
    }
}

impl WorldRenderer for ShadowResources {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        let RenderPhase::Shadow { cascade } = phase else {
            return;
        };
        let Some(params_bind_group) = &self.params_bind_group else {
            return;
        };
        let Some(camera) = self.camera_bind_groups.get(cascade) else {
            return;
        };
        let Some(geometry) = &self.terrain_geometry else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, params_bind_group, &[]);
        pass.set_vertex_buffer(0, geometry.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, geometry.instance_buffer.slice(..));
        pass.draw(0..geometry.vertex_count, 0..geometry.instance_count);
    }
}
