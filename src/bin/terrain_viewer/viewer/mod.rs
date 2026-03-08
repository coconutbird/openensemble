//! TerrainViewer struct and implementation.
//!
//! Split into sub-modules:
//! - `input` — keyboard/mouse handling, debug mode switching, UI
//! - `render` — GPU initialization and render pass logic

mod input;
mod rendering;

use std::path::PathBuf;

use data::Scenario;
use data::assets::AssetSource;
use data::xtd::{TessellationData, XtdFile, XtdReader};
use data::xtt::{ActiveTextureInfo, XttFile, XttReader};
use glam::Vec3;
use render::terrain::{Camera, CompositorResources, LodConfig, TessellationMode};
use render::wgpu;

use crate::types::{
    AlbedoData, AlphaTextureData, AoTextureData, ChunkDecalData, ChunkSplatData, DecalInstance,
    DecalTexture, FoliageQNChunk, FoliageSet, GpuResources, NormalMapTexture, RawXtdData,
    TerrainMesh, TerrainTexture,
};

/// Source for terrain loading.
#[allow(dead_code)]
pub enum TerrainSource {
    /// Load from a local XTD file path.
    File(PathBuf),
    /// Load from a scenario name using Scenario::load_terrain (requires OPENENSEMBLE_GAME_DIR).
    Scenario(String),
}

/// The terrain viewer application.
pub struct TerrainViewer {
    pub xtd_path: Option<PathBuf>,
    /// Scenario name (for ERA loading).
    pub scenario_name: Option<String>,
    /// Asset source for loading textures (set after loading from ERA or scenario).
    pub asset_source: Option<AssetSource>,
    pub terrain: Option<TerrainMesh>,
    pub albedo: Option<AlbedoData>,
    pub terrain_textures: Vec<TerrainTexture>,
    pub normal_textures: Vec<NormalMapTexture>,
    pub chunk_splat_data: Vec<ChunkSplatData>,
    pub camera: Camera,
    pub show_info: bool,
    pub wireframe: bool,
    pub load_error: Option<String>,
    pub gpu: Option<GpuResources>,
    pub surface_format: wgpu::TextureFormat,
    /// Debug mode: 0=normal, 1=alpha values, 2=in-chunk UV, 3=raw atlas, 4=terrain UV
    pub debug_mode: u32,
    /// Tessellation mode: None, CPU, or GPU.
    pub tessellation_mode: TessellationMode,
    /// Tessellation data from XTD file.
    pub tessellation_data: Option<TessellationData>,
    /// Raw XTD data for GPU tessellation.
    pub raw_xtd_data: Option<RawXtdData>,
    /// Normal map strength (gBumpPower in game, scales XY components).
    pub bump_power: f32,
    /// GPU terrain texture compositor (for pre-baked chunk textures).
    pub compositor: Option<CompositorResources>,
    /// Bind group for compositor (separate from main texture bind group).
    pub compositor_bind_group: Option<wgpu::BindGroup>,
    /// Whether to use GPU compositing (vs runtime splatting).
    pub use_gpu_compositing: bool,
    /// LOD configuration for distance-based compositing quality.
    pub lod_config: LodConfig,
    /// Pre-calculated chunk center positions [x, y, z] for LOD calculations.
    pub chunk_centers: Vec<[f32; 3]>,
    /// Decal textures loaded from ERA (_df and _op files).
    pub decal_textures: Vec<DecalTexture>,
    /// Decal instances from XTT (position, rotation, scale).
    pub decal_instances: Vec<DecalInstance>,
    /// Per-chunk decal data (layer IDs and alpha maps).
    pub chunk_decal_data: Vec<ChunkDecalData>,
    /// Foliage sets loaded from ERA (textures + blade geometry).
    pub foliage_sets: Vec<FoliageSet>,
    /// Per quad-node foliage chunk data from XTT.
    pub foliage_qn_chunks: Vec<FoliageQNChunk>,
    /// Foliage GPU resources (pipeline, textures, bind groups).
    pub foliage_resources: Option<crate::foliage::FoliageResources>,
    /// Shadow map resources (pipeline, depth texture, light VP).
    pub shadow_resources: Option<crate::shadow::ShadowResources>,
}

impl TerrainViewer {
    /// Create a terrain viewer from a file path (legacy mode).
    pub fn new(xtd_path: Option<PathBuf>) -> Self {
        Self {
            xtd_path,
            scenario_name: None,
            asset_source: None,
            terrain: None,
            albedo: None,
            terrain_textures: Vec::new(),
            normal_textures: Vec::new(),
            chunk_splat_data: Vec::new(),
            camera: Camera::default(),
            show_info: true,
            wireframe: false,
            load_error: None,
            gpu: None,
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
            debug_mode: 12, // Default to GPU composited rendering
            tessellation_mode: TessellationMode::Gpu, // Default to GPU tessellation (fast)
            tessellation_data: None,
            raw_xtd_data: None,
            bump_power: 1.0, // Default normal map strength (game default)
            compositor: None,
            compositor_bind_group: None,
            use_gpu_compositing: true, // Enabled by default to test GPU compositing
            lod_config: LodConfig::default(),
            chunk_centers: Vec::new(),
            decal_textures: Vec::new(),
            decal_instances: Vec::new(),
            chunk_decal_data: Vec::new(),
            foliage_sets: Vec::new(),
            foliage_qn_chunks: Vec::new(),
            foliage_resources: None,
            shadow_resources: None,
        }
    }

    /// Create a terrain viewer from a scenario name (loads from game directory/ERA).
    pub fn from_scenario(scenario_name: String) -> Self {
        Self {
            xtd_path: None,
            scenario_name: Some(scenario_name),
            asset_source: None,
            terrain: None,
            albedo: None,
            terrain_textures: Vec::new(),
            normal_textures: Vec::new(),
            chunk_splat_data: Vec::new(),
            camera: Camera::default(),
            show_info: true,
            wireframe: false,
            load_error: None,
            gpu: None,
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
            debug_mode: 12, // Default to GPU composited rendering
            tessellation_mode: TessellationMode::Gpu, // Default to GPU tessellation (fast)
            tessellation_data: None,
            raw_xtd_data: None,
            bump_power: 1.0, // Default normal map strength (game default)
            compositor: None,
            compositor_bind_group: None,
            use_gpu_compositing: true, // Enabled by default to test GPU compositing
            lod_config: LodConfig::default(),
            chunk_centers: Vec::new(),
            decal_textures: Vec::new(),
            decal_instances: Vec::new(),
            chunk_decal_data: Vec::new(),
            foliage_sets: Vec::new(),
            foliage_qn_chunks: Vec::new(),
            foliage_resources: None,
            shadow_resources: None,
        }
    }

    fn load_terrain(&mut self) {
        // Determine which loading path to use
        let (xtd, xtt, asset_source_opt) = if let Some(scenario_name) = &self.scenario_name {
            // Load scenario first, then load terrain from it
            log::info!("Loading scenario: {}", scenario_name);
            let scenario = match Scenario::load(scenario_name) {
                Ok(s) => s,
                Err(e) => {
                    self.load_error = Some(format!("Failed to load scenario: {}", e));
                    log::error!("{}", self.load_error.as_ref().unwrap());
                    return;
                }
            };

            log::info!("Loading terrain for scenario: {}", scenario_name);
            match scenario.load_terrain() {
                Ok(terrain) => {
                    // Create asset source for texture loading
                    let asset_source = match AssetSource::for_scenario(scenario_name) {
                        Ok(source) => Some(source),
                        Err(e) => {
                            log::warn!("Failed to create asset source: {}", e);
                            None
                        }
                    };
                    (terrain.xtd, terrain.xtt, asset_source)
                }
                Err(e) => {
                    self.load_error = Some(format!("Failed to load terrain: {}", e));
                    log::error!("{}", self.load_error.as_ref().unwrap());
                    return;
                }
            }
        } else if let Some(path) = &self.xtd_path {
            // Load from file path (legacy mode)
            log::info!("Loading XTD from file: {}", path.display());
            match std::fs::read(path) {
                Ok(data) => match XtdReader::read(&data) {
                    Ok(xtd) => {
                        // Try to load XTT from same location
                        let xtt_path = path.with_extension("xtt");
                        let xtt = if xtt_path.exists() {
                            match std::fs::read(&xtt_path) {
                                Ok(xtt_data) => match XttReader::read(&xtt_data) {
                                    Ok(xtt) => Some(xtt),
                                    Err(e) => {
                                        log::warn!("Failed to parse XTT: {}", e);
                                        None
                                    }
                                },
                                Err(e) => {
                                    log::warn!("Failed to read XTT file: {}", e);
                                    None
                                }
                            }
                        } else {
                            None
                        };
                        (xtd, xtt, None)
                    }
                    Err(e) => {
                        self.load_error = Some(format!("Failed to parse XTD: {}", e));
                        log::error!("{}", self.load_error.as_ref().unwrap());
                        return;
                    }
                },
                Err(e) => {
                    self.load_error = Some(format!("Failed to read file: {}", e));
                    log::error!("{}", self.load_error.as_ref().unwrap());
                    return;
                }
            }
        } else {
            self.load_error = Some("No terrain source specified".to_string());
            return;
        };

        // Store asset source for texture loading
        self.asset_source = asset_source_opt;

        // Process the loaded XTD
        self.process_xtd(&xtd);

        // Process XTT if available
        if let Some(xtt) = xtt {
            self.process_xtt(&xtt);
        }
    }

    /// Process loaded XTD data.
    fn process_xtd(&mut self, xtd: &XtdFile) {
        log::info!(
            "XTD loaded: {}x{} verts, tile_scale={}",
            xtd.header.num_x_verts,
            xtd.header.num_x_verts,
            xtd.header.tile_scale
        );
        log::info!(
            "XTD world_min: [{:.2}, {:.2}, {:.2}]",
            xtd.header.world_min[0],
            xtd.header.world_min[1],
            xtd.header.world_min[2]
        );
        log::info!(
            "XTD world_max: [{:.2}, {:.2}, {:.2}]",
            xtd.header.world_max[0],
            xtd.header.world_max[1],
            xtd.header.world_max[2]
        );
        log::info!(
            "XTD world_size: [{:.2}, {:.2}, {:.2}]",
            xtd.header.world_max[0] - xtd.header.world_min[0],
            xtd.header.world_max[1] - xtd.header.world_min[1],
            xtd.header.world_max[2] - xtd.header.world_min[2]
        );

        // Decode tessellation data
        if let Some(tess) = xtd.decode_tessellation() {
            log::info!(
                "Tessellation: {}x{} patches, max_level={}",
                tess.num_x_patches,
                tess.num_z_patches,
                tess.max_tess_level
            );
            self.tessellation_data = Some(tess);
        } else {
            log::warn!("No tessellation data available in XTD");
            self.tessellation_data = None;
        }

        // Extract raw data for GPU tessellation
        match xtd.extract_raw_data() {
            Ok(raw) => {
                log::info!(
                    "Extracted raw terrain data: {}x{} vertices",
                    raw.num_verts_per_axis,
                    raw.num_verts_per_axis
                );

                // Decode AO data if available
                // Based on IDA RE: AO is half-resolution (512×1024 for 1024×1024 terrain)
                let ao_data = match xtd.decode_ao() {
                    Ok(ao) => {
                        log::info!(
                            "Decoded AO data: {}x{} texture ({} total bytes, half-resolution)",
                            ao.width,
                            ao.height,
                            ao.values.len()
                        );
                        Some(AoTextureData {
                            values: ao.values,
                            width: ao.width as u32,
                            height: ao.height as u32,
                        })
                    }
                    Err(e) => {
                        log::warn!("Failed to decode AO data: {}", e);
                        None
                    }
                };

                // Decode Alpha data if available (same compression as AO)
                // Used for terrain holes/transparency
                let alpha_data = match xtd.decode_alpha() {
                    Ok(alpha) => {
                        log::info!(
                            "Decoded Alpha data: {}x{} texture ({} total bytes, half-resolution)",
                            alpha.width,
                            alpha.height,
                            alpha.values.len()
                        );
                        Some(AlphaTextureData {
                            values: alpha.values,
                            width: alpha.width as u32,
                            height: alpha.height as u32,
                        })
                    }
                    Err(e) => {
                        log::warn!("Failed to decode Alpha data: {}", e);
                        None
                    }
                };

                self.raw_xtd_data = Some(RawXtdData {
                    packed_positions: raw.packed_positions,
                    packed_normals: raw.packed_normals,
                    num_verts_per_axis: raw.num_verts_per_axis,
                    mid: raw.mid,
                    range: raw.range,
                    tile_scale: raw.tile_scale,
                    ao_data,
                    alpha_data,
                });
            }
            Err(e) => {
                log::warn!("Failed to extract raw XTD data for GPU tessellation: {}", e);
                self.raw_xtd_data = None;
            }
        }

        match xtd.decode_vertices() {
            Ok(vertices) => {
                log::info!(
                    "Decoded {} vertices, {} triangles",
                    vertices.positions.len(),
                    vertices.generate_indices().len() / 3
                );

                let mesh = match self.tessellation_mode {
                    TessellationMode::Cpu => {
                        if let Some(ref tess_data) = self.tessellation_data {
                            log::info!("Applying CPU tessellation (this may take a while)...");
                            let start = std::time::Instant::now();
                            let tessellated = vertices.tessellate(tess_data);
                            log::info!(
                                "Tessellation complete: {} vertices, {} triangles ({:.1}s)",
                                tessellated.positions.len(),
                                tessellated.indices.len() / 3,
                                start.elapsed().as_secs_f32()
                            );
                            TerrainMesh::from_tessellated(
                                tessellated,
                                xtd.header.world_min,
                                xtd.header.world_max,
                                xtd.header.tile_scale,
                            )
                        } else {
                            log::warn!(
                                "CPU tessellation enabled but no tessellation data available"
                            );
                            TerrainMesh::from_xtd(
                                &vertices,
                                xtd.header.world_min,
                                xtd.header.world_max,
                                xtd.header.tile_scale,
                            )
                        }
                    }
                    TessellationMode::Gpu => {
                        // For GPU tessellation, we still need a basic mesh for fallback
                        // The actual tessellation happens in the GPU resources creation
                        log::info!("GPU tessellation mode - will use instanced patches");
                        TerrainMesh::from_xtd(
                            &vertices,
                            xtd.header.world_min,
                            xtd.header.world_max,
                            xtd.header.tile_scale,
                        )
                    }
                    TessellationMode::None => TerrainMesh::from_xtd(
                        &vertices,
                        xtd.header.world_min,
                        xtd.header.world_max,
                        xtd.header.tile_scale,
                    ),
                };

                // Position camera at terrain center (only on first load)
                if self.terrain.is_none() {
                    self.camera.position = mesh.center() + Vec3::new(0.0, 200.0, -300.0);
                }
                self.terrain = Some(mesh);
                self.load_error = None;
            }
            Err(e) => {
                self.load_error = Some(format!("Failed to decode vertices: {}", e));
                log::error!("{}", self.load_error.as_ref().unwrap());
            }
        }
    }

    /// Process loaded XTT data.
    fn process_xtt(&mut self, xtt: &XttFile) {
        log::info!(
            "XTT loaded: {} textures, {} linker chunks, {} bytes albedo",
            xtt.header.num_active_textures,
            xtt.linkers.len(),
            xtt.albedo_data.len()
        );

        // Debug: Print active texture filenames
        log::info!("Active textures:");
        for (i, tex) in xtt.active_textures.iter().enumerate() {
            log::info!(
                "  [{}] {} (u_scale={}, v_scale={}, blend_op={})",
                i,
                tex.filename,
                tex.u_scale,
                tex.v_scale,
                tex.blend_op
            );
        }

        // Debug: Print first few linkers' splat info
        if !xtt.linkers.is_empty() {
            // Print first 5 and last linker grid positions
            for (i, linker) in xtt.linkers.iter().enumerate() {
                if i < 5 || i == xtt.linkers.len() - 1 {
                    log::info!("Linker [{}]: grid=({},{})", i, linker.grid_x, linker.grid_z);
                }
            }
            let linker = &xtt.linkers[0];
            log::info!(
                "First linker: grid=({},{}), {} splat layers, {} decal layers",
                linker.grid_x,
                linker.grid_z,
                linker.num_splat_layers,
                linker.num_decal_layers
            );
            log::info!("  splat_layer_ids: {:?}", linker.splat_layer_ids);
            log::info!(
                "  splat_alpha_data: {} bytes",
                linker.splat_alpha_data.len()
            );

            // Test alpha decoding
            match linker.decode_splat_alpha() {
                Ok(alpha_data) => {
                    log::info!(
                        "  Alpha decoded: {} layers, {} alpha maps",
                        alpha_data.num_layers,
                        alpha_data.alpha_maps.len()
                    );
                    // Print alpha map statistics
                    if let Some(first_map) = alpha_data.alpha_maps.first() {
                        let non_zero: usize = first_map.iter().filter(|&&v| v > 0).count();
                        let min_val = first_map.iter().copied().min().unwrap_or(0);
                        let max_val = first_map.iter().copied().max().unwrap_or(0);
                        let sum: u32 = first_map.iter().map(|&v| v as u32).sum();
                        let avg = sum / first_map.len() as u32;
                        log::info!(
                            "  Alpha layer 1: {} non-zero of {}, min={}, max={}, avg={}",
                            non_zero,
                            first_map.len(),
                            min_val,
                            max_val,
                            avg
                        );
                    }
                }
                Err(e) => {
                    log::warn!("  Failed to decode alpha: {}", e);
                }
            }
        }

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

        // Extract chunk splat data for texture splatting
        self.extract_chunk_splat_data(xtt);

        // Extract decal data
        self.extract_decal_data(xtt);

        // Try to load terrain textures from ERA
        self.try_load_terrain_textures(&xtt.active_textures);

        // Try to load decal textures from ERA
        self.try_load_decal_textures(&xtt.active_decals);

        // Extract foliage data from XTT
        self.extract_foliage_data(xtt);

        // Try to load foliage textures and geometry from ERA
        self.try_load_foliage_sets(&xtt.foliage.sets);
    }

    /// Extract chunk splat data from XTT linkers for texture splatting.
    fn extract_chunk_splat_data(&mut self, xtt: &data::xtt::XttFile) {
        // Use the render crate's extraction function
        self.chunk_splat_data = render::terrain::extract_chunk_splat_data(xtt);

        // Debug: Verify chunks are stored in expected order and print any mismatches
        log::info!("Checking chunk storage order vs expected grid positions:");
        for (i, chunk) in self.chunk_splat_data.iter().enumerate() {
            // Expected: chunks stored as x * 16 + z (row-major by X)
            // OR: chunks stored as z * 16 + x (row-major by Z)
            let expected_idx_x_major = chunk.grid_x * 16 + chunk.grid_z;
            let expected_idx_z_major = chunk.grid_z * 16 + chunk.grid_x;

            // Print first 20 chunks to see the pattern
            if i < 20 {
                log::info!(
                    "  Chunk[{}]: grid=({},{}), expected_x_major={}, expected_z_major={}",
                    i,
                    chunk.grid_x,
                    chunk.grid_z,
                    expected_idx_x_major,
                    expected_idx_z_major
                );
            }
        }

        // Debug: Print first row of chunks to verify grid ordering and layer data
        log::info!("First row of chunks (z=0) grid positions and layer IDs:");
        for chunk in &self.chunk_splat_data {
            if chunk.grid_z == 0 {
                // Count actual (non-padding) layers - layer 0 is always valid,
                // layers 1+ are valid only if ID > 0
                let actual_layers: usize = 1 + chunk
                    .layer_texture_ids
                    .iter()
                    .skip(1)
                    .filter(|&&id| id > 0)
                    .count();
                let alpha_count = chunk.alpha_maps.len();

                // Check if first alpha map has any non-zero values
                let alpha1_nonzero = chunk
                    .alpha_maps
                    .first()
                    .map(|m| m.iter().filter(|&&v| v > 0).count())
                    .unwrap_or(0);

                log::info!(
                    "  Chunk x={}: ids={:?}, actual_layers={}, alpha_maps={}, alpha1_nonzero={}",
                    chunk.grid_x,
                    chunk.layer_texture_ids,
                    actual_layers,
                    alpha_count,
                    alpha1_nonzero
                );
            }
        }

        // Debug: Find chunks with DIFFERENT layer configurations
        // Check which chunk boundaries have mismatched layer arrays
        let mut mismatched_boundaries = 0;
        for z in 0..16 {
            for x in 0..16 {
                let idx = z * 16 + x;
                if idx >= self.chunk_splat_data.len() {
                    continue;
                }
                let chunk = &self.chunk_splat_data[idx];

                // Check right neighbor
                if x < 15 {
                    let right_idx = z * 16 + (x + 1);
                    if right_idx < self.chunk_splat_data.len() {
                        let right = &self.chunk_splat_data[right_idx];
                        if chunk.layer_texture_ids != right.layer_texture_ids {
                            mismatched_boundaries += 1;
                            if mismatched_boundaries <= 3 {
                                log::info!(
                                    "Layer mismatch at ({},{}) <-> ({},{}): {:?} vs {:?}",
                                    x,
                                    z,
                                    x + 1,
                                    z,
                                    chunk.layer_texture_ids,
                                    right.layer_texture_ids
                                );
                            }
                        }
                    }
                }
                // Check bottom neighbor
                if z < 15 {
                    let bottom_idx = (z + 1) * 16 + x;
                    if bottom_idx < self.chunk_splat_data.len() {
                        let bottom = &self.chunk_splat_data[bottom_idx];
                        if chunk.layer_texture_ids != bottom.layer_texture_ids {
                            mismatched_boundaries += 1;
                        }
                    }
                }
            }
        }
        log::info!(
            "Total mismatched chunk boundaries: {}",
            mismatched_boundaries
        );
    }

    /// Try to load terrain textures from root.era if available.
    fn try_load_terrain_textures(&mut self, active_textures: &[ActiveTextureInfo]) {
        // Clear existing textures to avoid duplication on reload
        self.terrain_textures.clear();
        self.normal_textures.clear();

        // Log active textures so we can see the index-to-name mapping
        log::info!("Active textures in XTT ({} total):", active_textures.len());
        for (i, tex) in active_textures.iter().enumerate() {
            log::info!("  [{}] {}", i, tex.filename);
        }

        // Use the asset source set during terrain loading
        let source = match &self.asset_source {
            Some(s) => s,
            None => {
                log::info!("No asset source available, skipping terrain texture loading");
                return;
            }
        };

        log::info!("Loading terrain textures from asset source (parallel)");

        // Use the render crate's parallel loading function
        let (textures, normals) = render::terrain::load_terrain_textures(source, active_textures);

        self.terrain_textures = textures;
        self.normal_textures = normals;

        if self.terrain_textures.is_empty() {
            log::info!("No terrain textures loaded");
        } else {
            log::info!("Loaded {} terrain textures", self.terrain_textures.len());
        }
        if self.normal_textures.is_empty() {
            log::info!("No normal maps loaded");
        } else {
            log::info!("Loaded {} normal maps", self.normal_textures.len());
        }
    }

    /// Extract decal data from XTT file.
    fn extract_decal_data(&mut self, xtt: &data::xtt::XttFile) {
        // Log decal statistics
        log::info!(
            "Decal data: {} active decals, {} decal instances",
            xtt.header.num_active_decals,
            xtt.header.num_active_decal_instances
        );

        // Log active decal names
        if !xtt.active_decals.is_empty() {
            log::info!("Active decals:");
            for (i, decal) in xtt.active_decals.iter().enumerate() {
                log::info!("  [{}] {}", i, decal.filename);
            }
        }

        // Use the render crate's extraction function
        let (instances, chunk_data) = render::terrain::extract_decal_data(xtt);
        self.decal_instances = instances;
        self.chunk_decal_data = chunk_data;

        if !self.decal_instances.is_empty() {
            log::info!("Extracted {} decal instances:", self.decal_instances.len());
            for (i, inst) in self.decal_instances.iter().take(5).enumerate() {
                log::info!(
                    "  [{}] decal={}, rot={:.2}, pos=({:.2},{:.2}), scale=({:.2},{:.2})",
                    i,
                    inst.decal_index,
                    inst.rotation,
                    inst.tile_center_x,
                    inst.tile_center_y,
                    inst.u_scale,
                    inst.v_scale
                );
            }
        }

        if !self.chunk_decal_data.is_empty() {
            log::info!(
                "Extracted decal data for {} chunks (out of {} total)",
                self.chunk_decal_data.len(),
                xtt.linkers.len()
            );
        } else {
            log::info!("No chunks have decal layers");
        }
    }

    /// Try to load decal textures from asset source.
    fn try_load_decal_textures(&mut self, active_decals: &[data::xtt::ActiveDecalInfo]) {
        if active_decals.is_empty() {
            log::info!("No active decals to load");
            return;
        }

        self.decal_textures.clear();

        // Use the asset source set during terrain loading
        let source = match &self.asset_source {
            Some(s) => s,
            None => {
                log::info!("No asset source available, skipping decal texture loading");
                return;
            }
        };

        log::info!("Loading decal textures from asset source (parallel)");

        // Use the render crate's parallel loading function
        self.decal_textures = render::terrain::load_decal_textures(source, active_decals);

        log::info!("Loaded {} decal textures", self.decal_textures.len());
    }

    /// Extract foliage data from XTT file.
    fn extract_foliage_data(&mut self, xtt: &data::xtt::XttFile) {
        // Log foliage statistics
        log::info!(
            "Foliage data: {} sets, {} QN chunks",
            xtt.foliage.sets.len(),
            xtt.foliage.qn_chunks.len()
        );

        // Log foliage set names
        for (i, set) in xtt.foliage.sets.iter().enumerate() {
            log::info!("  Foliage set [{}]: {}", i, set.filename);
        }

        // Use the render crate's extraction function
        self.foliage_qn_chunks = render::terrain::extract_foliage_chunks(xtt);

        if !self.foliage_qn_chunks.is_empty() {
            // Log first few QN chunks for debugging
            log::info!(
                "Extracted {} foliage QN chunks:",
                self.foliage_qn_chunks.len()
            );
            for (i, qn) in self.foliage_qn_chunks.iter().take(3).enumerate() {
                log::info!(
                    "  [{}] parent={}, sets={}, polys={:?}",
                    i,
                    qn.qn_parent_index,
                    qn.num_sets,
                    qn.set_poly_counts
                );
            }
        }
    }

    /// Try to load foliage textures and geometry from asset source.
    fn try_load_foliage_sets(&mut self, foliage_sets: &[data::xtt::FoliageSetInfo]) {
        if foliage_sets.is_empty() {
            log::info!("No foliage sets to load");
            return;
        }

        self.foliage_sets.clear();

        // Use the asset source set during terrain loading
        let source = match &self.asset_source {
            Some(s) => s,
            None => {
                log::info!("No asset source available, skipping foliage texture loading");
                return;
            }
        };

        log::info!("Loading foliage textures from asset source (parallel)");

        // Use the render crate's parallel loading function
        self.foliage_sets = render::terrain::load_foliage_sets(source, foliage_sets);

        log::info!(
            "Loaded {} foliage sets ({} with albedo textures)",
            self.foliage_sets.len(),
            self.foliage_sets
                .iter()
                .filter(|s| !s.albedo_pixels.is_empty())
                .count()
        );
    }
}
