//! GPU terrain texture compositing.
//!
//! Produces mipmapped unique albedo and normal atlases. Each source mip is
//! sampled at the same explicit LOD as the destination, matching the original
//! compositor's `cb4[0].x` contract.

use num_traits::ToPrimitive;
use wgpu;

use crate::gpu::{
    buffer_entry, buffer_layout_entry, filtering_sampler_layout_entry, texture_entry,
    texture_layout_entry,
};

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
        for (index, &threshold) in self.distance_thresholds.iter().enumerate() {
            if distance < threshold {
                return u8::try_from(index).unwrap_or(3);
            }
        }
        3
    }
}

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
        Self::for_chunk_grid(16, 16).expect("default compositor dimensions must be valid")
    }
}

impl CompositingConfig {
    /// Creates a 512-pixel-per-chunk atlas for the supplied terrain grid.
    #[must_use]
    pub fn for_chunk_grid(chunks_x: u32, chunks_z: u32) -> Option<Self> {
        const CHUNK_TEXTURE_SIZE: u32 = 512;
        if chunks_x == 0 || chunks_z == 0 {
            return None;
        }
        Some(Self {
            chunk_texture_size: CHUNK_TEXTURE_SIZE,
            chunks_x,
            chunks_z,
            atlas_width: chunks_x.checked_mul(CHUNK_TEXTURE_SIZE)?,
            atlas_height: chunks_z.checked_mul(CHUNK_TEXTURE_SIZE)?,
        })
    }

    #[must_use]
    pub fn total_chunks(&self) -> u32 {
        self.chunks_x * self.chunks_z
    }

    #[must_use]
    pub fn mip_level_count(&self) -> u32 {
        self.chunk_texture_size.ilog2() + 1
    }
}

#[cfg(test)]
mod config_tests {
    use super::CompositingConfig;

    #[test]
    fn tundra_atlas_follows_its_decoded_chunk_grid() {
        let config = CompositingConfig::for_chunk_grid(14, 14).expect("valid chunk grid");
        assert_eq!(config.total_chunks(), 196);
        assert_eq!(config.atlas_width, 7168);
        assert_eq!(config.atlas_height, 7168);
    }

    #[test]
    fn empty_chunk_grids_are_rejected() {
        assert!(CompositingConfig::for_chunk_grid(0, 14).is_none());
        assert!(CompositingConfig::for_chunk_grid(14, 0).is_none());
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompositeParams {
    pub chunk_index: u32,
    pub num_layers: u32,
    pub debug_mode: u32,
    pub num_decal_layers: u32,
    pub lod_level: u32,
    pub hdr_mode: u32,
    pub hdr_scale: f32,
    pub chunks_x: u32,
}

pub struct CompositeBindings<'a> {
    pub terrain_array: &'a wgpu::TextureView,
    pub alpha_atlas: &'a wgpu::TextureView,
    pub chunk_layers: &'a wgpu::Buffer,
    pub texture_scales: &'a wgpu::Buffer,
    pub terrain_sampler: &'a wgpu::Sampler,
    pub alpha_sampler: &'a wgpu::Sampler,
    pub alpha_atlas_hi: &'a wgpu::TextureView,
    pub decal_alpha_atlas: &'a wgpu::TextureView,
    pub chunk_decal_layers: &'a wgpu::Buffer,
    pub decal_instances: &'a wgpu::Buffer,
    pub decal_uv_scales: &'a wgpu::Buffer,
    pub normal_array: &'a wgpu::TextureView,
    pub decal_alpha_atlas_hi: &'a wgpu::TextureView,
    pub decal_diffuse_array: &'a wgpu::TextureView,
    pub decal_opacity_array: &'a wgpu::TextureView,
    pub specular_array: &'a wgpu::TextureView,
}

const PARAMS_ALIGN: u64 = 256;

fn sampled_array_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    texture_layout_entry(
        binding,
        wgpu::ShaderStages::FRAGMENT,
        wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureViewDimension::D2Array,
    )
}

fn storage_buffer_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    buffer_layout_entry(
        binding,
        wgpu::ShaderStages::FRAGMENT,
        wgpu::BufferBindingType::Storage { read_only: true },
    )
}

fn filtering_sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    filtering_sampler_layout_entry(binding, wgpu::ShaderStages::FRAGMENT)
}

fn create_atlas(
    device: &wgpu::Device,
    config: &CompositingConfig,
    label: &str,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: config.atlas_width,
            height: config.atlas_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: config.mip_level_count(),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_mip_views(texture: &wgpu::Texture, mip_count: u32) -> Vec<wgpu::TextureView> {
    (0..mip_count)
        .map(|mip| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            })
        })
        .collect()
}

struct CompositeAtlases {
    albedo: wgpu::Texture,
    albedo_view: wgpu::TextureView,
    albedo_mips: Vec<wgpu::TextureView>,
    normal: wgpu::Texture,
    normal_view: wgpu::TextureView,
    normal_mips: Vec<wgpu::TextureView>,
    specular: wgpu::Texture,
    specular_view: wgpu::TextureView,
    specular_mips: Vec<wgpu::TextureView>,
}

fn create_composite_atlases(device: &wgpu::Device, config: &CompositingConfig) -> CompositeAtlases {
    let mip_count = config.mip_level_count();
    let albedo = create_atlas(
        device,
        config,
        "Composited Unique Albedo Atlas",
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    let albedo_view = albedo.create_view(&wgpu::TextureViewDescriptor::default());
    let albedo_mips = create_mip_views(&albedo, mip_count);
    let normal = create_atlas(
        device,
        config,
        "Composited Unique Normal Atlas",
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let normal_view = normal.create_view(&wgpu::TextureViewDescriptor::default());
    let normal_mips = create_mip_views(&normal, mip_count);
    let specular = create_atlas(
        device,
        config,
        "Composited Unique Specular Atlas",
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let specular_view = specular.create_view(&wgpu::TextureViewDescriptor::default());
    let specular_mips = create_mip_views(&specular, mip_count);
    CompositeAtlases {
        albedo,
        albedo_view,
        albedo_mips,
        normal,
        normal_view,
        normal_mips,
        specular,
        specular_view,
        specular_mips,
    }
}

fn create_composite_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Composite Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(32),
                },
                count: None,
            },
            sampled_array_entry(1),
            sampled_array_entry(2),
            storage_buffer_entry(3),
            storage_buffer_entry(4),
            filtering_sampler_entry(5),
            filtering_sampler_entry(6),
            sampled_array_entry(7),
            sampled_array_entry(8),
            storage_buffer_entry(9),
            storage_buffer_entry(10),
            storage_buffer_entry(11),
            sampled_array_entry(12),
            sampled_array_entry(13),
            sampled_array_entry(14),
            sampled_array_entry(15),
            sampled_array_entry(16),
        ],
    })
}

fn create_composite_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Composite Shader"),
        source: wgpu::ShaderSource::Wgsl(super::shaders::COMPOSITE_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Composite Pipeline Layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Unique Terrain Composite Pipeline"),
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
            targets: &[
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ],
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
    })
}

fn index_usize(index: u32) -> usize {
    usize::try_from(index).expect("GPU resource index must fit usize")
}

pub struct CompositorResources {
    _albedo_atlas: wgpu::Texture,
    albedo_atlas_view: wgpu::TextureView,
    albedo_mip_views: Vec<wgpu::TextureView>,
    _normal_atlas: wgpu::Texture,
    normal_atlas_view: wgpu::TextureView,
    normal_mip_views: Vec<wgpu::TextureView>,
    _specular_atlas: wgpu::Texture,
    specular_atlas_view: wgpu::TextureView,
    specular_mip_views: Vec<wgpu::TextureView>,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    pub config: CompositingConfig,
    dirty_chunks: Vec<bool>,
    chunk_lod: Vec<u8>,
    hdr_scale: Option<f32>,
}

impl CompositorResources {
    /// Creates all unique-map compositor resources.
    ///
    /// # Panics
    ///
    /// Panics if the configured chunk size is not a power of two or if the
    /// configured chunk count cannot be represented by the host address space.
    #[must_use]
    pub fn new(device: &wgpu::Device, config: CompositingConfig) -> Self {
        assert!(config.chunk_texture_size.is_power_of_two());
        let mip_count = config.mip_level_count();
        let atlases = create_composite_atlases(device, &config);

        let total_slots = u64::from(config.total_chunks()) * u64::from(mip_count);
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Composite Params Buffer"),
            size: total_slots * PARAMS_ALIGN,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = create_composite_layout(device);
        let pipeline = create_composite_pipeline(device, &bind_group_layout);

        let chunk_count =
            usize::try_from(config.total_chunks()).expect("chunk count must fit usize");
        Self {
            _albedo_atlas: atlases.albedo,
            albedo_atlas_view: atlases.albedo_view,
            albedo_mip_views: atlases.albedo_mips,
            _normal_atlas: atlases.normal,
            normal_atlas_view: atlases.normal_view,
            normal_mip_views: atlases.normal_mips,
            _specular_atlas: atlases.specular,
            specular_atlas_view: atlases.specular_view,
            specular_mip_views: atlases.specular_mips,
            pipeline,
            bind_group_layout,
            params_buffer,
            config,
            dirty_chunks: vec![true; chunk_count],
            chunk_lod: vec![0; chunk_count],
            hdr_scale: None,
        }
    }

    #[must_use]
    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        bindings: &CompositeBindings<'_>,
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
                        size: wgpu::BufferSize::new(32),
                    }),
                },
                texture_entry(1, bindings.terrain_array),
                texture_entry(2, bindings.alpha_atlas),
                buffer_entry(3, bindings.chunk_layers),
                buffer_entry(4, bindings.texture_scales),
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(bindings.terrain_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(bindings.alpha_sampler),
                },
                texture_entry(7, bindings.alpha_atlas_hi),
                texture_entry(8, bindings.decal_alpha_atlas),
                buffer_entry(9, bindings.chunk_decal_layers),
                buffer_entry(10, bindings.decal_instances),
                buffer_entry(11, bindings.decal_uv_scales),
                texture_entry(12, bindings.normal_array),
                texture_entry(13, bindings.decal_alpha_atlas_hi),
                texture_entry(14, bindings.decal_diffuse_array),
                texture_entry(15, bindings.decal_opacity_array),
                texture_entry(16, bindings.specular_array),
            ],
        })
    }

    fn params_offset(&self, chunk_index: u32, mip: u32) -> u64 {
        let slot =
            u64::from(chunk_index) * u64::from(self.config.mip_level_count()) + u64::from(mip);
        slot * PARAMS_ALIGN
    }

    /// Rebuilds every dirty chunk and every destination mip.
    ///
    /// # Panics
    ///
    /// Panics if a configured GPU resource index or dynamic uniform offset
    /// cannot be represented on the host or by WebGPU.
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
            .filter(|&index| self.dirty_chunks[index_usize(index)])
            .collect();
        if dirty_indices.is_empty() {
            return;
        }

        let mip_count = self.config.mip_level_count();
        for &chunk_index in &dirty_indices {
            let num_layers = chunk_layer_counts
                .get(index_usize(chunk_index))
                .copied()
                .unwrap_or(1);
            let num_decal_layers = chunk_decal_layer_counts
                .get(index_usize(chunk_index))
                .copied()
                .unwrap_or(0);
            for mip in 0..mip_count {
                let params = CompositeParams {
                    chunk_index,
                    num_layers,
                    debug_mode,
                    num_decal_layers,
                    lod_level: mip,
                    hdr_mode: u32::from(self.hdr_scale.is_some()),
                    hdr_scale: self.hdr_scale.unwrap_or(1.0),
                    chunks_x: self.config.chunks_x,
                };
                queue.write_buffer(
                    &self.params_buffer,
                    self.params_offset(chunk_index, mip),
                    bytemuck::bytes_of(&params),
                );
            }
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Composite All Encoder"),
        });
        for &chunk_index in &dirty_indices {
            let grid_z = chunk_index / self.config.chunks_x;
            let grid_x = chunk_index % self.config.chunks_x;
            for mip in 0..mip_count {
                let chunk_size = (self.config.chunk_texture_size >> mip).max(1);
                let viewport_x = grid_x * chunk_size;
                let viewport_y = grid_z * chunk_size;
                let dynamic_offset = u32::try_from(self.params_offset(chunk_index, mip))
                    .expect("composite dynamic offset must fit u32");
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Composite Chunk Mip"),
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: &self.albedo_mip_views[index_usize(mip)],
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: &self.normal_mip_views[index_usize(mip)],
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: &self.specular_mip_views[index_usize(mip)],
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_viewport(
                    viewport_x.to_f32().expect("viewport X must fit f32"),
                    viewport_y.to_f32().expect("viewport Y must fit f32"),
                    chunk_size.to_f32().expect("viewport width must fit f32"),
                    chunk_size.to_f32().expect("viewport height must fit f32"),
                    0.0,
                    1.0,
                );
                pass.set_scissor_rect(viewport_x, viewport_y, chunk_size, chunk_size);
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, bind_group, &[dynamic_offset]);
                pass.draw(0..3, 0..1);
            }
            self.dirty_chunks[index_usize(chunk_index)] = false;
        }
        queue.submit(std::iter::once(encoder.finish()));
        log::info!(
            "Composited {} chunks across {mip_count} unique-texture mip levels",
            dirty_indices.len(),
        );
    }

    pub fn set_hdr_scale(&mut self, hdr_scale: Option<f32>) {
        if self.hdr_scale != hdr_scale {
            self.hdr_scale = hdr_scale;
            self.mark_all_dirty();
        }
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty_chunks.fill(true);
    }

    pub fn mark_chunk_dirty(&mut self, chunk_index: u32) {
        if let Ok(index) = usize::try_from(chunk_index)
            && let Some(dirty) = self.dirty_chunks.get_mut(index)
        {
            *dirty = true;
        }
    }

    #[must_use]
    pub fn albedo_atlas_view(&self) -> &wgpu::TextureView {
        &self.albedo_atlas_view
    }

    #[must_use]
    pub fn normal_atlas_view(&self) -> &wgpu::TextureView {
        &self.normal_atlas_view
    }

    /// Returns the composited colored-specular atlas used by lit terrain.
    #[must_use]
    pub fn specular_atlas_view(&self) -> &wgpu::TextureView {
        &self.specular_atlas_view
    }

    pub fn update_lod(
        &mut self,
        camera_pos: [f32; 3],
        chunk_centers: &[[f32; 3]],
        lod_config: &LodConfig,
    ) -> bool {
        let mut any_changed = false;
        let chunk_count = usize::try_from(self.config.total_chunks())
            .unwrap_or(usize::MAX)
            .min(chunk_centers.len());
        for (chunk_index, center) in chunk_centers.iter().enumerate().take(chunk_count) {
            let delta = [
                camera_pos[0] - center[0],
                camera_pos[1] - center[1],
                camera_pos[2] - center[2],
            ];
            let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
            let new_lod = lod_config.lod_for_distance(distance);
            if self.chunk_lod[chunk_index] != new_lod {
                self.chunk_lod[chunk_index] = new_lod;
                any_changed = true;
            }
        }
        any_changed
    }

    #[must_use]
    pub fn dirty_chunk_count(&self) -> usize {
        self.dirty_chunks.iter().filter(|&&dirty| dirty).count()
    }
}
