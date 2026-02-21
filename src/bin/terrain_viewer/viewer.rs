//! TerrainViewer struct and implementation.

use std::path::PathBuf;

use anyhow::Result;
use data::ddx::DdxTexture;
use data::era::EraArchive;
use data::xtd::{TessellationData, XtdReader};
use data::xtt::{ActiveTextureInfo, XttReader};
use glam::Vec3;
use render::terrain::{
    Camera, CompositingConfig, CompositorResources, GPU_TESS_SHADER, GpuTessParams, LodConfig,
    TERRAIN_SHADER, TerrainParams, TessellationMode, generate_mipmaps, mip_level_count,
};
use render::{Application3D, RenderContext, wgpu};
use xcore::app::{Application, FrameContext, Input, KeyCode};
use xcore::prelude::*;

use crate::camera::CameraInput;
use crate::gpu::create_depth_texture;
use crate::types::{
    AlbedoData, AlphaTextureData, AoTextureData, ChunkSplatData, GpuResources, NormalMapTexture,
    RawXtdData, TerrainMesh, TerrainTexture,
};

/// The terrain viewer application.
pub struct TerrainViewer {
    pub xtd_path: Option<PathBuf>,
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
}

impl TerrainViewer {
    pub fn new(xtd_path: Option<PathBuf>) -> Self {
        Self {
            xtd_path,
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
            debug_mode: 0, // Default to normal rendering (XTT albedo with lighting)
            tessellation_mode: TessellationMode::Gpu, // Default to GPU tessellation (fast)
            tessellation_data: None,
            raw_xtd_data: None,
            bump_power: 1.0, // Default normal map strength (game default)
            compositor: None,
            compositor_bind_group: None,
            use_gpu_compositing: true, // Enabled by default to test GPU compositing
            lod_config: LodConfig::default(),
            chunk_centers: Vec::new(),
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
                            log::warn!(
                                "Failed to extract raw XTD data for GPU tessellation: {}",
                                e
                            );
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
                                        log::info!(
                                            "Applying CPU tessellation (this may take a while)..."
                                        );
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
                                    log::info!(
                                        "GPU tessellation mode - will use instanced patches"
                                    );
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
                                self.camera.position =
                                    mesh.center() + Vec3::new(0.0, 200.0, -300.0);
                            }
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
                                log::info!(
                                    "Linker [{}]: grid=({},{})",
                                    i,
                                    linker.grid_x,
                                    linker.grid_z
                                );
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
                                    let non_zero: usize =
                                        first_map.iter().filter(|&&v| v > 0).count();
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
                    self.extract_chunk_splat_data(&xtt.linkers);

                    // Try to load terrain textures from ERA
                    self.try_load_terrain_textures(xtd_path, &xtt.active_textures);
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

    /// Extract chunk splat data from XTT linkers for texture splatting.
    fn extract_chunk_splat_data(&mut self, linkers: &[data::xtt::XttLinker]) {
        self.chunk_splat_data.clear();

        for linker in linkers {
            // Decode alpha maps for this chunk
            let alpha_maps = match linker.decode_splat_alpha() {
                Ok(alpha_data) => alpha_data.alpha_maps,
                Err(e) => {
                    log::warn!(
                        "Failed to decode splat alpha for chunk ({}, {}): {}",
                        linker.grid_x,
                        linker.grid_z,
                        e
                    );
                    Vec::new()
                }
            };

            self.chunk_splat_data.push(ChunkSplatData {
                grid_x: linker.grid_x,
                grid_z: linker.grid_z,
                layer_texture_ids: linker.splat_layer_ids.clone(),
                alpha_maps,
            });
        }

        log::info!(
            "Extracted splat data for {} chunks",
            self.chunk_splat_data.len()
        );

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
    fn try_load_terrain_textures(
        &mut self,
        xtd_path: &PathBuf,
        active_textures: &[ActiveTextureInfo],
    ) {
        // Clear existing textures to avoid duplication on reload
        self.terrain_textures.clear();

        // Log active textures so we can see the index-to-name mapping
        log::info!("Active textures in XTT ({} total):", active_textures.len());
        for (i, tex) in active_textures.iter().enumerate() {
            log::info!("  [{}] {}", i, tex.filename);
        }

        // Get game directory from environment variable
        let game_dir = match std::env::var("OPENENSEMBLE_GAME_DIR") {
            Ok(dir) => PathBuf::from(dir),
            Err(_) => {
                log::info!("OPENENSEMBLE_GAME_DIR not set, skipping terrain texture loading");
                log::info!(
                    "To load high-res terrain textures, set OPENENSEMBLE_GAME_DIR to your Halo Wars DE install"
                );
                return;
            }
        };

        // Derive ERA path from XTD path - same name but .era extension
        // e.g., blood_gulch.xtd -> blood_gulch.era
        let era_name = xtd_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| format!("{}.era", s));

        let era_path = match era_name {
            Some(name) => game_dir.join(&name),
            None => {
                log::warn!("Could not derive ERA name from XTD path");
                return;
            }
        };

        if !era_path.exists() {
            log::warn!("Scenario ERA not found at: {}", era_path.display());
            return;
        }

        log::info!("Loading terrain textures from: {}", era_path.display());

        let mut archive = match EraArchive::open(&era_path) {
            Ok(a) => a,
            Err(e) => {
                log::warn!("Failed to open {}: {}", era_path.display(), e);
                return;
            }
        };

        // Debug: print ERA entries containing terrain or _nm
        log::debug!("ERA entries with 'terrain' or '_nm':");
        for (i, entry) in archive.iter().enumerate() {
            if let Some(name) = &entry.filename {
                let lower = name.to_lowercase();
                if lower.contains("terrain") || lower.contains("_nm") {
                    log::debug!("  [{}] {}", i, name);
                }
            }
        }

        // Load each texture (diffuse and normal map)
        for tex_info in active_textures {
            // Convert texture name to ERA path
            // XTT stores "sw interior\grass_01", we need "art/terrain/sw interior/grass_01_df.ddx"
            let tex_name = tex_info.filename.replace('\\', "/");

            // Load diffuse texture (_df.ddx)
            let ddx_path = format!("art/terrain/{}_df.ddx", tex_name);
            log::debug!("Looking for diffuse texture: {}", ddx_path);

            let ddx_path_normalized = ddx_path.replace('/', "\\").to_lowercase();
            let file_index = archive.iter().enumerate().find(|(_, e)| {
                e.filename
                    .as_ref()
                    .map(|n| n.replace('/', "\\").to_lowercase() == ddx_path_normalized)
                    .unwrap_or(false)
            });

            // Track if we successfully loaded this texture
            let mut loaded = false;

            if let Some((idx, _)) = file_index {
                if let Ok(data) = archive.read_entry(idx) {
                    if let Ok(ddx) = DdxTexture::from_bytes(&data) {
                        if let Ok(decoded) = ddx.decode_to_rgba() {
                            log::info!(
                                "Loaded terrain texture [{}]: {} ({}x{})",
                                self.terrain_textures.len(),
                                tex_info.filename,
                                decoded.width,
                                decoded.height
                            );
                            self.terrain_textures.push(TerrainTexture {
                                name: tex_info.filename.clone(),
                                width: decoded.width,
                                height: decoded.height,
                                pixels: decoded.pixels,
                                u_scale: tex_info.u_scale,
                                v_scale: tex_info.v_scale,
                            });
                            loaded = true;
                        }
                    }
                }
            }

            // IMPORTANT: Always maintain index alignment with active_textures
            // If loading failed, add a placeholder texture
            if !loaded {
                log::warn!(
                    "Failed to load texture [{}]: {} - using placeholder",
                    self.terrain_textures.len(),
                    tex_info.filename
                );
                // Create a small placeholder texture (magenta for visibility)
                let placeholder_size = 64u32;
                let mut pixels = vec![0u8; (placeholder_size * placeholder_size * 4) as usize];
                for i in 0..(placeholder_size * placeholder_size) as usize {
                    pixels[i * 4] = 255; // R
                    pixels[i * 4 + 1] = 0; // G
                    pixels[i * 4 + 2] = 255; // B (magenta)
                    pixels[i * 4 + 3] = 255; // A
                }
                self.terrain_textures.push(TerrainTexture {
                    name: format!("placeholder_{}", tex_info.filename),
                    width: placeholder_size,
                    height: placeholder_size,
                    pixels,
                    u_scale: 1,
                    v_scale: 1,
                });
            }

            // Load normal map texture (_nm.ddx)
            let nm_path = format!("art/terrain/{}_nm.ddx", tex_name);
            log::debug!("Looking for normal map: {}", nm_path);

            let nm_path_normalized = nm_path.replace('/', "\\").to_lowercase();
            let nm_index = archive.iter().enumerate().find(|(_, e)| {
                e.filename
                    .as_ref()
                    .map(|n| n.replace('/', "\\").to_lowercase() == nm_path_normalized)
                    .unwrap_or(false)
            });

            match nm_index {
                Some((idx, _)) => match archive.read_entry(idx) {
                    Ok(data) => match DdxTexture::from_bytes(&data) {
                        Ok(ddx) => match ddx.decode_to_rgba() {
                            Ok(decoded) => {
                                log::info!(
                                    "Loaded normal map: {} ({}x{})",
                                    tex_info.filename,
                                    decoded.width,
                                    decoded.height
                                );
                                self.normal_textures.push(NormalMapTexture {
                                    name: tex_info.filename.clone(),
                                    width: decoded.width,
                                    height: decoded.height,
                                    pixels: decoded.pixels,
                                });
                            }
                            Err(e) => {
                                log::warn!("Failed to decode normal map {}: {}", nm_path, e);
                            }
                        },
                        Err(e) => {
                            log::warn!("Failed to parse normal map DDX {}: {}", nm_path, e);
                        }
                    },
                    Err(e) => {
                        log::warn!("Failed to read normal map {} from ERA: {}", nm_path, e);
                    }
                },
                None => {
                    log::debug!("Normal map not found in ERA: {}", nm_path);
                }
            }
        }

        if self.terrain_textures.is_empty() {
            log::info!("No terrain textures loaded from ERA");
        } else {
            log::info!("Loaded {} terrain textures", self.terrain_textures.len());
        }
        if self.normal_textures.is_empty() {
            log::info!("No normal maps loaded from ERA");
        } else {
            log::info!("Loaded {} normal maps", self.normal_textures.len());
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

        // Debug mode toggle: 1-5 for specific modes, 0 or ` for normal
        if input.is_key_pressed(KeyCode::Key1) {
            self.debug_mode = 1;
            log::info!("Debug mode: 1 (alpha values)");
        }
        if input.is_key_pressed(KeyCode::Key2) {
            self.debug_mode = 2;
            log::info!("Debug mode: 2 (in-chunk UVs)");
        }
        if input.is_key_pressed(KeyCode::Key3) {
            self.debug_mode = 3;
            log::info!("Debug mode: 3 (raw atlas)");
        }
        if input.is_key_pressed(KeyCode::Key4) {
            self.debug_mode = 4;
            log::info!("Debug mode: 4 (terrain UVs)");
        }
        if input.is_key_pressed(KeyCode::Key5) {
            self.debug_mode = 5;
            log::info!("Debug mode: 5 (pre-composited albedo - correct blending)");
        }
        if input.is_key_pressed(KeyCode::Key6) {
            self.debug_mode = 6;
            log::info!("Debug mode: 6 (layer IDs as colors)");
        }
        if input.is_key_pressed(KeyCode::Key7) {
            self.debug_mode = 7;
            log::info!("Debug mode: 7 (chunk grid positions)");
        }
        if input.is_key_pressed(KeyCode::Key8) {
            self.debug_mode = 8;
            log::info!("Debug mode: 8 (chunk_idx + layer0)");
        }
        if input.is_key_pressed(KeyCode::Key0) {
            self.debug_mode = 0;
            log::info!("Debug mode: 0 (runtime splatting - has boundary issues)");
        }
        if input.is_key_pressed(KeyCode::Key9) {
            self.debug_mode = 9;
            log::info!(
                "Debug mode: 9 (Alpha - terrain holes/transparency, white=solid, black=hole)"
            );
        }
        if input.is_key_pressed(KeyCode::Backspace) {
            self.debug_mode = 10;
            log::info!("Debug mode: 10 (Direct texture array test - left=layer0, right=layer1)");
        }

        // Toggle GPU compositing: C key
        if input.is_key_pressed(KeyCode::C) {
            self.use_gpu_compositing = !self.use_gpu_compositing;
            if self.use_gpu_compositing {
                // Mark all chunks as dirty so they get composited
                if let Some(compositor) = &mut self.compositor {
                    compositor.mark_all_dirty();
                }
            }
            log::info!(
                "GPU compositing: {}",
                if self.use_gpu_compositing {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }

        // Bump power (normal map strength) adjustment: B to decrease, N to increase
        if input.is_key_pressed(KeyCode::B) {
            self.bump_power = (self.bump_power - 0.25).max(0.0);
            log::info!("Bump power: {:.2}", self.bump_power);
        }
        if input.is_key_pressed(KeyCode::N) {
            self.bump_power = (self.bump_power + 0.25).min(4.0);
            log::info!("Bump power: {:.2}", self.bump_power);
        }

        // Toggle tessellation mode (T key) - toggles between GPU and None
        // (CPU mode is skipped because it takes ~30 seconds)
        if input.is_key_pressed(KeyCode::T) {
            // Save camera state before reloading
            let saved_camera_pos = self.camera.position;
            let saved_camera_yaw = self.camera.yaw;
            let saved_camera_pitch = self.camera.pitch;

            // Toggle between GPU and None only (skip slow CPU tessellation)
            self.tessellation_mode = match self.tessellation_mode {
                TessellationMode::Gpu => TessellationMode::None,
                TessellationMode::None => TessellationMode::Gpu,
                TessellationMode::Cpu => TessellationMode::Gpu, // Skip CPU, go to GPU
            };
            log::info!(
                "Tessellation mode: {} (reloading terrain...)",
                self.tessellation_mode.name()
            );
            // Clear GPU resources so they get recreated with new mesh
            self.gpu = None;
            // Reload terrain with new tessellation setting
            self.load_terrain();

            // Restore camera state after reloading
            self.camera.position = saved_camera_pos;
            self.camera.yaw = saved_camera_yaw;
            self.camera.pitch = saved_camera_pitch;

            if let Some(ref terrain) = self.terrain {
                log::info!(
                    "Terrain reloaded: {} vertices, {} triangles",
                    terrain.positions.len(),
                    terrain.indices.len() / 3
                );
            }
        }

        self.camera.update(input, ctx.delta_time);

        // Update LOD levels based on camera position (only when GPU compositing is enabled)
        if self.use_gpu_compositing && !self.chunk_centers.is_empty() {
            if let Some(compositor) = &mut self.compositor {
                let camera_pos = [
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                ];
                let lod_changed =
                    compositor.update_lod(camera_pos, &self.chunk_centers, &self.lod_config);
                if lod_changed {
                    // LOD changed - compositor will mark dirty chunks automatically
                    log::debug!(
                        "LOD updated: {} dirty chunks",
                        compositor.dirty_chunk_count()
                    );
                }
            }
        }

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
                        ui.label(format!("Tessellation: {}", self.tessellation_mode.name()));
                        ui.label(format!(
                            "GPU Compositing: {} (C to toggle)",
                            if self.use_gpu_compositing {
                                "ON"
                            } else {
                                "OFF"
                            }
                        ));
                    }

                    if let Some(err) = &self.load_error {
                        ui.separator();
                        ui.colored_label(egui::Color32::RED, err);
                    }

                    ui.separator();
                    ui.label(format!("Debug Mode: {}", self.debug_mode));
                    ui.horizontal(|ui| {
                        if ui.button("0: Splat").clicked() {
                            self.debug_mode = 0;
                        }
                        if ui.button("1: Alpha").clicked() {
                            self.debug_mode = 1;
                        }
                        if ui.button("2: UV").clicked() {
                            self.debug_mode = 2;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("3: Atlas").clicked() {
                            self.debug_mode = 3;
                        }
                        if ui.button("4: TerrUV").clicked() {
                            self.debug_mode = 4;
                        }
                        if ui.button("5: Comp").clicked() {
                            self.debug_mode = 5;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("6: LayerID").clicked() {
                            self.debug_mode = 6;
                        }
                        if ui.button("7: ChunkPos").clicked() {
                            self.debug_mode = 7;
                        }
                        if ui.button("8: L1Info").clicked() {
                            self.debug_mode = 8;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("9: XTT").clicked() {
                            self.debug_mode = 9;
                        }
                        if ui.button("10: TexTest").clicked() {
                            self.debug_mode = 10;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("11: TerrUV").clicked() {
                            self.debug_mode = 11;
                        }
                        if ui.button("12: GPUComp").clicked() {
                            self.debug_mode = 12;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("13: L0 Only").clicked() {
                            self.debug_mode = 13;
                        }
                        if ui.button("14: L1 ID").clicked() {
                            self.debug_mode = 14;
                        }
                        if ui.button("15: L1 Only").clicked() {
                            self.debug_mode = 15;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("16: Rock").clicked() {
                            self.debug_mode = 16;
                        }
                        if ui.button("17: CPU Blend").clicked() {
                            self.debug_mode = 17;
                        }
                    });

                    ui.separator();
                    ui.label("Controls:");
                    ui.label("  WASD - Move");
                    ui.label("  Space/Ctrl - Up/Down");
                    ui.label("  Arrows - Look");
                    ui.label("  Shift - Fast");
                    ui.label("  Tab - Toggle info");
                    ui.label("  F - Toggle wireframe");
                    ui.label("  T - Toggle tessellation");
                    ui.label("  Escape - Quit");
                });
        }
    }

    fn render(&mut self, _ctx: &FrameContext) -> Color {
        // Sky blue clear color
        Color::new(0.4, 0.6, 0.9, 1.0)
    }
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
            // Check if we should use GPU tessellation
            if self.tessellation_mode == TessellationMode::Gpu {
                if let Some(raw_data) = &self.raw_xtd_data {
                    let raw_data_clone = RawXtdData {
                        packed_positions: raw_data.packed_positions.clone(),
                        packed_normals: raw_data.packed_normals.clone(),
                        num_verts_per_axis: raw_data.num_verts_per_axis,
                        mid: raw_data.mid,
                        range: raw_data.range,
                        tile_scale: raw_data.tile_scale,
                        ao_data: raw_data.ao_data.clone(),
                        alpha_data: raw_data.alpha_data.clone(),
                    };
                    let albedo = self.albedo.take();
                    self.create_gpu_tessellation_resources(
                        ctx.device,
                        ctx.queue,
                        &raw_data_clone,
                        albedo,
                        ctx.size.0,
                        ctx.size.1,
                    );
                } else {
                    // Fallback to regular rendering if no raw data
                    log::warn!(
                        "GPU tessellation requested but no raw XTD data available, falling back to regular rendering"
                    );
                    let terrain = self.terrain.as_ref().unwrap();
                    let positions = terrain.positions.clone();
                    let normals = terrain.normals.clone();
                    let uvs = terrain.uvs.clone();
                    let indices = terrain.indices.clone();
                    let albedo = self.albedo.take();
                    self.create_gpu_resources_from_data(
                        ctx.device, ctx.queue, &positions, &normals, &uvs, &indices, albedo,
                        ctx.size.0, ctx.size.1,
                    );
                }
            } else {
                // Regular rendering (no tessellation or CPU tessellation)
                let terrain = self.terrain.as_ref().unwrap();
                let positions = terrain.positions.clone();
                let normals = terrain.normals.clone();
                let uvs = terrain.uvs.clone();
                let indices = terrain.indices.clone();
                let albedo = self.albedo.take();
                self.create_gpu_resources_from_data(
                    ctx.device, ctx.queue, &positions, &normals, &uvs, &indices, albedo,
                    ctx.size.0, ctx.size.1,
                );
            }
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

        // Update terrain params (for debug mode and bump power changes)
        let params = TerrainParams {
            terrain_size: gpu.terrain_size,
            chunk_count: [16.0, 16.0],
            texture_tile_scale: gpu.tile_scale,
            debug_mode: self.debug_mode as f32,
            bump_power: self.bump_power,
            _padding: 0.0,
        };
        ctx.queue
            .write_buffer(&gpu.params_buffer, 0, bytemuck::bytes_of(&params));

        // Run GPU compositing pass for dirty chunks (if enabled)
        if self.use_gpu_compositing {
            if let (Some(compositor), Some(bind_group)) =
                (&mut self.compositor, &self.compositor_bind_group)
            {
                // Get layer counts per chunk
                let chunk_layer_counts: Vec<u32> = self
                    .chunk_splat_data
                    .iter()
                    .map(|c| c.layer_texture_ids.len() as u32)
                    .collect();

                compositor.composite_all_dirty(
                    ctx.encoder,
                    bind_group,
                    ctx.queue,
                    &chunk_layer_counts,
                );
            }
        }

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

            if gpu.use_gpu_tessellation {
                // GPU tessellation: instanced draw with patch vertices
                // index_buffer contains instance indices (patch indices)
                render_pass.set_vertex_buffer(1, gpu.index_buffer.slice(..));
                // Draw non-indexed triangles, instanced per patch
                render_pass.draw(0..gpu.index_count, 0..gpu.num_patch_instances);
            } else {
                // Regular indexed draw
                render_pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..gpu.index_count, 0, 0..1);
            }
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

        // Create terrain texture array from loaded textures
        let (terrain_array, terrain_array_view) =
            self.create_terrain_texture_array(device, queue, &albedo);

        // Create alpha atlas from chunk splat data
        let (alpha_atlas, alpha_atlas_view) = self.create_alpha_atlas(device, queue);

        // Create pre-composited albedo atlas (correct blending, no boundary issues)
        let (_composited_texture, composited_view) =
            self.create_composited_albedo_atlas(device, queue);

        // Create XTT albedo texture (original pre-composited from game export)
        let (_xtt_albedo_texture, xtt_albedo_view) =
            self.create_xtt_albedo_texture(device, queue, &albedo);

        // Create chunk layers storage buffer
        let chunk_layers_buffer = self.create_chunk_layers_buffer(device);

        // Create terrain params uniform
        let (terrain_size, tile_scale) = if let Some(terrain) = &self.terrain {
            (terrain.size(), terrain.tile_scale)
        } else {
            (Vec3::new(1024.0, 100.0, 1024.0), 1.0)
        };

        let params = TerrainParams {
            terrain_size: [terrain_size.x, terrain_size.z],
            chunk_count: [16.0, 16.0],
            texture_tile_scale: tile_scale,
            debug_mode: self.debug_mode as f32,
            bump_power: self.bump_power,
            _padding: 0.0,
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Params Buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Terrain Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat, // Repeat for tiling
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });

        // Separate sampler for alpha atlas - use Linear filtering like the game does
        // The game uses: MinFilter = LINEAR; MagFilter = LINEAR;
        // This gives smooth blending between textures within each chunk
        let alpha_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Alpha Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest, // No mipmaps on alpha atlas
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    // binding 0: terrain texture array
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 1: sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 2: alpha atlas (filterable for linear sampling like the game)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 3: chunk layers storage buffer
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
                    // binding 4: terrain params uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 5: pre-composited albedo texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 6: alpha atlas sampler (linear filtering like the game)
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 7: XTT albedo (original pre-composited from game export)
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 8: Texture scales buffer (per-texture u_scale/v_scale)
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Create texture scales buffer
        let texture_scales_buffer = self.create_texture_scales_buffer(device);

        // Initialize GPU compositor (for pre-baked terrain textures)
        self.init_compositor(
            device,
            &terrain_array_view,
            &alpha_atlas_view,
            &chunk_layers_buffer,
            &texture_scales_buffer,
            &sampler,
        );

        // Calculate chunk centers for LOD calculations
        self.calculate_chunk_centers();

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&terrain_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&alpha_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: chunk_layers_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&composited_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&alpha_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&xtt_albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: texture_scales_buffer.as_entire_binding(),
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
            params_buffer,
            terrain_size: [terrain_size.x, terrain_size.z],
            tile_scale,
            use_gpu_tessellation: false,
            num_patch_instances: 0,
        });

        log::info!(
            "GPU resources created: {} vertices, {} indices, {} terrain textures",
            positions.len(),
            indices.len(),
            self.terrain_textures.len()
        );
    }

    /// Create GPU resources for GPU tessellation mode.
    /// Uses instanced patch rendering with vertex shader displacement.
    #[allow(clippy::too_many_arguments)]
    fn create_gpu_tessellation_resources(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw_data: &RawXtdData,
        albedo: Option<AlbedoData>,
        width: u32,
        height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let num_verts = raw_data.num_verts_per_axis;
        let num_patches = 64u32; // 64x64 patches like original game
        let verts_per_patch = 16u32; // 16x16 vertices per patch for subdivision

        log::info!(
            "Creating GPU tessellation resources: {}x{} patches, {}x{} verts per patch",
            num_patches,
            num_patches,
            verts_per_patch,
            verts_per_patch
        );

        // Create subdivided patch mesh template
        // Each patch has verts_per_patch x verts_per_patch vertices
        // with local UVs from [0, 1]
        let mut patch_vertices: Vec<[f32; 2]> = Vec::new();
        let mut patch_indices: Vec<u32> = Vec::new();

        for z in 0..verts_per_patch {
            for x in 0..verts_per_patch {
                let u = x as f32 / (verts_per_patch - 1) as f32;
                let v = z as f32 / (verts_per_patch - 1) as f32;
                patch_vertices.push([u, v]);
            }
        }

        // Generate indices for patch triangles
        for z in 0..(verts_per_patch - 1) {
            for x in 0..(verts_per_patch - 1) {
                let top_left = z * verts_per_patch + x;
                let top_right = top_left + 1;
                let bottom_left = (z + 1) * verts_per_patch + x;
                let bottom_right = bottom_left + 1;

                // Two triangles per quad
                patch_indices.push(top_left);
                patch_indices.push(bottom_left);
                patch_indices.push(top_right);

                patch_indices.push(top_right);
                patch_indices.push(bottom_left);
                patch_indices.push(bottom_right);
            }
        }

        // Create instance data (patch indices)
        let total_patches = num_patches * num_patches;
        let instance_data: Vec<u32> = (0..total_patches).collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Patch Vertex Buffer"),
            contents: bytemuck::cast_slice(&patch_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Patch Index Buffer"),
            contents: bytemuck::cast_slice(&patch_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Create position texture (R32Uint format)
        let position_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Position Texture"),
            size: wgpu::Extent3d {
                width: num_verts,
                height: num_verts,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &position_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&raw_data.packed_positions),
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

        let position_view = position_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create normal texture (R32Uint format)
        let normal_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Normal Texture"),
            size: wgpu::Extent3d {
                width: num_verts,
                height: num_verts,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &normal_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&raw_data.packed_normals),
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

        let normal_view = normal_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create AO texture (R8Unorm format, half resolution)
        // Based on IDA RE: AO is stored at 1024×512 for a 1024×1024 terrain
        // (full width, half height)
        // The game samples with bilinear filtering via gVertSampler_ao_Texture
        let (ao_width, ao_height, ao_values) = raw_data.ao_data.as_ref().map_or_else(
            || {
                log::warn!(
                    "No AO data available, using default fully-lit values at half resolution"
                );
                let w = num_verts; // full width
                let h = num_verts / 2; // half height
                (w, h, vec![255u8; (w * h) as usize])
            },
            |ao| {
                log::info!(
                    "Using half-resolution AO texture: {}x{} ({} bytes)",
                    ao.width,
                    ao.height,
                    ao.values.len()
                );
                (ao.width, ao.height, ao.values.clone())
            },
        );

        let ao_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("AO Texture (Half Resolution)"),
            size: wgpu::Extent3d {
                width: ao_width,
                height: ao_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &ao_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &ao_values,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ao_width),
                rows_per_image: Some(ao_height),
            },
            wgpu::Extent3d {
                width: ao_width,
                height: ao_height,
                depth_or_array_layers: 1,
            },
        );

        let ao_view = ao_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create Alpha texture (same format/dimensions as AO - terrain holes/transparency)
        let (alpha_width, alpha_height, alpha_values) = raw_data.alpha_data.as_ref().map_or_else(
            || {
                log::warn!(
                    "No Alpha data available, using default fully-opaque values at half resolution"
                );
                let w = num_verts; // full width
                let h = num_verts / 2; // half height
                (w, h, vec![255u8; (w * h) as usize])
            },
            |alpha| {
                log::info!(
                    "Using half-resolution Alpha texture: {}x{} ({} bytes)",
                    alpha.width,
                    alpha.height,
                    alpha.values.len()
                );
                (alpha.width, alpha.height, alpha.values.clone())
            },
        );

        let alpha_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Alpha Texture (Half Resolution)"),
            size: wgpu::Extent3d {
                width: alpha_width,
                height: alpha_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &alpha_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &alpha_values,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(alpha_width),
                rows_per_image: Some(alpha_height),
            },
            wgpu::Extent3d {
                width: alpha_width,
                height: alpha_height,
                depth_or_array_layers: 1,
            },
        );

        let alpha_view = alpha_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create XTT albedo texture
        let (_xtt_albedo_texture, xtt_albedo_view) =
            self.create_xtt_albedo_texture(device, queue, &albedo);

        // Create normal map texture array
        let (_normal_map_array, normal_map_array_view) =
            self.create_normal_map_array(device, queue);

        // Create terrain texture array (for splatting)
        let (_terrain_array, terrain_array_view) =
            self.create_terrain_texture_array(device, queue, &albedo);

        // Create alpha atlas from chunk splat data (for texture splatting)
        let (_alpha_atlas, alpha_atlas_view) = self.create_alpha_atlas(device, queue);

        // Create chunk layers storage buffer (for texture splatting)
        let chunk_layers_buffer = self.create_chunk_layers_buffer(device);

        // Create CPU-composited albedo atlas (for comparison/debugging)
        let (_composited_texture, composited_view) =
            self.create_composited_albedo_atlas(device, queue);

        // Create alpha sampler (linear filtering like the game for smooth blending)
        let alpha_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Alpha Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create tessellation params uniform buffer
        let tess_params = GpuTessParams {
            mid: [raw_data.mid[0], raw_data.mid[1], raw_data.mid[2], 0.0],
            range: [raw_data.range[0], raw_data.range[1], raw_data.range[2], 0.0],
            terrain_info: [
                num_verts as f32,
                raw_data.tile_scale,
                num_patches as f32,
                num_patches as f32,
            ],
            world_min: [0.0, 0.0, 0.0, 0.0], // Not used directly, computed from tile_scale
            world_max: [0.0, 0.0, 0.0, 0.0],
        };

        let tess_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Params Buffer"),
            contents: bytemuck::bytes_of(&tess_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create terrain params buffer for debug mode
        let terrain_size = if let Some(terrain) = &self.terrain {
            terrain.size()
        } else {
            Vec3::new(1024.0, 100.0, 1024.0)
        };

        let params = TerrainParams {
            terrain_size: [terrain_size.x, terrain_size.z],
            chunk_count: [16.0, 16.0],
            texture_tile_scale: 32.0,
            debug_mode: self.debug_mode as f32,
            bump_power: self.bump_power,
            _padding: 0.0,
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Params Buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create sampler - MUST use Repeat for texture tiling (UVs go 0-16 for 16 chunks)
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Terrain Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });

        // Camera uniform buffer
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

        // Texture bind group layout for GPU tessellation
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GPU Tess Texture Bind Group Layout"),
                entries: &[
                    // binding 0: tess params uniform (needed by both VS and FS for normal sampling)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 1: position texture (R32Uint)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Uint,
                        },
                        count: None,
                    },
                    // binding 2: normal texture (R32Uint) - needed by both VS and FS for normal sampling
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Uint,
                        },
                        count: None,
                    },
                    // binding 3: XTT albedo texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 4: sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 5: terrain params
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 6: AO texture (R8Unorm)
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 7: Alpha texture (R8Unorm) - terrain holes/transparency
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 8: Normal map texture array
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 9: Terrain texture array (for splatting with normal maps)
                    wgpu::BindGroupLayoutEntry {
                        binding: 9,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 10: Alpha atlas texture (for texture splatting blend weights)
                    wgpu::BindGroupLayoutEntry {
                        binding: 10,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 11: Chunk layers storage buffer (per-chunk texture IDs)
                    wgpu::BindGroupLayoutEntry {
                        binding: 11,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 12: Alpha sampler (linear filtering like the game)
                    wgpu::BindGroupLayoutEntry {
                        binding: 12,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 13: CPU-composited albedo atlas
                    wgpu::BindGroupLayoutEntry {
                        binding: 13,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 14: Texture scales buffer (per-texture u_scale/v_scale)
                    wgpu::BindGroupLayoutEntry {
                        binding: 14,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 15: GPU-composited albedo atlas (from compositor)
                    wgpu::BindGroupLayoutEntry {
                        binding: 15,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        // Create texture scales buffer
        let texture_scales_buffer = self.create_texture_scales_buffer(device);

        // Initialize GPU compositor (for pre-baked terrain textures)
        self.init_compositor(
            device,
            &terrain_array_view,
            &alpha_atlas_view,
            &chunk_layers_buffer,
            &texture_scales_buffer,
            &sampler,
        );

        // Calculate chunk centers for LOD calculations
        self.calculate_chunk_centers();

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU Tess Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: tess_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&position_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&xtt_albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&alpha_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&normal_map_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&terrain_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&alpha_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: chunk_layers_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::Sampler(&alpha_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: wgpu::BindingResource::TextureView(&composited_view),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: texture_scales_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: wgpu::BindingResource::TextureView(
                        self.compositor.as_ref().unwrap().albedo_atlas_view(),
                    ),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Tessellation Shader"),
            source: wgpu::ShaderSource::Wgsl(GPU_TESS_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU Tess Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Two vertex buffers: patch vertices (per-vertex) and instance data (per-instance)
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GPU Tessellation Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // Per-vertex: local UV
                    wgpu::VertexBufferLayout {
                        array_stride: 8, // 2 floats
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0, // local_uv
                        }],
                    },
                    // Per-instance: patch index
                    wgpu::VertexBufferLayout {
                        array_stride: 4, // 1 u32
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 0,
                            shader_location: 1, // patch_index
                        }],
                    },
                ],
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

        // Create non-indexed patch vertices (expanded triangles)
        // This is less efficient but simpler - each triangle has its own vertices
        let mut expanded_vertices: Vec<[f32; 2]> = Vec::new();
        for idx in &patch_indices {
            expanded_vertices.push(patch_vertices[*idx as usize]);
        }

        let expanded_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tess Expanded Vertex Buffer"),
            contents: bytemuck::cast_slice(&expanded_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Now we have:
        // - expanded_vertex_buffer: patch triangle vertices (non-indexed)
        // - instance_buffer: patch indices for instancing

        self.gpu = Some(GpuResources {
            pipeline,
            vertex_buffer: expanded_vertex_buffer,
            index_buffer: instance_buffer,
            index_count: expanded_vertices.len() as u32, // vertex count for draw()
            camera_buffer,
            camera_bind_group,
            texture_bind_group,
            depth_texture,
            depth_view,
            params_buffer,
            terrain_size: [terrain_size.x, terrain_size.z],
            tile_scale: raw_data.tile_scale,
            use_gpu_tessellation: true,
            num_patch_instances: total_patches,
        });

        log::info!(
            "GPU tessellation resources created: {} patches, {} vertices per patch, {} total triangles",
            total_patches,
            expanded_vertices.len(),
            (expanded_vertices.len() / 3) * total_patches as usize
        );

        // Suppress unused variable warnings
        let _ = _xtt_albedo_texture;
        let _ = index_buffer;
        let _ = vertex_buffer;
    }

    /// Creates a 2D texture array from loaded terrain textures.
    fn create_terrain_texture_array(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        albedo: &Option<AlbedoData>,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        // Use terrain textures if available, otherwise fall back to albedo or white
        if !self.terrain_textures.is_empty() {
            // All textures should be same size (e.g., 1024x1024)
            let tex_width = self.terrain_textures[0].width;
            let tex_height = self.terrain_textures[0].height;
            let layer_count = self.terrain_textures.len() as u32;
            let num_mips = mip_level_count(tex_width, tex_height);

            log::info!(
                "Creating terrain texture array: {}x{} x {} layers with {} mip levels",
                tex_width,
                tex_height,
                layer_count,
                num_mips
            );

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Terrain Texture Array"),
                size: wgpu::Extent3d {
                    width: tex_width,
                    height: tex_height,
                    depth_or_array_layers: layer_count,
                },
                mip_level_count: num_mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            // Upload each layer with mipmaps
            log::info!("=== Uploading textures to GPU array ===");
            for (i, tex) in self.terrain_textures.iter().enumerate() {
                log::info!("  GPU layer [{}] = {}", i, tex.name);
                // Generate mipmaps for this texture
                let mips = generate_mipmaps(&tex.pixels, tex_width, tex_height);

                // Upload each mip level
                let mut mip_width = tex_width;
                let mut mip_height = tex_height;
                for (mip_level, mip_data) in mips.iter().enumerate() {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: mip_level as u32,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: i as u32,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        mip_data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * mip_width),
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
            }

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        } else {
            // Fallback: create single-layer array from albedo or white
            let (tex_width, tex_height, tex_data) = if let Some(a) = albedo {
                log::info!("Using albedo atlas as fallback: {}x{}", a.width, a.height);
                (a.width, a.height, a.pixels.clone())
            } else {
                log::info!("Using white fallback texture");
                (1, 1, vec![255u8, 255, 255, 255])
            };

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Terrain Texture Array (Fallback)"),
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

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        }
    }

    /// Creates a 2D texture array from loaded normal map textures.
    fn create_normal_map_array(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        if !self.normal_textures.is_empty() {
            // All normal maps should be same size as terrain textures
            let tex_width = self.normal_textures[0].width;
            let tex_height = self.normal_textures[0].height;
            let layer_count = self.normal_textures.len() as u32;
            let num_mips = mip_level_count(tex_width, tex_height);

            log::info!(
                "Creating normal map array: {}x{} x {} layers with {} mip levels",
                tex_width,
                tex_height,
                layer_count,
                num_mips
            );

            // Normal maps should NOT be sRGB - they contain linear data
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Normal Map Array"),
                size: wgpu::Extent3d {
                    width: tex_width,
                    height: tex_height,
                    depth_or_array_layers: layer_count,
                },
                mip_level_count: num_mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm, // NOT sRGB for normal maps
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            // Upload each layer with mipmaps
            for (i, tex) in self.normal_textures.iter().enumerate() {
                // Generate mipmaps for this texture
                let mips = generate_mipmaps(&tex.pixels, tex_width, tex_height);

                // Upload each mip level
                let mut mip_width = tex_width;
                let mut mip_height = tex_height;
                for (mip_level, mip_data) in mips.iter().enumerate() {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: mip_level as u32,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: i as u32,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        mip_data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * mip_width),
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
            }

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        } else {
            // Fallback: flat normal (pointing up)
            log::info!("Using flat normal fallback texture");
            // Normal map flat = (0.5, 0.5, 1.0) in tangent space = (128, 128, 255) in 0-255
            let flat_normal = vec![128u8, 128, 255, 255];

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Normal Map Array (Fallback)"),
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
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &flat_normal,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        }
    }

    /// Creates the alpha atlas texture (1024x1024 RGBA, each chunk is 64x64).
    fn create_alpha_atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        // Alpha atlas: 16x16 chunks, 64x64 per chunk = 1024x1024
        // Each pixel has RGBA for up to 4 alpha channels (layers 1-4)
        const ATLAS_SIZE: u32 = 1024;
        const CHUNK_SIZE: u32 = 64;

        let mut atlas_data = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];

        if !self.chunk_splat_data.is_empty() {
            log::info!(
                "Creating alpha atlas from {} chunks",
                self.chunk_splat_data.len()
            );

            // Debug: log first few chunk positions and alpha stats
            for (i, chunk) in self.chunk_splat_data.iter().take(5).enumerate() {
                let non_zero: usize = chunk
                    .alpha_maps
                    .iter()
                    .flat_map(|m| m.iter())
                    .filter(|&&v| v > 0)
                    .count();
                log::info!(
                    "  Chunk {}: grid=({},{}), {} alpha maps, {} non-zero values",
                    i,
                    chunk.grid_x,
                    chunk.grid_z,
                    chunk.alpha_maps.len(),
                    non_zero
                );
            }

            for chunk in &self.chunk_splat_data {
                // Place chunk at atlas position matching its grid coordinates
                // Shader calculates chunk_x/chunk_y from UV and uses that to index
                let chunk_x = chunk.grid_x as u32;
                let chunk_y = chunk.grid_z as u32;

                // Copy alpha maps to atlas (build normally first)
                for y in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let atlas_x = chunk_x * CHUNK_SIZE + x;
                        let atlas_y = chunk_y * CHUNK_SIZE + y;
                        let atlas_idx = ((atlas_y * ATLAS_SIZE + atlas_x) * 4) as usize;
                        let chunk_idx = (y * CHUNK_SIZE + x) as usize;

                        // R = alpha for layer 1, G = layer 2, B = layer 3, A = layer 4
                        if chunk.alpha_maps.len() > 0 && chunk_idx < chunk.alpha_maps[0].len() {
                            atlas_data[atlas_idx] = chunk.alpha_maps[0][chunk_idx];
                        }
                        if chunk.alpha_maps.len() > 1 && chunk_idx < chunk.alpha_maps[1].len() {
                            atlas_data[atlas_idx + 1] = chunk.alpha_maps[1][chunk_idx];
                        }
                        if chunk.alpha_maps.len() > 2 && chunk_idx < chunk.alpha_maps[2].len() {
                            atlas_data[atlas_idx + 2] = chunk.alpha_maps[2][chunk_idx];
                        }
                        // A channel for layer 4 if we have it (rare)
                        if chunk.alpha_maps.len() > 3 && chunk_idx < chunk.alpha_maps[3].len() {
                            atlas_data[atlas_idx + 3] = chunk.alpha_maps[3][chunk_idx];
                        } else {
                            atlas_data[atlas_idx + 3] = 255; // Unused alpha = opaque
                        }
                    }
                }
            }

            // The alpha data in the file is stored inverted, so we need to mirror horizontally
            // then rotate 90° CCW to match the terrain's coordinate system

            // Step 1: Mirror horizontally (flip X)
            let mut mirrored_atlas = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
            for y in 0..ATLAS_SIZE {
                for x in 0..ATLAS_SIZE {
                    let src_idx = ((y * ATLAS_SIZE + x) * 4) as usize;
                    let dst_x = (ATLAS_SIZE - 1) - x;
                    let dst_idx = ((y * ATLAS_SIZE + dst_x) * 4) as usize;
                    mirrored_atlas[dst_idx..dst_idx + 4]
                        .copy_from_slice(&atlas_data[src_idx..src_idx + 4]);
                }
            }

            // Step 2: Rotate 90 degrees counter-clockwise
            // Original (x, y) -> New (y, SIZE - 1 - x)
            let mut rotated_atlas = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
            for y in 0..ATLAS_SIZE {
                for x in 0..ATLAS_SIZE {
                    let src_idx = ((y * ATLAS_SIZE + x) * 4) as usize;
                    let dst_x = y;
                    let dst_y = (ATLAS_SIZE - 1) - x;
                    let dst_idx = ((dst_y * ATLAS_SIZE + dst_x) * 4) as usize;
                    rotated_atlas[dst_idx..dst_idx + 4]
                        .copy_from_slice(&mirrored_atlas[src_idx..src_idx + 4]);
                }
            }
            atlas_data = rotated_atlas;

            // Debug: Save alpha atlas to disk for inspection
            if let Err(e) =
                Self::save_debug_atlas(&atlas_data, ATLAS_SIZE, "/tmp/alpha_atlas_debug.png")
            {
                log::warn!("Failed to save debug atlas: {}", e);
            } else {
                log::info!("Saved alpha atlas to /tmp/alpha_atlas_debug.png for inspection");
            }
        } else {
            log::info!("No splat data, using empty alpha atlas");
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Alpha Atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, // Linear, not sRGB
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
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * ATLAS_SIZE),
                rows_per_image: Some(ATLAS_SIZE),
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Saves the alpha atlas as a PNG for debugging.
    fn save_debug_atlas(atlas_data: &[u8], size: u32, path: &str) -> Result<()> {
        use std::fs::File;
        use std::io::BufWriter;

        let file = File::create(path)?;
        let w = BufWriter::new(file);

        let mut encoder = png::Encoder::new(w, size, size);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header()?;
        writer.write_image_data(atlas_data)?;

        Ok(())
    }

    /// Creates a pre-composited albedo atlas by blending all texture layers on the CPU.
    /// This avoids the per-chunk layer index mismatch problem at chunk boundaries.
    fn create_composited_albedo_atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        // Composited atlas size: 2048x2048 for higher quality (128x128 per chunk)
        // Each chunk gets more pixels for better texture detail
        const ATLAS_SIZE: u32 = 2048;
        const CHUNK_SIZE: u32 = 128; // 2048 / 16 = 128 pixels per chunk
        const ALPHA_CHUNK_SIZE: u32 = 64;

        let mut atlas_data = vec![128u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize]; // Gray default

        if !self.chunk_splat_data.is_empty() && !self.terrain_textures.is_empty() {
            log::info!(
                "Creating composited albedo atlas: {}x{} ({} chunks, {} textures)",
                ATLAS_SIZE,
                ATLAS_SIZE,
                self.chunk_splat_data.len(),
                self.terrain_textures.len()
            );

            // Debug: log texture names in order
            for (i, tex) in self.terrain_textures.iter().enumerate() {
                log::info!("  CPU blend texture[{}] = {}", i, tex.name);
            }

            // Debug: log first few chunks' layer IDs
            for (i, chunk) in self.chunk_splat_data.iter().take(5).enumerate() {
                log::info!(
                    "  CPU blend chunk[{}] grid=({},{}) layers={:?}",
                    i,
                    chunk.grid_x,
                    chunk.grid_z,
                    chunk.layer_texture_ids
                );
            }

            let tex_width = self.terrain_textures[0].width;
            let tex_height = self.terrain_textures[0].height;

            for chunk in &self.chunk_splat_data {
                // Place chunk at position matching its grid coordinates
                let chunk_x = chunk.grid_x as u32;
                let chunk_z = chunk.grid_z as u32;

                // For each pixel in this chunk's output region
                for py in 0..CHUNK_SIZE {
                    for px in 0..CHUNK_SIZE {
                        // Output position in atlas
                        let atlas_x = chunk_x * CHUNK_SIZE + px;
                        let atlas_z = chunk_z * CHUNK_SIZE + py;
                        let atlas_idx = ((atlas_z * ATLAS_SIZE + atlas_x) * 4) as usize;

                        // Calculate UV within the chunk (0-1)
                        let in_chunk_u = px as f32 / CHUNK_SIZE as f32;
                        let in_chunk_v = py as f32 / CHUNK_SIZE as f32;

                        // Sample alpha from the chunk's alpha maps
                        // Alpha maps are 64x64 per chunk
                        let alpha_x = ((in_chunk_u * ALPHA_CHUNK_SIZE as f32) as u32)
                            .min(ALPHA_CHUNK_SIZE - 1);
                        let alpha_y = ((in_chunk_v * ALPHA_CHUNK_SIZE as f32) as u32)
                            .min(ALPHA_CHUNK_SIZE - 1);
                        let alpha_idx = (alpha_y * ALPHA_CHUNK_SIZE + alpha_x) as usize;

                        // Get alpha values for each layer
                        let alpha0: f32 = 1.0; // Base layer always 100%
                        let alpha1 = if chunk.alpha_maps.len() > 0
                            && alpha_idx < chunk.alpha_maps[0].len()
                        {
                            chunk.alpha_maps[0][alpha_idx] as f32 / 255.0
                        } else {
                            0.0
                        };
                        let alpha2 = if chunk.alpha_maps.len() > 1
                            && alpha_idx < chunk.alpha_maps[1].len()
                        {
                            chunk.alpha_maps[1][alpha_idx] as f32 / 255.0
                        } else {
                            0.0
                        };
                        let alpha3 = if chunk.alpha_maps.len() > 2
                            && alpha_idx < chunk.alpha_maps[2].len()
                        {
                            chunk.alpha_maps[2][alpha_idx] as f32 / 255.0
                        } else {
                            0.0
                        };

                        // Calculate tiled UV for texture sampling (16x tiling for terrain textures)
                        let global_u = (chunk.grid_x as f32 + in_chunk_u) / 16.0;
                        let global_v = (chunk.grid_z as f32 + in_chunk_v) / 16.0;
                        let tiled_u = (global_u * 16.0).fract();
                        let tiled_v = (global_v * 16.0).fract();

                        // Sample position in source textures
                        let tex_x = ((tiled_u * tex_width as f32) as u32).min(tex_width - 1);
                        let tex_y = ((tiled_v * tex_height as f32) as u32).min(tex_height - 1);
                        let tex_idx = ((tex_y * tex_width + tex_x) * 4) as usize;

                        // Get layer texture IDs for this chunk
                        // Layer IDs are direct indices into active_textures/terrain_textures
                        let layer0_id = chunk.layer_texture_ids.get(0).copied().unwrap_or(0);
                        let layer1_id = chunk.layer_texture_ids.get(1).copied().unwrap_or(0);
                        let layer2_id = chunk.layer_texture_ids.get(2).copied().unwrap_or(0);
                        let layer3_id = chunk.layer_texture_ids.get(3).copied().unwrap_or(0);

                        // Sample base layer
                        let mut r: f32;
                        let mut g: f32;
                        let mut b: f32;

                        let layer0_idx = layer0_id as usize;
                        if layer0_idx < self.terrain_textures.len()
                            && tex_idx + 3 < self.terrain_textures[layer0_idx].pixels.len()
                        {
                            r = self.terrain_textures[layer0_idx].pixels[tex_idx] as f32;
                            g = self.terrain_textures[layer0_idx].pixels[tex_idx + 1] as f32;
                            b = self.terrain_textures[layer0_idx].pixels[tex_idx + 2] as f32;
                        } else {
                            r = 128.0;
                            g = 128.0;
                            b = 128.0;
                        }

                        // Blend layer 1 - ONLY if layer ID is non-zero (ID=0 means padding)
                        let layer1_idx = layer1_id as usize;
                        if layer1_id > 0
                            && alpha1 > 0.0
                            && layer1_idx < self.terrain_textures.len()
                            && tex_idx + 3 < self.terrain_textures[layer1_idx].pixels.len()
                        {
                            let lr = self.terrain_textures[layer1_idx].pixels[tex_idx] as f32;
                            let lg = self.terrain_textures[layer1_idx].pixels[tex_idx + 1] as f32;
                            let lb = self.terrain_textures[layer1_idx].pixels[tex_idx + 2] as f32;
                            r = r * (1.0 - alpha1) + lr * alpha1;
                            g = g * (1.0 - alpha1) + lg * alpha1;
                            b = b * (1.0 - alpha1) + lb * alpha1;
                        }

                        // Blend layer 2 - ONLY if layer ID is non-zero
                        let layer2_idx = layer2_id as usize;
                        if layer2_id > 0
                            && alpha2 > 0.0
                            && layer2_idx < self.terrain_textures.len()
                            && tex_idx + 3 < self.terrain_textures[layer2_idx].pixels.len()
                        {
                            let lr = self.terrain_textures[layer2_idx].pixels[tex_idx] as f32;
                            let lg = self.terrain_textures[layer2_idx].pixels[tex_idx + 1] as f32;
                            let lb = self.terrain_textures[layer2_idx].pixels[tex_idx + 2] as f32;
                            r = r * (1.0 - alpha2) + lr * alpha2;
                            g = g * (1.0 - alpha2) + lg * alpha2;
                            b = b * (1.0 - alpha2) + lb * alpha2;
                        }

                        // Blend layer 3 - ONLY if layer ID is non-zero
                        let layer3_idx = layer3_id as usize;
                        if layer3_id > 0
                            && alpha3 > 0.0
                            && layer3_idx < self.terrain_textures.len()
                            && tex_idx + 3 < self.terrain_textures[layer3_idx].pixels.len()
                        {
                            let lr = self.terrain_textures[layer3_idx].pixels[tex_idx] as f32;
                            let lg = self.terrain_textures[layer3_idx].pixels[tex_idx + 1] as f32;
                            let lb = self.terrain_textures[layer3_idx].pixels[tex_idx + 2] as f32;
                            r = r * (1.0 - alpha3) + lr * alpha3;
                            g = g * (1.0 - alpha3) + lg * alpha3;
                            b = b * (1.0 - alpha3) + lb * alpha3;
                        }

                        atlas_data[atlas_idx] = r.clamp(0.0, 255.0) as u8;
                        atlas_data[atlas_idx + 1] = g.clamp(0.0, 255.0) as u8;
                        atlas_data[atlas_idx + 2] = b.clamp(0.0, 255.0) as u8;
                        atlas_data[atlas_idx + 3] = 255;
                    }
                }
            }

            // Save debug image
            if let Err(e) =
                Self::save_debug_atlas(&atlas_data, ATLAS_SIZE, "/tmp/composited_albedo.png")
            {
                log::warn!("Failed to save composited albedo debug: {}", e);
            } else {
                log::info!("Saved composited albedo to /tmp/composited_albedo.png");
            }
        } else {
            log::info!("No splat data or textures, using gray composited atlas");
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Composited Albedo Atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
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
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * ATLAS_SIZE),
                rows_per_image: Some(ATLAS_SIZE),
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Creates the XTT albedo texture (original pre-composited from game export).
    /// This is the unique texture that the game's export tools created with proper blending.
    fn create_xtt_albedo_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        albedo: &Option<AlbedoData>,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let (tex_width, tex_height, tex_data) = if let Some(a) = albedo {
            log::info!("Creating XTT albedo texture: {}x{}", a.width, a.height);
            (a.width, a.height, a.pixels.clone())
        } else {
            log::info!("No XTT albedo, using gray fallback");
            (1, 1, vec![128u8, 128, 128, 255])
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("XTT Albedo Texture"),
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

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Creates the chunk layers storage buffer (256 chunks * 8 layer IDs each).
    fn create_chunk_layers_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;

        // 256 chunks * 8 layers = 2048 u32s
        let mut layer_data = vec![0u32; 256 * 8];

        for chunk in &self.chunk_splat_data {
            // Z-major order: index = gridZ * 16 + gridX (matches shader's chunk_x = idx % 16, chunk_z = idx / 16)
            let chunk_idx = (chunk.grid_z * 16 + chunk.grid_x) as usize;
            let base = chunk_idx * 8;

            for (i, &layer_id) in chunk.layer_texture_ids.iter().enumerate().take(8) {
                // Use layer IDs directly - they appear to be 0-based indices into active_textures
                layer_data[base + i] = layer_id as u32;
            }
        }

        log::info!(
            "Creating chunk layers buffer: {} chunks",
            self.chunk_splat_data.len()
        );
        // Log first few chunks for debugging
        log::info!("=== First 5 chunk layer IDs in buffer ===");
        for chunk in self.chunk_splat_data.iter().take(5) {
            let chunk_idx = (chunk.grid_z * 16 + chunk.grid_x) as usize;
            log::info!(
                "  Chunk ({}, {}) idx={}: layers={:?}",
                chunk.grid_x,
                chunk.grid_z,
                chunk_idx,
                &chunk.layer_texture_ids
            );
        }

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Chunk Layers Buffer"),
            contents: bytemuck::cast_slice(&layer_data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }

    /// Creates the texture scales storage buffer (per-texture u_scale/v_scale values).
    /// The game uses these to control how many times each texture tiles across the terrain.
    fn create_texture_scales_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;

        // Each texture has a vec2<f32> with (u_scale, v_scale)
        // Max 8 textures to match the texture array
        let mut scale_data = vec![1.0f32; 8 * 2]; // Default scale of 1.0

        for (i, tex) in self.terrain_textures.iter().enumerate().take(8) {
            // Game stores scale as i32, but shader needs f32
            // Scale values are typically 1, 2, 4, etc.
            scale_data[i * 2] = tex.u_scale as f32;
            scale_data[i * 2 + 1] = tex.v_scale as f32;
            log::info!(
                "Texture[{}] {} scale: ({}, {})",
                i,
                tex.name,
                tex.u_scale,
                tex.v_scale
            );
        }

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Texture Scales Buffer"),
            contents: bytemuck::cast_slice(&scale_data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }

    /// Initialize the GPU compositor for pre-baking terrain textures.
    /// This creates an 8K×8K atlas (16×16 chunks, 512×512 each) where terrain
    /// layers are composited once and then sampled efficiently during rendering.
    fn init_compositor(
        &mut self,
        device: &wgpu::Device,
        terrain_array_view: &wgpu::TextureView,
        alpha_atlas_view: &wgpu::TextureView,
        chunk_layers_buffer: &wgpu::Buffer,
        texture_scales_buffer: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
    ) {
        let config = CompositingConfig::default();
        log::info!(
            "Initializing GPU compositor: {}×{} atlas ({} chunks)",
            config.atlas_width,
            config.atlas_height,
            config.total_chunks()
        );

        // Create compositor resources (atlas textures, pipeline, bind group layout)
        let compositor = CompositorResources::new(device, config);

        // Create bind group with actual terrain textures
        let bind_group = compositor.create_bind_group(
            device,
            terrain_array_view,
            alpha_atlas_view,
            chunk_layers_buffer,
            texture_scales_buffer,
            sampler,
        );

        self.compositor = Some(compositor);
        self.compositor_bind_group = Some(bind_group);

        log::info!("GPU compositor initialized successfully");
    }

    /// Calculate chunk center positions based on terrain bounds.
    /// Chunks are arranged in a 16×16 grid covering the terrain.
    fn calculate_chunk_centers(&mut self) {
        let Some(terrain) = &self.terrain else {
            return;
        };

        let world_min = terrain.world_min;
        let world_max = terrain.world_max;
        let chunks_x = 16u32;
        let chunks_z = 16u32;

        let chunk_width = (world_max[0] - world_min[0]) / chunks_x as f32;
        let chunk_depth = (world_max[2] - world_min[2]) / chunks_z as f32;
        let chunk_height = (world_max[1] - world_min[1]) / 2.0; // Average Y for center

        self.chunk_centers.clear();
        for cz in 0..chunks_z {
            for cx in 0..chunks_x {
                let center_x = world_min[0] + (cx as f32 + 0.5) * chunk_width;
                let center_y = world_min[1] + chunk_height; // Approximate center Y
                let center_z = world_min[2] + (cz as f32 + 0.5) * chunk_depth;
                self.chunk_centers.push([center_x, center_y, center_z]);
            }
        }

        log::info!(
            "Calculated {} chunk centers for LOD (chunk size: {:.1} x {:.1})",
            self.chunk_centers.len(),
            chunk_width,
            chunk_depth
        );
    }
}
