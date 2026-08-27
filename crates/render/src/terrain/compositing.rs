//! GPU Terrain Texture Compositing.
//!
//! Pre-composites terrain splat layers into an 8192×8192 atlas texture.
//! Each of the 256 chunks (16×16 grid) gets a 512×512 region.
//!
//! The compositor uses a separate params buffer per chunk. To avoid the
//! problem of `queue.write_buffer` only taking effect on the next submit,
//! we use a staging buffer large enough for ALL 256 chunks and use dynamic
//! offsets so each draw reads from a different slot.

use num_traits::ToPrimitive;
use wgpu;

/// LOD level configuration for distance-based compositing quality.
#[derive(Debug, Clone)]
pub struct LodConfig {
    pub distance_thresholds: [f32; 4],
    pub size_multipliers: [f32; 4],
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            distance_thresholds: [200.0, 500.0, 1000.0, f32::MAX],
            size_multipliers: [1.0, 0.5, 0.25, 0.125],
        }
    }
}

impl LodConfig {
    #[must_use]
    pub fn with_distances(d0: f32, d1: f32, d2: f32) -> Self {
        Self {
            distance_thresholds: [d0, d1, d2, f32::MAX],
            ..Default::default()
        }
    }

    #[must_use]
    pub fn lod_for_distance(&self, distance: f32) -> u8 {
        for (i, &threshold) in self.distance_thresholds.iter().enumerate() {
            if distance < threshold {
                return u8::try_from(i).unwrap_or(3);
            }
        }
        3
    }
}

/// Configuration for terrain texture compositing.
#[derive(Debug, Clone)]
pub struct CompositingConfig {
    pub chunk_texture_size: u32,
    pub chunks_x: u32,
    pub chunks_z: u32,
    pub atlas_width: u32,
    pub atlas_height: u32,
}

impl Default for CompositingConfig {
    fn default() -> Self {
        Self {
            chunk_texture_size: 512,
            chunks_x: 16,
            chunks_z: 16,
            atlas_width: 8192,
            atlas_height: 8192,
        }
    }
}

impl CompositingConfig {
    #[must_use]
    pub fn total_chunks(&self) -> u32 {
        self.chunks_x * self.chunks_z
    }
}

/// Per-chunk uniform data. Must be 256-byte aligned for dynamic offsets.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompositeParams {
    pub chunk_index: u32,
    pub num_layers: u32,
    pub debug_mode: u32,
    pub num_decal_layers: u32,
}

/// Aligned params slot (256 bytes to satisfy wgpu dynamic offset alignment).
const PARAMS_ALIGN: u64 = 256;

fn sampled_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

fn storage_buffer_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn filtering_sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn create_albedo_atlas(device: &wgpu::Device, config: &CompositingConfig) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Composited Albedo Atlas"),
        size: wgpu::Extent3d {
            width: config.atlas_width,
            height: config.atlas_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// GPU resources for terrain texture compositing.
pub struct CompositorResources {
    #[allow(dead_code)] // Kept alive to back albedo_atlas_view
    albedo_atlas: wgpu::Texture,
    albedo_atlas_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    /// Single buffer holding 256 aligned `CompositeParams` slots.
    params_buffer: wgpu::Buffer,
    pub config: CompositingConfig,
    dirty_chunks: Vec<bool>,
    chunk_lod: Vec<u8>,
}

impl CompositorResources {
    #[must_use]
    pub fn new(device: &wgpu::Device, config: CompositingConfig) -> Self {
        use super::shaders::COMPOSITE_SHADER;

        let albedo_atlas = create_albedo_atlas(device, &config);
        let albedo_atlas_view = albedo_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        // Params buffer: 256 slots × 256 bytes each = 64 KiB
        let total_chunks = config.total_chunks();
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Composite Params Buffer"),
            size: u64::from(total_chunks) * PARAMS_ALIGN,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Bind Group Layout"),
            entries: &[
                // binding 0: CompositeParams uniform (dynamic offset)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(
                            u64::try_from(std::mem::size_of::<CompositeParams>())
                                .unwrap_or_default(),
                        ),
                    },
                    count: None,
                },
                sampled_texture_entry(1),
                sampled_texture_entry(2),
                storage_buffer_entry(3),
                storage_buffer_entry(4),
                filtering_sampler_entry(5),
                filtering_sampler_entry(6),
                sampled_texture_entry(7),
                sampled_texture_entry(8),
                storage_buffer_entry(9),
                storage_buffer_entry(10),
                storage_buffer_entry(11),
            ],
        });

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
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let n = usize::try_from(total_chunks).unwrap_or_default();
        Self {
            albedo_atlas,
            albedo_atlas_view,
            pipeline,
            bind_group_layout,
            params_buffer,
            config,
            dirty_chunks: vec![true; n],
            chunk_lod: vec![0; n],
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        terrain_array_view: &wgpu::TextureView,
        alpha_atlas_view: &wgpu::TextureView,
        alpha_atlas_hi_view: &wgpu::TextureView,
        chunk_layers_buffer: &wgpu::Buffer,
        texture_scales_buffer: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        alpha_sampler: &wgpu::Sampler,
        decal_alpha_atlas_view: &wgpu::TextureView,
        chunk_decal_layers_buffer: &wgpu::Buffer,
        decal_instances_buffer: &wgpu::Buffer,
        decal_uv_scales_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.params_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(
                            u64::try_from(std::mem::size_of::<CompositeParams>())
                                .unwrap_or_default(),
                        ),
                    }),
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
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(alpha_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(alpha_atlas_hi_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(decal_alpha_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: chunk_decal_layers_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: decal_instances_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: decal_uv_scales_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// Composite all dirty chunks in a single encoder submission.
    ///
    /// Writes all params up front, then records all render passes into one
    /// encoder, then the caller submits. No per-chunk submit needed because
    /// we use dynamic buffer offsets.
    pub fn composite_all_dirty(
        &mut self,
        bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        chunk_layer_counts: &[u32],
        chunk_decal_layer_counts: &[u32],
        device: &wgpu::Device,
        debug_mode: u32,
    ) {
        let dirty_indices: Vec<u32> = (0..self.config.total_chunks())
            .filter(|&i| self.dirty_chunks[i as usize])
            .collect();

        if dirty_indices.is_empty() {
            return;
        }

        log::debug!("Compositing {} dirty chunks", dirty_indices.len());

        // Stage 1: Write ALL params into the buffer at their aligned offsets.
        // We write them all before creating the encoder so they're all visible.
        for &chunk_idx in &dirty_indices {
            let num_layers = chunk_layer_counts
                .get(chunk_idx as usize)
                .copied()
                .unwrap_or(1);

            let num_decal_layers = chunk_decal_layer_counts
                .get(chunk_idx as usize)
                .copied()
                .unwrap_or(0);

            let params = CompositeParams {
                chunk_index: chunk_idx,
                num_layers,
                debug_mode,
                num_decal_layers,
            };

            let offset = u64::from(chunk_idx) * PARAMS_ALIGN;
            queue.write_buffer(&self.params_buffer, offset, bytemuck::bytes_of(&params));
        }

        // Stage 2: Record all render passes into a single encoder.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Composite All Encoder"),
        });

        let chunk_size = self.config.chunk_texture_size;

        for &chunk_idx in &dirty_indices {
            // chunk_idx = Z*16+X (XTT layout: grid_x=Z, grid_z=X)
            let grid_x = chunk_idx / self.config.chunks_z; // = Z
            let grid_z = chunk_idx % self.config.chunks_z; // = X

            // Atlas layout: X→U (columns), Z→V (rows) to match terrain_gpu's
            // sample_uv = (worldX/extent, worldZ/extent)
            // grid_z = X direction → viewport_x, grid_x = Z direction → viewport_y
            let viewport_x = grid_z * chunk_size;
            let viewport_y = grid_x * chunk_size;

            let dynamic_offset =
                u32::try_from(u64::from(chunk_idx) * PARAMS_ALIGN).unwrap_or_default();

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Composite Chunk"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.albedo_atlas_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                pass.set_viewport(
                    viewport_x.to_f32().unwrap_or_default(),
                    viewport_y.to_f32().unwrap_or_default(),
                    chunk_size.to_f32().unwrap_or_default(),
                    chunk_size.to_f32().unwrap_or_default(),
                    0.0,
                    1.0,
                );
                pass.set_scissor_rect(viewport_x, viewport_y, chunk_size, chunk_size);
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, bind_group, &[dynamic_offset]);
                pass.draw(0..3, 0..1);
            }

            self.dirty_chunks[chunk_idx as usize] = false;
        }

        // Single submit for all chunks
        queue.submit(std::iter::once(encoder.finish()));

        log::info!("Composited {} chunks to GPU atlas", dirty_indices.len());
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty_chunks.fill(true);
    }

    pub fn mark_chunk_dirty(&mut self, chunk_index: u32) {
        if (chunk_index as usize) < self.dirty_chunks.len() {
            self.dirty_chunks[chunk_index as usize] = true;
        }
    }

    #[must_use]
    pub fn albedo_atlas_view(&self) -> &wgpu::TextureView {
        &self.albedo_atlas_view
    }

    pub fn update_lod(
        &mut self,
        camera_pos: [f32; 3],
        chunk_centers: &[[f32; 3]],
        lod_config: &LodConfig,
    ) -> bool {
        let mut any_changed = false;
        let num_chunks = usize::try_from(self.config.total_chunks())
            .unwrap_or(usize::MAX)
            .min(chunk_centers.len());

        for (chunk_idx, center) in chunk_centers.iter().enumerate().take(num_chunks) {
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

    #[must_use]
    pub fn dirty_chunk_count(&self) -> usize {
        self.dirty_chunks.iter().filter(|&&d| d).count()
    }
}
