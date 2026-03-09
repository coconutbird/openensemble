//! Shadow map generation for directional light.
//!
//! Renders the terrain from the light's perspective into a depth texture
//! that is then sampled by the main terrain shaders for VSM shadows.

use glam::{Mat4, Vec3};
use render::wgpu;

/// Shadow map resolution (square).
const SHADOW_MAP_SIZE: u32 = 2048;

/// GPU resources for the shadow pass.
#[allow(dead_code)]
pub struct ShadowResources {
    /// The shadow map color texture (Rg16Float for VSM: depth + depth²).
    pub shadow_texture: wgpu::Texture,
    /// View for sampling the shadow map in the main pass.
    pub shadow_view: wgpu::TextureView,
    /// Depth attachment for the shadow render pass.
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    /// Shadow depth render pipeline.
    pub pipeline: wgpu::RenderPipeline,
    /// Camera (light VP) uniform buffer.
    pub light_vp_buffer: wgpu::Buffer,
    /// Camera bind group (group 0).
    pub camera_bind_group: wgpu::BindGroup,
    /// Shadow params uniform buffer.
    pub params_buffer: wgpu::Buffer,
    /// Shadow params bind group (group 1).
    pub params_bind_group_layout: wgpu::BindGroupLayout,
    pub params_bind_group: Option<wgpu::BindGroup>,
    /// The light view-projection matrix (for passing to main shaders).
    pub light_vp: Mat4,
}

/// Shadow params uniform (must match shadow_depth.wesl ShadowParams).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowParamsUniform {
    pub terrain_info: [f32; 4],
    pub mid: [f32; 4],
    pub range: [f32; 4],
}

impl ShadowResources {
    /// Create shadow map resources.
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        vertex_buffer_layout: &[wgpu::VertexBufferLayout],
    ) -> Self {
        // Shadow map color texture (Rg16Float for VSM - filterable)
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow Map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Depth buffer for shadow pass
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow Depth"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Light VP buffer
        let light_vp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow Light VP"),
            size: 64, // mat4x4
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Camera bind group for shadow pass (reuse layout from main pass)
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow Camera Bind Group"),
            layout: camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_vp_buffer.as_entire_binding(),
            }],
        });

        // Shadow params layout
        let params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Shadow Params Layout"),
                entries: &[
                    // ShadowParams uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Position texture (Uint, not filterable)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Uint,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow Params"),
            size: std::mem::size_of::<ShadowParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create shadow depth pipeline
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow Depth Shader"),
            source: wgpu::ShaderSource::Wgsl(render::terrain::SHADOW_DEPTH_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow Pipeline Layout"),
            bind_group_layouts: &[camera_bind_group_layout, &params_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Depth Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: vertex_buffer_layout,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rg16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // No culling for shadow pass
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2, // Small bias to reduce shadow acne
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            shadow_texture,
            shadow_view,
            depth_texture,
            depth_view,
            pipeline,
            light_vp_buffer,
            camera_bind_group,
            params_buffer,
            params_bind_group_layout,
            params_bind_group: None,
            light_vp: Mat4::IDENTITY,
        }
    }

    /// Set up the params bind group with the terrain position texture.
    pub fn setup_params(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        position_texture_view: &wgpu::TextureView,
        terrain_info: [f32; 4],
        mid: [f32; 3],
        range: [f32; 3],
    ) {
        let params = ShadowParamsUniform {
            terrain_info,
            mid: [mid[0], mid[1], mid[2], 0.0],
            range: [range[0], range[1], range[2], 0.0],
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow Params Bind Group"),
            layout: &self.params_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(position_texture_view),
                },
            ],
        });
        self.params_bind_group = Some(bind_group);
    }

    /// Compute the light's orthographic view-projection matrix.
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

        // Compute terrain AABB in light space for tight ortho bounds
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
        for corner in &corners {
            let ls = view.transform_point3(*corner);
            min_ls = min_ls.min(ls);
            max_ls = max_ls.max(ls);
        }

        let padding = (max_ls - min_ls) * 0.05;
        min_ls -= padding;
        max_ls += padding;

        let proj =
            Mat4::orthographic_rh(min_ls.x, max_ls.x, min_ls.y, max_ls.y, -max_ls.z, -min_ls.z);

        self.light_vp = proj * view;
        self.light_vp
    }

    /// Update the light VP buffer on the GPU.
    pub fn update_light_vp(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.light_vp_buffer,
            0,
            bytemuck::cast_slice(&self.light_vp.to_cols_array()),
        );
    }

    /// Render the shadow pass.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        vertex_buffer: &wgpu::Buffer,
        instance_buffer: &wgpu::Buffer,
        vertex_count: u32,
        instance_count: u32,
    ) {
        let Some(params_bg) = &self.params_bind_group else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.shadow_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 1.0,
                        g: 1.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(1, params_bg, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.draw(0..vertex_count, 0..instance_count);
    }
}
