//! Terrain Viewer for Halo Wars XTD files.
//!
//! Loads and renders terrain from XTD files using wgpu.
//! Supports XTT texturing for albedo atlas rendering.
//! WASD + mouse to fly around the terrain.

use anyhow::Result;
use core::app::{Application, FrameContext, Input, KeyCode, WindowConfig};
use core::prelude::*;
use data::xtd::{TerrainVertices, XtdReader};
use data::xtt::XttReader;
use glam::{Mat4, Vec3};
use render::{Application3D, RenderContext, wgpu};
use std::path::PathBuf;

/// Camera for flying around the terrain.
struct Camera {
    position: Vec3,
    yaw: f32,   // Horizontal rotation (radians)
    pitch: f32, // Vertical rotation (radians)
    fov: f32,
    near: f32,
    far: f32,
    speed: f32,
    sensitivity: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(500.0, 200.0, 500.0),
            yaw: -std::f32::consts::FRAC_PI_4,
            pitch: -0.3,
            fov: 60.0_f32.to_radians(),
            near: 1.0,
            far: 10000.0,
            speed: 100.0,
            sensitivity: 0.002,
        }
    }
}

impl Camera {
    fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.position + self.forward(), Vec3::Y)
    }

    fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov, aspect, self.near, self.far)
    }

    fn update(&mut self, input: &Input, dt: f32) {
        let speed = if input.is_key_held(KeyCode::LShift) {
            self.speed * 3.0
        } else {
            self.speed
        };

        // Movement
        if input.is_key_held(KeyCode::W) {
            self.position += self.forward() * speed * dt;
        }
        if input.is_key_held(KeyCode::S) {
            self.position -= self.forward() * speed * dt;
        }
        if input.is_key_held(KeyCode::A) {
            self.position -= self.right() * speed * dt;
        }
        if input.is_key_held(KeyCode::D) {
            self.position += self.right() * speed * dt;
        }
        if input.is_key_held(KeyCode::Space) {
            self.position.y += speed * dt;
        }
        if input.is_key_held(KeyCode::LCtrl) {
            self.position.y -= speed * dt;
        }

        // Arrow keys for looking
        if input.is_key_held(KeyCode::Left) {
            self.yaw -= 1.5 * dt;
        }
        if input.is_key_held(KeyCode::Right) {
            self.yaw += 1.5 * dt;
        }
        if input.is_key_held(KeyCode::Up) {
            self.pitch += 1.0 * dt;
        }
        if input.is_key_held(KeyCode::Down) {
            self.pitch -= 1.0 * dt;
        }

        // Clamp pitch
        self.pitch = self.pitch.clamp(-1.5, 1.5);
    }
}

/// Terrain mesh data for rendering.
struct TerrainMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    world_min: [f32; 3],
    world_max: [f32; 3],
}

impl TerrainMesh {
    fn from_xtd(vertices: &TerrainVertices, world_min: [f32; 3], world_max: [f32; 3]) -> Self {
        let indices = vertices.generate_indices();
        Self {
            positions: vertices.positions.clone(),
            normals: vertices.normals.clone(),
            uvs: vertices.uvs.clone(),
            indices,
            world_min,
            world_max,
        }
    }

    fn center(&self) -> Vec3 {
        Vec3::new(
            (self.world_min[0] + self.world_max[0]) / 2.0,
            (self.world_min[1] + self.world_max[1]) / 2.0,
            (self.world_min[2] + self.world_max[2]) / 2.0,
        )
    }

    fn size(&self) -> Vec3 {
        Vec3::new(
            self.world_max[0] - self.world_min[0],
            self.world_max[1] - self.world_min[1],
            self.world_max[2] - self.world_min[2],
        )
    }
}

/// GPU resources for terrain rendering
struct GpuResources {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    texture_bind_group: wgpu::BindGroup,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

/// Albedo atlas data from XTT file.
struct AlbedoData {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// The terrain viewer application.
struct TerrainViewer {
    xtd_path: Option<PathBuf>,
    terrain: Option<TerrainMesh>,
    albedo: Option<AlbedoData>,
    camera: Camera,
    show_info: bool,
    wireframe: bool,
    load_error: Option<String>,
    gpu: Option<GpuResources>,
    surface_format: wgpu::TextureFormat,
}

impl TerrainViewer {
    fn new(xtd_path: Option<PathBuf>) -> Self {
        Self {
            xtd_path,
            terrain: None,
            albedo: None,
            camera: Camera::default(),
            show_info: true,
            wireframe: false,
            load_error: None,
            gpu: None,
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
        }
    }

    fn load_terrain(&mut self) {
        let Some(path) = self.xtd_path.clone() else {
            self.load_error = Some("No XTD file specified".to_string());
            return;
        };

        log::info!("Loading XTD from: {}", path.display());

        match std::fs::read(&path) {
            Ok(data) => match XtdReader::read(&data) {
                Ok(xtd) => {
                    log::info!(
                        "XTD loaded: {}x{} verts, tile_scale={}",
                        xtd.header.num_x_verts,
                        xtd.header.num_x_verts,
                        xtd.header.tile_scale
                    );

                    match xtd.decode_vertices() {
                        Ok(vertices) => {
                            log::info!(
                                "Decoded {} vertices, {} triangles",
                                vertices.positions.len(),
                                vertices.generate_indices().len() / 3
                            );

                            let mesh = TerrainMesh::from_xtd(
                                &vertices,
                                xtd.header.world_min,
                                xtd.header.world_max,
                            );

                            // Position camera at terrain center
                            self.camera.position = mesh.center() + Vec3::new(0.0, 200.0, -300.0);
                            self.terrain = Some(mesh);
                            self.load_error = None;

                            // Try to load corresponding XTT file
                            self.load_xtt(&path);
                        }
                        Err(e) => {
                            self.load_error = Some(format!("Failed to decode vertices: {}", e));
                            log::error!("{}", self.load_error.as_ref().unwrap());
                        }
                    }
                }
                Err(e) => {
                    self.load_error = Some(format!("Failed to parse XTD: {}", e));
                    log::error!("{}", self.load_error.as_ref().unwrap());
                }
            },
            Err(e) => {
                self.load_error = Some(format!("Failed to read file: {}", e));
                log::error!("{}", self.load_error.as_ref().unwrap());
            }
        }
    }

    fn load_xtt(&mut self, xtd_path: &PathBuf) {
        // XTT file has same path but .xtt extension
        let xtt_path = xtd_path.with_extension("xtt");

        if !xtt_path.exists() {
            log::info!("No XTT file found at: {}", xtt_path.display());
            return;
        }

        log::info!("Loading XTT from: {}", xtt_path.display());

        match std::fs::read(&xtt_path) {
            Ok(data) => match XttReader::read(&data) {
                Ok(xtt) => {
                    log::info!(
                        "XTT loaded: {} textures, {} linker chunks, {} bytes albedo",
                        xtt.header.num_active_textures,
                        xtt.linkers.len(),
                        xtt.albedo_data.len()
                    );

                    match xtt.decode_albedo() {
                        Ok(atlas) => {
                            log::info!(
                                "Albedo atlas decoded: {}x{} pixels",
                                atlas.width,
                                atlas.height
                            );
                            self.albedo = Some(AlbedoData {
                                width: atlas.width,
                                height: atlas.height,
                                pixels: atlas.pixels,
                            });
                        }
                        Err(e) => {
                            log::warn!("Failed to decode XTT albedo: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to parse XTT: {}", e);
                }
            },
            Err(e) => {
                log::warn!("Failed to read XTT file: {}", e);
            }
        }
    }
}

impl Application for TerrainViewer {
    fn init(&mut self) {
        log::info!("Terrain Viewer initialized");
        self.load_terrain();
    }

    fn update(&mut self, input: &Input, ctx: &FrameContext) -> bool {
        if input.is_key_pressed(KeyCode::Escape) {
            return false;
        }

        if input.is_key_pressed(KeyCode::Tab) {
            self.show_info = !self.show_info;
        }

        if input.is_key_pressed(KeyCode::F) {
            self.wireframe = !self.wireframe;
        }

        self.camera.update(input, ctx.delta_time);

        true
    }

    fn ui(&mut self, ctx: &egui::Context) {
        if self.show_info {
            egui::Window::new("Terrain Info")
                .default_pos([10.0, 10.0])
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Camera: ({:.1}, {:.1}, {:.1})",
                        self.camera.position.x, self.camera.position.y, self.camera.position.z
                    ));

                    if let Some(terrain) = &self.terrain {
                        ui.separator();
                        ui.label(format!("Vertices: {}", terrain.positions.len()));
                        ui.label(format!("Triangles: {}", terrain.indices.len() / 3));
                        ui.label(format!(
                            "World Size: {:.0} x {:.0} x {:.0}",
                            terrain.size().x,
                            terrain.size().y,
                            terrain.size().z
                        ));
                    }

                    if let Some(err) = &self.load_error {
                        ui.separator();
                        ui.colored_label(egui::Color32::RED, err);
                    }

                    ui.separator();
                    ui.label("Controls:");
                    ui.label("  WASD - Move");
                    ui.label("  Space/Ctrl - Up/Down");
                    ui.label("  Arrows - Look");
                    ui.label("  Shift - Fast");
                    ui.label("  Tab - Toggle info");
                    ui.label("  F - Toggle wireframe");
                    ui.label("  Escape - Quit");
                });
        }
    }

    fn render(&mut self, _ctx: &FrameContext) -> Color {
        // Sky blue clear color
        Color::new(0.4, 0.6, 0.9, 1.0)
    }
}

// Terrain shader with texture support
const TERRAIN_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var t_albedo: texture_2d<f32>;
@group(1) @binding(1)
var s_albedo: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.world_pos = in.position;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let normal = normalize(in.normal);
    let diff = max(dot(normal, light_dir), 0.0);
    let ambient = 0.4;
    let lighting = ambient + diff * 0.6;

    // Sample albedo texture
    let albedo = textureSample(t_albedo, s_albedo, in.uv);
    let base_color = albedo.rgb;

    return vec4<f32>(base_color * lighting, 1.0);
}
"#;

fn create_depth_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

impl Application3D for TerrainViewer {
    fn init_gpu(
        &mut self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        self.surface_format = format;

        // We'll initialize GPU resources after terrain is loaded
        // This is called before init(), so terrain isn't loaded yet
    }

    fn resize_gpu(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        // Recreate depth texture
        if let Some(gpu) = &mut self.gpu {
            let (depth_texture, depth_view) = create_depth_texture(device, width, height);
            gpu.depth_texture = depth_texture;
            gpu.depth_view = depth_view;
        }
    }

    fn render_3d(&mut self, ctx: &mut RenderContext) {
        // Create GPU resources if not yet created and terrain is loaded
        if self.gpu.is_none() && self.terrain.is_some() {
            // Extract values we need before calling create_gpu_resources
            let terrain = self.terrain.as_ref().unwrap();
            let positions = terrain.positions.clone();
            let normals = terrain.normals.clone();
            let uvs = terrain.uvs.clone();
            let indices = terrain.indices.clone();
            let albedo = self.albedo.take();
            self.create_gpu_resources_from_data(
                ctx.device, ctx.queue, &positions, &normals, &uvs, &indices, albedo, ctx.size.0,
                ctx.size.1,
            );
        }

        let Some(gpu) = &self.gpu else {
            return;
        };

        // Update camera uniform
        let aspect = ctx.size.0 as f32 / ctx.size.1 as f32;
        let view = self.camera.view_matrix();
        let proj = self.camera.projection_matrix(aspect);
        let view_proj = proj * view;
        ctx.queue.write_buffer(
            &gpu.camera_buffer,
            0,
            bytemuck::cast_slice(&view_proj.to_cols_array()),
        );

        // Render terrain
        {
            let mut render_pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Terrain Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear - already cleared
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&gpu.pipeline);
            render_pass.set_bind_group(0, &gpu.camera_bind_group, &[]);
            render_pass.set_bind_group(1, &gpu.texture_bind_group, &[]);
            render_pass.set_vertex_buffer(0, gpu.vertex_buffer.slice(..));
            render_pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..gpu.index_count, 0, 0..1);
        }
    }
}

impl TerrainViewer {
    #[allow(clippy::too_many_arguments)]
    fn create_gpu_resources_from_data(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        albedo: Option<AlbedoData>,
        width: u32,
        height: u32,
    ) {
        use wgpu::util::DeviceExt;

        // Create interleaved vertex data: [pos, normal, uv, pos, normal, uv, ...]
        // 3 + 3 + 2 = 8 floats per vertex
        let mut vertex_data = Vec::with_capacity(positions.len() * 8);
        for i in 0..positions.len() {
            vertex_data.extend_from_slice(&positions[i]);
            vertex_data.extend_from_slice(&normals[i]);
            vertex_data.extend_from_slice(&uvs[i]);
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Uniform Buffer"),
            size: 64, // mat4x4<f32>
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Create albedo texture
        let (tex_width, tex_height, tex_data) = if let Some(albedo) = albedo {
            log::info!(
                "Creating albedo texture: {}x{} ({} bytes)",
                albedo.width,
                albedo.height,
                albedo.pixels.len()
            );
            (albedo.width, albedo.height, albedo.pixels)
        } else {
            // Create a 1x1 white texture as fallback
            log::info!("No albedo data, using white fallback texture");
            (1, 1, vec![255u8, 255, 255, 255])
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Terrain Albedo Texture"),
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
            &tex_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * tex_width),
                rows_per_image: Some(tex_height),
            },
            wgpu::Extent3d {
                width: tex_width,
                height: tex_height,
                depth_or_array_layers: 1,
            },
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Terrain Albedo Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 16, // Max anisotropic filtering for better quality at angles
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
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
            label: Some("Texture Bind Group"),
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain Shader"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terrain Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32, // 8 floats * 4 bytes
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0, // position
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1, // normal
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2, // uv
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.surface_format,
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
        });

        let (depth_texture, depth_view) = create_depth_texture(device, width, height);

        self.gpu = Some(GpuResources {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            camera_buffer,
            camera_bind_group,
            texture_bind_group,
            depth_texture,
            depth_view,
        });

        log::info!(
            "GPU resources created: {} vertices, {} indices, {}x{} texture",
            positions.len(),
            indices.len(),
            tex_width,
            tex_height
        );
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Terrain Viewer starting...");

    // Parse args for XTD path
    let args: Vec<String> = std::env::args().collect();
    let xtd_path = if args.len() > 1 {
        Some(PathBuf::from(&args[1]))
    } else {
        // Default to test file
        let default_path = PathBuf::from(
            "../ensemble-rs/test_extract/scenario/skirmish/design/blood_gulch/blood_gulch.xtd",
        );
        if default_path.exists() {
            Some(default_path)
        } else {
            None
        }
    };

    if let Some(path) = &xtd_path {
        log::info!("XTD file: {}", path.display());
    } else {
        log::warn!("No XTD file specified. Usage: terrain_viewer <path/to/file.xtd>");
    }

    let config = WindowConfig::new("Terrain Viewer - Halo Wars XTD", 1280, 720);
    render::run_3d(config, TerrainViewer::new(xtd_path))?;

    Ok(())
}
