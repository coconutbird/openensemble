//! GPU resources for foliage rendering.

use std::collections::BTreeSet;

use num_traits::ToPrimitive;
use render::terrain::LightingParams;
use render::wgpu;
use render::wgpu::util::DeviceExt;

use super::FoliageConfig;
use crate::gpu::xtd_packed_to_world;
use crate::types::FoliageSet;

mod gpu;
mod types;

use gpu::{
    FoliageMaterialBindings, FoliageParamsBindings, create_foliage_bind_group_layouts,
    create_foliage_material_bind_group, create_foliage_params_bind_group, create_foliage_pipeline,
    create_foliage_samplers, create_foliage_shadow_pipeline, create_opacity_texture,
    create_uploaded_rgba_texture, initial_foliage_params,
};
pub use types::{
    ChunkInfoUniform, FoliageDrawCall, FoliageParamsUniform, FoliageResources, FoliageSetResources,
    FoliageWorldBindings,
};
use types::{FallbackFoliageWorldResources, FoliageDrawInfo};

/// Extracts one (`local_grid_index`, `blade_type`) entry per foliage blade.
///
/// The fresh PC shader consumes a 32-bit index whose upper 16 bits select the
/// blade geometry and whose lower 16 bits are
/// `local_grid_index * 10 + vertex_in_blade`. The local XTT decoder has already
/// converted the file's big-endian bytes into those words. The shader's decimal
/// decomposition is hard-coded to ten vertices, so sets with any other geometry
/// stride cannot use this path.
pub(crate) fn parse_foliage_index_buffer(
    indices: &[u32],
    num_verts_per_blade: u32,
) -> Vec<[u32; 2]> {
    const INDEX_VERTICES_PER_BLADE: u32 = 10;
    let mut blade_entries = Vec::new();
    if indices.is_empty() || num_verts_per_blade != INDEX_VERTICES_PER_BLADE {
        return blade_entries;
    }

    let mut seen = BTreeSet::new();
    for &packed in indices {
        let index_part = packed & 0xFFFF;
        if index_part == 0xFFFF {
            continue;
        }

        let vertex_in_blade = index_part % INDEX_VERTICES_PER_BLADE;
        if vertex_in_blade == 0 {
            let entry = [index_part / INDEX_VERTICES_PER_BLADE, packed >> 16];
            if seen.insert(entry) {
                blade_entries.push(entry);
            }
        }
    }
    blade_entries
}

impl FoliageResources {
    /// Create foliage rendering resources.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        camera_bind_group: &wgpu::BindGroup,
        shadow_camera_bind_groups: &[wgpu::BindGroup],
    ) -> Self {
        let min_offset_alignment = device.limits().min_uniform_buffer_offset_alignment;
        let (params_bind_group_layout, material_bind_group_layout) =
            create_foliage_bind_group_layouts(device);
        let params_size = u64::try_from(std::mem::size_of::<FoliageParamsUniform>())
            .expect("foliage params size must fit u64");
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foliage Params Buffer"),
            size: params_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline = create_foliage_pipeline(
            device,
            surface_format,
            camera_bind_group_layout,
            &params_bind_group_layout,
            &material_bind_group_layout,
        );
        let shadow_pipeline = create_foliage_shadow_pipeline(
            device,
            camera_bind_group_layout,
            &params_bind_group_layout,
            &material_bind_group_layout,
        );

        Self {
            pipeline,
            shadow_pipeline,
            camera_bind_group: camera_bind_group.clone(),
            shadow_camera_bind_groups: shadow_camera_bind_groups.to_vec(),
            params_buffer,
            chunk_info_buffer: None,
            params_bind_group_layout,
            material_bind_group_layout,
            set_resources: Vec::new(),
            params_bind_group: None,
            shadow_params_bind_group: None,
            draw_calls: Vec::new(),
            min_offset_alignment,
            config: FoliageConfig::default(),
            blade_map_texture: None,
            blade_map_view: None,
            params: None,
        }
    }

    /// Create GPU resources for a single foliage set.
    pub fn create_set_resources(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        set: &FoliageSet,
    ) -> Option<FoliageSetResources> {
        if set.albedo_pixels.is_empty() {
            log::warn!("Skipping foliage set '{}': no albedo texture", set.name);
            return None;
        }

        let (albedo_texture, albedo_view) = create_uploaded_rgba_texture(
            device,
            queue,
            &format!("Foliage Albedo: {}", set.name),
            [set.albedo_width, set.albedo_height],
            &set.albedo_pixels,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let (opacity_texture, opacity_view) = create_opacity_texture(device, queue, set);
        let (blade_positions, blade_normals, num_blade_types, num_verts) = if set
            .blade_positions
            .is_empty()
        {
            let inferred_types = set
                .albedo_width
                .checked_div(set.albedo_height)
                .unwrap_or(1)
                .max(1);
            log::info!(
                "Foliage '{}': inferred {} blade types from {}x{} texture",
                set.name,
                inferred_types,
                set.albedo_width,
                set.albedo_height,
            );
            Self::generate_default_blade_geometry(inferred_types)
        } else {
            log::info!(
                "Foliage '{}': using loaded blade geometry ({} types, {} verts/blade, {} total)",
                set.name,
                set.num_blade_types,
                set.num_verts_per_blade,
                set.blade_positions.len()
            );
            (
                set.blade_positions.clone(),
                set.blade_normals.clone(),
                set.num_blade_types,
                set.num_verts_per_blade,
            )
        };

        let blade_texel_count = num_verts
            .checked_mul(num_blade_types)
            .expect("foliage blade texel count must fit u32");
        let blade_positions_texture = Self::create_blade_texture(
            device,
            queue,
            &blade_positions,
            &format!("Foliage Blade Positions: {}", set.name),
            blade_texel_count,
        );
        let blade_positions_view =
            blade_positions_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let blade_normals_texture = Self::create_blade_texture(
            device,
            queue,
            &blade_normals,
            &format!("Foliage Blade Normals: {}", set.name),
            blade_texel_count,
        );
        let blade_normals_view =
            blade_normals_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (material_sampler, blade_sampler) = create_foliage_samplers(device);
        let material_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Foliage Material Params: {}", set.name)),
            contents: bytemuck::cast_slice(&[set.backside_shadow_scalar, 0.0, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let material_bind_group = create_foliage_material_bind_group(
            device,
            &self.material_bind_group_layout,
            &set.name,
            &FoliageMaterialBindings {
                albedo: &albedo_view,
                opacity: &opacity_view,
                material_sampler: &material_sampler,
                blade_positions: &blade_positions_view,
                blade_normals: &blade_normals_view,
                blade_sampler: &blade_sampler,
                material_params: &material_params_buffer,
            },
        );

        Some(FoliageSetResources {
            _albedo_texture: albedo_texture,
            _albedo_view: albedo_view,
            _opacity_texture: opacity_texture,
            _opacity_view: opacity_view,
            _blade_positions_texture: blade_positions_texture,
            _blade_positions_view: blade_positions_view,
            _blade_normals_texture: blade_normals_texture,
            _blade_normals_view: blade_normals_view,
            material_bind_group,
            _material_params_buffer: material_params_buffer,
            num_blade_types,
            num_verts_per_blade: num_verts,
        })
    }

    /// Generate default grass blade geometry (simple quad strip) for N blade types.
    ///
    /// Each blade type gets UVs covering its horizontal slice of the atlas texture.
    /// For a texture with N types, type i covers U range [i/N, (i+1)/N].
    fn generate_default_blade_geometry(
        num_blade_types: u32,
    ) -> (Vec<[f32; 4]>, Vec<[f32; 4]>, u32, u32) {
        let num_verts_per_blade = 10u32;

        let total_verts = usize::try_from(
            num_verts_per_blade
                .checked_mul(num_blade_types)
                .expect("foliage vertex count must fit u32"),
        )
        .expect("foliage vertex count must fit usize");
        let mut positions = Vec::with_capacity(total_verts);
        let mut normals = Vec::with_capacity(total_verts);

        // Simple grass blade: 5 quads stacked vertically
        // Width tapers from base to top
        // Note: game blade geometry is stored at world scale (no shader scaling).
        // Typical game blades are ~1.5 units tall, ~0.3 wide.
        let blade_height = 1.5;
        let base_width = 0.3;

        for blade_type in 0..num_blade_types {
            // UV range for this blade type in the atlas
            let type_count = num_blade_types
                .to_f32()
                .expect("foliage blade type count must fit f32");
            let u_min = blade_type
                .to_f32()
                .expect("foliage blade type must fit f32")
                / type_count;
            let u_max = (blade_type + 1)
                .to_f32()
                .expect("foliage blade type must fit f32")
                / type_count;

            for step in 0_u32..5 {
                let t = step.to_f32().expect("blade step must fit f32") / 4.0;
                let y = t * blade_height;
                let width = base_width * (1.0 - t * 0.8); // Taper to 20% at top

                // Left vertex
                positions.push([-width, y, 0.0, u_min]); // x, y, z, u
                normals.push([0.0, 0.0, 1.0, 1.0 - t]); // nx, ny, nz, runtime v

                // Right vertex
                positions.push([width, y, 0.0, u_max]); // x, y, z, u
                normals.push([0.0, 0.0, 1.0, 1.0 - t]); // nx, ny, nz, runtime v
            }
        }

        (positions, normals, num_blade_types, num_verts_per_blade)
    }

    /// Create a 1D texture for blade vertex data.
    fn create_blade_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[[f32; 4]],
        label: &str,
        width: u32,
    ) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        if !data.is_empty() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 16), // 4 floats * 4 bytes
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: width.max(1),
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }

        texture
    }

    /// Create the params bind group with heightmap and chunk info dynamic buffer.
    ///
    /// This requires terrain data to create the heightmap texture.
    fn ensure_chunk_info_buffer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        num_verts_per_blade: u32,
    ) {
        if self.chunk_info_buffer.is_some() {
            return;
        }
        let uniform_size = u32::try_from(std::mem::size_of::<ChunkInfoUniform>())
            .expect("chunk info size must fit u32");
        let aligned_size = Self::align_up(uniform_size, self.min_offset_alignment);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foliage Chunk Info Buffer (initial)"),
            size: u64::from(aligned_size),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let default_chunk = ChunkInfoUniform {
            chunk_offset: [0.0, 0.0],
            num_verts_per_blade: num_verts_per_blade
                .to_f32()
                .expect("foliage vertex count must fit f32"),
            blade_data_offset: 0.0,
        };
        queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&default_chunk));
        self.chunk_info_buffer = Some(buffer);
    }

    fn ensure_blade_map(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.blade_map_view.is_some() {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Foliage Dummy Blade Map"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Uint,
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
            &[0; 8],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.blade_map_view = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.blade_map_texture = Some(texture);
    }

    fn create_fallback_world_resources(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> FallbackFoliageWorldResources {
        let blackmap = Self::create_dummy_texture(device, queue, "Blackmap", [0, 0, 0, 0]);
        let unexplored = Self::create_dummy_texture(device, queue, "Unexplored", [0, 0, 0, 0]);
        FallbackFoliageWorldResources {
            shadow: Self::create_dummy_shadow_view(device, queue),
            blackmap: blackmap.create_view(&wgpu::TextureViewDescriptor::default()),
            unexplored: unexplored.create_view(&wgpu::TextureViewDescriptor::default()),
            local_lights: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Foliage Empty Local Lights"),
                contents: &[0; 20 * 8 * 16],
                usage: wgpu::BufferUsages::STORAGE,
            }),
        }
    }

    pub fn create_params_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_data: &crate::types::RawXtdData,
        world: Option<&FoliageWorldBindings<'_>>,
    ) {
        let Some(first_set) = self.set_resources.first() else {
            log::warn!("Cannot create params bind group: no foliage sets loaded");
            return;
        };
        let num_verts_per_blade = first_set.num_verts_per_blade;
        let params = initial_foliage_params(terrain_data, first_set, &self.config);
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));
        self.params = Some(params);

        let num_verts = terrain_data.num_verts_per_axis;
        let heightmap_texture =
            Self::create_heightmap_texture(device, queue, terrain_data, num_verts);
        let heightmap_view = heightmap_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let heightmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Foliage Heightmap Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        self.ensure_chunk_info_buffer(device, queue, num_verts_per_blade);
        self.ensure_blade_map(device, queue);

        let fallback = world
            .is_none()
            .then(|| Self::create_fallback_world_resources(device, queue));
        let fallback_bindings = fallback
            .as_ref()
            .map(FallbackFoliageWorldResources::bindings);
        let world = world
            .or(fallback_bindings.as_ref())
            .expect("world or fallback foliage bindings must exist");
        let chunk_buffer = self
            .chunk_info_buffer
            .as_ref()
            .expect("chunk info buffer was created above");
        let blade_map = self
            .blade_map_view
            .as_ref()
            .expect("blade map was created above");
        self.params_bind_group = Some(create_foliage_params_bind_group(
            device,
            &self.params_bind_group_layout,
            &FoliageParamsBindings {
                params_buffer: &self.params_buffer,
                chunk_buffer,
                heightmap: &heightmap_view,
                heightmap_sampler: &heightmap_sampler,
                shadow: world.shadow,
                blackmap: world.blackmap,
                unexplored: world.unexplored,
                blade_map,
                local_lights: world.local_lights,
            },
        ));
        let caster_shadow = Self::create_dummy_shadow_view(device, queue);
        self.shadow_params_bind_group = Some(create_foliage_params_bind_group(
            device,
            &self.params_bind_group_layout,
            &FoliageParamsBindings {
                params_buffer: &self.params_buffer,
                chunk_buffer,
                heightmap: &heightmap_view,
                heightmap_sampler: &heightmap_sampler,
                shadow: &caster_shadow,
                blackmap: world.blackmap,
                unexplored: world.unexplored,
                blade_map,
                local_lights: world.local_lights,
            },
        ));
        log::info!("Created foliage params bind group with {num_verts}x{num_verts} heightmap");
    }

    /// Create a 1x1 dummy texture with the given RGBA pixel.
    fn create_dummy_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        pixel: [u8; 4],
    ) -> wgpu::Texture {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("Foliage Dummy {label} Texture")),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        tex
    }

    fn create_dummy_shadow_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("Foliage Dummy Shadow Array"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255, 0, 0, 255],
        );
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
    }

    /// Align `value` up to the next multiple of `alignment`.
    fn align_up(value: u32, alignment: u32) -> u32 {
        (value + alignment - 1) & !(alignment - 1)
    }

    /// Parse a QN chunk's index buffer for one set, extracting (`grid_position`, `blade_type`) pairs.
    ///
    /// Index buffer format verified directly against the scenario XTT bytes:
    /// Each 32-bit entry: upper 16 bits = `blade_type`, lower 16 bits = localIndex * numVertsPerBlade + vert
    /// Entries for one blade are followed by a 0xFFFF strip reset marker.
    /// Build draw calls and chunk info buffer from QN chunk data.
    ///
    /// Parses index buffers to determine which blades are active and their types.
    /// Creates a blade map texture for the shader to look up blade placement.
    fn collect_draw_infos(
        &self,
        qn_chunks: &[crate::types::FoliageQNChunk],
        chunks_per_axis: u32,
    ) -> Vec<FoliageDrawInfo> {
        let mut draw_infos = Vec::new();
        for (chunk_index, chunk) in qn_chunks.iter().enumerate() {
            let (Ok(chunk_x), Ok(chunk_z)) =
                (u32::try_from(chunk.grid_x), u32::try_from(chunk.grid_z))
            else {
                log::warn!(
                    "Skipping foliage QN parent {} with negative XTD grid coordinates ({}, {})",
                    chunk.qn_parent_index,
                    chunk.grid_x,
                    chunk.grid_z
                );
                continue;
            };
            if chunk_x >= chunks_per_axis || chunk_z >= chunks_per_axis {
                log::warn!(
                    "Skipping foliage QN parent {} outside {chunks_per_axis}x{chunks_per_axis} world grid: ({chunk_x}, {chunk_z})",
                    chunk.qn_parent_index
                );
                continue;
            }
            if chunk_index < 5 {
                log::debug!(
                    "  QN[{chunk_index}] parent_idx={}, world_chunk=({chunk_x},{chunk_z}), sets={}, set_indices={:?}",
                    chunk.qn_parent_index,
                    chunk.num_sets,
                    chunk.set_indices
                );
            }

            let set_count =
                usize::try_from(chunk.num_sets).expect("foliage set count must fit usize");
            for set_slot in 0..set_count {
                let Some(&raw_set_index) = chunk.set_indices.get(set_slot) else {
                    continue;
                };
                let Ok(set_index) = usize::try_from(raw_set_index) else {
                    continue;
                };
                let Some(set_resources) = self.set_resources.get(set_index) else {
                    continue;
                };
                let Some(index_buffer) = chunk.index_buffers.get(set_slot) else {
                    continue;
                };
                let blades =
                    parse_foliage_index_buffer(index_buffer, set_resources.num_verts_per_blade);
                if !blades.is_empty() {
                    draw_infos.push(FoliageDrawInfo {
                        chunk_x,
                        chunk_z,
                        set_index,
                        num_verts_per_blade: set_resources.num_verts_per_blade,
                        blades,
                    });
                }
            }
        }
        draw_infos
    }

    fn build_chunk_data(
        &mut self,
        draw_infos: &[FoliageDrawInfo],
        aligned_slot: u32,
    ) -> (Vec<u8>, Vec<[u32; 2]>) {
        let draw_count =
            u32::try_from(draw_infos.len()).expect("foliage draw-call count must fit u32");
        let buffer_size = draw_count
            .checked_mul(aligned_slot)
            .expect("foliage chunk buffer size must fit u32");
        let mut chunk_data = vec![
            0;
            usize::try_from(buffer_size)
                .expect("foliage chunk buffer size must fit usize")
        ];
        let total_blades = draw_infos.iter().map(|info| info.blades.len()).sum();
        let mut blade_map_data = Vec::with_capacity(total_blades);
        let mut blade_data_offset = 0_u32;

        for (draw_index, info) in draw_infos.iter().enumerate() {
            let draw_index = u32::try_from(draw_index).expect("foliage draw index must fit u32");
            let dynamic_offset = draw_index
                .checked_mul(aligned_slot)
                .expect("foliage dynamic offset must fit u32");
            let byte_offset =
                usize::try_from(dynamic_offset).expect("foliage offset must fit usize");
            let north_offset = info
                .chunk_x
                .checked_mul(64)
                .expect("foliage X offset must fit u32")
                .to_f32()
                .expect("foliage X offset must fit f32");
            let east_offset = info
                .chunk_z
                .checked_mul(64)
                .expect("foliage Z offset must fit u32")
                .to_f32()
                .expect("foliage Z offset must fit f32");
            let chunk_info = ChunkInfoUniform {
                chunk_offset: [north_offset, east_offset],
                num_verts_per_blade: info
                    .num_verts_per_blade
                    .to_f32()
                    .expect("foliage vertex count must fit f32"),
                blade_data_offset: blade_data_offset
                    .to_f32()
                    .expect("foliage blade offset must fit f32"),
            };
            let bytes = bytemuck::bytes_of(&chunk_info);
            chunk_data[byte_offset..byte_offset + bytes.len()].copy_from_slice(bytes);

            let active_blades =
                u32::try_from(info.blades.len()).expect("active blade count must fit u32");
            blade_map_data.extend_from_slice(&info.blades);
            self.draw_calls.push(FoliageDrawCall {
                dynamic_offset,
                set_index: info.set_index,
                num_verts_per_blade: info.num_verts_per_blade,
                num_active_blades: active_blades,
            });
            blade_data_offset = blade_data_offset
                .checked_add(active_blades)
                .expect("foliage blade count must fit u32");
        }
        (chunk_data, blade_map_data)
    }

    fn upload_chunk_info_buffer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
    ) {
        let size = u64::try_from(data.len()).expect("foliage chunk buffer size must fit u64");
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foliage Chunk Info Buffer"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, data);
        self.chunk_info_buffer = Some(buffer);
    }

    fn upload_blade_map(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mut data: Vec<[u32; 2]>,
    ) -> [u32; 3] {
        let entry_count = u32::try_from(data.len()).expect("foliage blade-map size must fit u32");
        let width = entry_count.clamp(1, 8192);
        let height = entry_count.div_ceil(width).max(1);
        let texel_count = width
            .checked_mul(height)
            .expect("foliage blade-map dimensions must fit u32");
        data.resize(
            usize::try_from(texel_count).expect("foliage blade-map size must fit usize"),
            [0, 0],
        );

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Foliage Blade Map"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Uint,
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
            bytemuck::cast_slice(&data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 8),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.blade_map_view = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.blade_map_texture = Some(texture);
        [entry_count, width, height]
    }

    pub fn build_draw_calls(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        qn_chunks: &[crate::types::FoliageQNChunk],
        num_verts_per_axis: u32,
    ) {
        self.draw_calls.clear();
        if qn_chunks.is_empty() || self.set_resources.is_empty() {
            log::info!("No QN chunks or sets — no foliage draw calls");
            return;
        }
        let Some(chunks_per_axis) = num_verts_per_axis
            .checked_div(64)
            .filter(|&count| count > 0)
        else {
            log::warn!("Terrain is too small to contain foliage chunks");
            return;
        };
        let uniform_size = u32::try_from(std::mem::size_of::<ChunkInfoUniform>())
            .expect("chunk info size must fit u32");
        let aligned_slot = Self::align_up(uniform_size, self.min_offset_alignment);
        let draw_infos = self.collect_draw_infos(qn_chunks, chunks_per_axis);
        if draw_infos.is_empty() {
            log::info!("No valid foliage draw calls generated from index buffers");
            return;
        }

        let (chunk_data, blade_map_data) = self.build_chunk_data(&draw_infos, aligned_slot);
        self.upload_chunk_info_buffer(device, queue, &chunk_data);
        let [entry_count, width, height] = self.upload_blade_map(device, queue, blade_map_data);
        log::info!(
            "Built {} foliage draw calls from {} QN chunks ({entry_count} total blades, blade map {width}x{height}, grid {chunks_per_axis}x{chunks_per_axis})",
            self.draw_calls.len(),
            qn_chunks.len(),
        );
    }

    /// Synchronizes foliage with the shared lighting constants for this frame.
    pub fn update_frame(&mut self, queue: &wgpu::Queue, lighting: &LightingParams, time: f32) {
        let Some(params) = &mut self.params else {
            return;
        };
        params.camera_pos_time = [
            lighting.world_camera_pos[0],
            lighting.world_camera_pos[1],
            lighting.world_camera_pos[2],
            time,
        ];
        params.dir_light_vec = lighting.dir_light_vec;
        params.dir_light_color = lighting.dir_light_color;
        params.fog_params = lighting.fog_params;
        params.fog_color = lighting.fog_color;
        params.planar_fog_params = lighting.planar_fog_params;
        params.planar_fog_color = lighting.planar_fog_color;
        params.sh_fill_ar = lighting.sh_fill_ar;
        params.sh_fill_ag = lighting.sh_fill_ag;
        params.sh_fill_ab = lighting.sh_fill_ab;
        params.sh_fill_br = lighting.sh_fill_br;
        params.sh_fill_bg = lighting.sh_fill_bg;
        params.sh_fill_bb = lighting.sh_fill_bb;
        params.sh_fill_c = lighting.sh_fill_c;
        params.shadow_vp_col0 = lighting.shadow_vp_col0;
        params.shadow_vp_col1 = lighting.shadow_vp_col1;
        params.shadow_vp_col2 = lighting.shadow_vp_col2;
        params.shadow_vp_col3 = lighting.shadow_vp_col3;
        params.shadow_params = lighting.shadow_params;
        params.blackmap_params0 = lighting.blackmap_params0;
        params.blackmap_params1 = lighting.blackmap_params1;
        params.blackmap_params2 = lighting.blackmap_params2;
        params.local_light_params = lighting.local_light_params;
        params.blackmap_uv_scales = lighting.blackmap_uv_scales;
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(params));
    }

    /// Overrides distance fading for a deterministic overview capture.
    pub fn set_fade_distances(&mut self, queue: &wgpu::Queue, start: f32, end: f32) {
        self.config.fade_start_distance = start;
        self.config.max_render_distance = end;
        let Some(params) = &mut self.params else {
            return;
        };
        params.foliage_info[2] = start;
        params.foliage_info[3] = end;
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(params));
    }

    /// Create a heightmap texture from terrain position data.
    ///
    /// Preserves the filterable `R10G10B10A2_UNORM` data consumed by the oracle.
    fn create_heightmap_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_data: &crate::types::RawXtdData,
        num_verts: u32,
    ) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Foliage Heightmap"),
            size: wgpu::Extent3d {
                width: num_verts,
                height: num_verts,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgb10a2Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let world_positions = xtd_packed_to_world(&terrain_data.packed_positions, num_verts);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&world_positions),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(num_verts * 4),
                rows_per_image: Some(num_verts),
            },
            wgpu::Extent3d {
                width: num_verts,
                height: num_verts,
                depth_or_array_layers: 1,
            },
        );

        log::info!("Foliage position texture: {num_verts}x{num_verts} R10G10B10A2_UNORM");

        texture
    }
}

#[cfg(test)]
mod tests {
    use super::parse_foliage_index_buffer;

    #[test]
    fn foliage_index_buffer_extracts_one_entry_per_blade() {
        let indices = [
            0x0002_0014_u32,
            0x0002_0015,
            0x0002_0014,
            0x0000_FFFF,
            0x0001_0028,
        ];
        assert_eq!(parse_foliage_index_buffer(&indices, 10), [[2, 2], [4, 1]]);
    }

    #[test]
    fn foliage_index_buffer_rejects_geometry_that_cannot_match_the_oracle() {
        assert!(parse_foliage_index_buffer(&[0x0002_0014], 8).is_empty());
    }
}
