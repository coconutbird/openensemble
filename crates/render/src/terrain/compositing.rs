//! GPU Terrain Texture Compositing.
//!
//! This module provides GPU-based render-to-texture compositing for terrain textures.
//! Instead of blending texture layers per-pixel every frame (slow), we pre-composite
//! unique textures per chunk once (or on LOD change) and sample the composited atlas.
//!
//! This matches the approach used by retail Halo Wars DE:
//! - Each terrain chunk (16x16 grid = 256 total) gets a unique 512x512 composited texture
//! - Compositing happens on GPU using render-to-texture
//! - The terrain shader then samples the single composited texture
//!
//! ## Architecture
//!
//! ```text
//! Load Time:  [Tiled Textures] + [Alpha Maps]
//!                     ↓
//! Composite Pass:  Render-to-texture (once per chunk)
//!                     ↓
//!               [8K×8K Composited Atlas]
//!                     ↓
//! Render Pass:  Simple texture sample (every frame)
//! ```

use wgpu;

/// LOD level configuration for distance-based compositing quality.
#[derive(Debug, Clone)]
pub struct LodConfig {
    /// Distance thresholds for each LOD level (in world units).
    /// LOD 0 = closest (highest detail), LOD 3 = farthest (lowest detail).
    /// If camera distance < threshold[i], use LOD level i.
    pub distance_thresholds: [f32; 4],

    /// Texture size multiplier for each LOD level.
    /// LOD 0 = 1.0 (512), LOD 1 = 0.5 (256), LOD 2 = 0.25 (128), LOD 3 = 0.125 (64).
    /// Note: Currently we use fixed resolution atlas, so this is for future use.
    pub size_multipliers: [f32; 4],
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            // Reasonable defaults for terrain viewing
            distance_thresholds: [200.0, 500.0, 1000.0, f32::MAX],
            size_multipliers: [1.0, 0.5, 0.25, 0.125],
        }
    }
}

impl LodConfig {
    /// Create LOD config with custom distance thresholds.
    pub fn with_distances(d0: f32, d1: f32, d2: f32) -> Self {
        Self {
            distance_thresholds: [d0, d1, d2, f32::MAX],
            ..Default::default()
        }
    }

    /// Calculate LOD level based on distance.
    pub fn lod_for_distance(&self, distance: f32) -> u8 {
        for (i, &threshold) in self.distance_thresholds.iter().enumerate() {
            if distance < threshold {
                return i as u8;
            }
        }
        3 // Fallback to lowest LOD
    }
}

/// Configuration for terrain texture compositing.
#[derive(Debug, Clone)]
pub struct CompositingConfig {
    /// Size of each chunk's composited texture (512 = retail Halo Wars).
    pub chunk_texture_size: u32,
    /// Number of chunks in X direction (typically 16).
    pub chunks_x: u32,
    /// Number of chunks in Z direction (typically 16).
    pub chunks_z: u32,
    /// Atlas width (chunks_x * chunk_texture_size).
    pub atlas_width: u32,
    /// Atlas height (chunks_z * chunk_texture_size).
    pub atlas_height: u32,
}

impl Default for CompositingConfig {
    fn default() -> Self {
        Self {
            chunk_texture_size: 512,
            chunks_x: 16,
            chunks_z: 16,
            atlas_width: 8192,  // 16 * 512
            atlas_height: 8192, // 16 * 512
        }
    }
}

impl CompositingConfig {
    /// Create a config with a specific chunk texture size.
    pub fn with_chunk_size(chunk_texture_size: u32) -> Self {
        Self {
            chunk_texture_size,
            chunks_x: 16,
            chunks_z: 16,
            atlas_width: 16 * chunk_texture_size,
            atlas_height: 16 * chunk_texture_size,
        }
    }

    /// Total number of chunks.
    pub fn total_chunks(&self) -> u32 {
        self.chunks_x * self.chunks_z
    }
}

/// Uniform parameters for compositing a single chunk.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompositeParams {
    /// Which chunk we're compositing (0-255 for 16x16 grid).
    pub chunk_index: u32,
    /// Number of active layers for this chunk (1-8).
    pub num_layers: u32,
    /// UV offset in output atlas (x, y).
    pub chunk_offset: [f32; 2],
    /// UV size in output atlas (width, height).
    pub chunk_size: [f32; 2],
    /// Padding to 32 bytes.
    pub _padding: [f32; 2],
}

/// GPU resources for terrain texture compositing.
pub struct CompositorResources {
    /// Composited albedo atlas (render target).
    pub albedo_atlas: wgpu::Texture,
    /// View for sampling the composited albedo.
    pub albedo_atlas_view: wgpu::TextureView,

    /// Composited normal atlas (render target).
    pub normal_atlas: wgpu::Texture,
    /// View for sampling the composited normals.
    pub normal_atlas_view: wgpu::TextureView,

    /// Compositing render pipeline.
    pub pipeline: wgpu::RenderPipeline,

    /// Bind group layout for compositing shader.
    pub bind_group_layout: wgpu::BindGroupLayout,

    /// Per-chunk params buffer (updated for each chunk composite).
    pub params_buffer: wgpu::Buffer,

    /// Configuration.
    pub config: CompositingConfig,

    /// Dirty flags per chunk (needs re-composite).
    pub dirty_chunks: Vec<bool>,

    /// Current LOD level per chunk (0 = highest detail).
    pub chunk_lod: Vec<u8>,
}

impl CompositorResources {
    /// Create new compositor resources.
    ///
    /// Creates the composited atlas textures, pipeline, and bind group layout.
    /// Use `create_bind_group()` to create a bind group with actual textures.
    ///
    /// # Arguments
    /// * `device` - wgpu device
    /// * `config` - Compositing configuration
    pub fn new(device: &wgpu::Device, config: CompositingConfig) -> Self {
        use super::shaders::COMPOSITE_SHADER;

        // Create albedo atlas as render target
        let albedo_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Composited Albedo Atlas"),
            size: wgpu::Extent3d {
                width: config.atlas_width,
                height: config.atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1, // No mipmaps on atlas (chunks have internal detail)
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let albedo_atlas_view = albedo_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        // Create normal atlas as render target (linear data, not sRGB)
        let normal_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Composited Normal Atlas"),
            size: wgpu::Extent3d {
                width: config.atlas_width,
                height: config.atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, // Linear for normal maps
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let normal_atlas_view = normal_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        // Create params buffer for per-chunk compositing
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Composite Params Buffer"),
            size: std::mem::size_of::<CompositeParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Bind Group Layout"),
            entries: &[
                // CompositeParams uniform
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
                // Terrain texture array
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Alpha atlas
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
                // Chunk layers storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Texture scales storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create compositing pipeline
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Composite Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[], // Fullscreen triangle, no vertex buffers
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None, // No blending, we write directly
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Fullscreen triangle
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None, // No depth for compositing
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let total_chunks = config.total_chunks() as usize;

        Self {
            albedo_atlas,
            albedo_atlas_view,
            normal_atlas,
            normal_atlas_view,
            pipeline,
            bind_group_layout,
            params_buffer,
            config,
            dirty_chunks: vec![true; total_chunks], // All dirty initially
            chunk_lod: vec![0; total_chunks],       // All at highest LOD
        }
    }

    /// Create a bind group for compositing with the given resources.
    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        terrain_array_view: &wgpu::TextureView,
        alpha_atlas_view: &wgpu::TextureView,
        chunk_layers_buffer: &wgpu::Buffer,
        texture_scales_buffer: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(terrain_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(alpha_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: chunk_layers_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: texture_scales_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Composite a single chunk to the atlas.
    ///
    /// This renders the composited texture for the given chunk index
    /// to its region in the atlas.
    pub fn composite_chunk(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        chunk_index: u32,
        num_layers: u32,
    ) {
        let chunk_x = chunk_index % self.config.chunks_x;
        let chunk_z = chunk_index / self.config.chunks_x;

        // Calculate viewport for this chunk in the atlas
        let viewport_x = chunk_x * self.config.chunk_texture_size;
        let viewport_y = chunk_z * self.config.chunk_texture_size;

        // Update params buffer
        let params = CompositeParams {
            chunk_index,
            num_layers,
            chunk_offset: [
                chunk_x as f32 / self.config.chunks_x as f32,
                chunk_z as f32 / self.config.chunks_z as f32,
            ],
            chunk_size: [
                1.0 / self.config.chunks_x as f32,
                1.0 / self.config.chunks_z as f32,
            ],
            _padding: [0.0, 0.0],
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));

        // Create render pass targeting this chunk's region in the atlas
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Composite Chunk Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.albedo_atlas_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // Don't clear, we render per-chunk
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Set viewport to this chunk's region
        render_pass.set_viewport(
            viewport_x as f32,
            viewport_y as f32,
            self.config.chunk_texture_size as f32,
            self.config.chunk_texture_size as f32,
            0.0,
            1.0,
        );

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.draw(0..3, 0..1); // Fullscreen triangle

        // Mark chunk as clean
        self.dirty_chunks[chunk_index as usize] = false;
    }

    /// Composite all dirty chunks.
    pub fn composite_all_dirty(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        chunk_layer_counts: &[u32], // Number of layers per chunk
    ) {
        let dirty_count = self.dirty_chunks.iter().filter(|&&d| d).count();
        if dirty_count > 0 {
            log::debug!(
                "Compositing {} dirty chunks to {}×{} atlas",
                dirty_count,
                self.config.atlas_width,
                self.config.atlas_height
            );
        }

        let mut composited_count = 0u32;
        for chunk_idx in 0..self.config.total_chunks() {
            if self.dirty_chunks[chunk_idx as usize] {
                let num_layers = chunk_layer_counts
                    .get(chunk_idx as usize)
                    .copied()
                    .unwrap_or(1);
                self.composite_chunk(encoder, bind_group, queue, chunk_idx, num_layers);
                composited_count += 1;
            }
        }

        if composited_count > 0 {
            log::info!("Composited {} chunks to GPU atlas", composited_count);
        }
    }

    /// Mark all chunks as dirty (needs re-composite).
    pub fn mark_all_dirty(&mut self) {
        self.dirty_chunks.fill(true);
    }

    /// Mark a specific chunk as dirty.
    pub fn mark_chunk_dirty(&mut self, chunk_index: u32) {
        if (chunk_index as usize) < self.dirty_chunks.len() {
            self.dirty_chunks[chunk_index as usize] = true;
        }
    }

    /// Get the composited albedo atlas view for sampling in terrain shader.
    pub fn albedo_atlas_view(&self) -> &wgpu::TextureView {
        &self.albedo_atlas_view
    }

    /// Get the composited normal atlas view for sampling in terrain shader.
    pub fn normal_atlas_view(&self) -> &wgpu::TextureView {
        &self.normal_atlas_view
    }

    /// Update LOD levels for all chunks based on camera distance.
    ///
    /// Returns `true` if any chunk's LOD changed (needs re-composite).
    ///
    /// # Arguments
    /// * `camera_pos` - Camera position in world coordinates [x, y, z]
    /// * `chunk_centers` - Pre-calculated center position of each chunk [x, y, z]
    /// * `lod_config` - LOD distance configuration
    pub fn update_lod(
        &mut self,
        camera_pos: [f32; 3],
        chunk_centers: &[[f32; 3]],
        lod_config: &LodConfig,
    ) -> bool {
        let mut any_changed = false;
        let num_chunks = self.config.total_chunks().min(chunk_centers.len() as u32);

        for chunk_idx in 0..num_chunks as usize {
            let center = chunk_centers[chunk_idx];
            let dx = camera_pos[0] - center[0];
            let dy = camera_pos[1] - center[1];
            let dz = camera_pos[2] - center[2];
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();

            let new_lod = lod_config.lod_for_distance(distance);

            if self.chunk_lod[chunk_idx] != new_lod {
                self.chunk_lod[chunk_idx] = new_lod;
                self.dirty_chunks[chunk_idx] = true;
                any_changed = true;
            }
        }

        any_changed
    }

    /// Get the current LOD level for a chunk.
    pub fn get_chunk_lod(&self, chunk_index: usize) -> u8 {
        self.chunk_lod.get(chunk_index).copied().unwrap_or(0)
    }

    /// Get count of dirty chunks (needing re-composite).
    pub fn dirty_chunk_count(&self) -> usize {
        self.dirty_chunks.iter().filter(|&&d| d).count()
    }
}
