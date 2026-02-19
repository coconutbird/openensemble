//! Terrain Viewer for Halo Wars XTD files.
//!
//! Loads and renders terrain from XTD files using wgpu.
//! Supports XTT texturing for albedo atlas rendering.
//! WASD + mouse to fly around the terrain.

use anyhow::Result;
use core::app::{Application, FrameContext, Input, KeyCode, WindowConfig};
use core::prelude::*;
use data::ddx::DdxTexture;
use data::era::EraArchive;
use data::xtd::{TerrainVertices, TessellatedMesh, TessellationData, XtdReader};
use data::xtt::{ActiveTextureInfo, XttReader};
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
    tile_scale: f32,
}

impl TerrainMesh {
    fn from_xtd(
        vertices: &TerrainVertices,
        world_min: [f32; 3],
        world_max: [f32; 3],
        tile_scale: f32,
    ) -> Self {
        let indices = vertices.generate_indices();
        Self {
            positions: vertices.positions.clone(),
            normals: vertices.normals.clone(),
            uvs: vertices.uvs.clone(),
            indices,
            world_min,
            world_max,
            tile_scale,
        }
    }

    fn from_tessellated(
        tessellated: TessellatedMesh,
        world_min: [f32; 3],
        world_max: [f32; 3],
        tile_scale: f32,
    ) -> Self {
        Self {
            positions: tessellated.positions,
            normals: tessellated.normals,
            uvs: tessellated.uvs,
            indices: tessellated.indices,
            world_min,
            world_max,
            tile_scale,
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
    params_buffer: wgpu::Buffer,
    terrain_size: [f32; 2],
    tile_scale: f32,
    /// GPU tessellation mode - use instanced patch rendering
    use_gpu_tessellation: bool,
    /// Number of patch instances to draw (64x64 = 4096)
    num_patch_instances: u32,
}

/// Albedo atlas data from XTT file.
struct AlbedoData {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Terrain shader parameters (must match WGSL struct).
/// WGSL alignment rules: vec2=8, vec3=16, f32=4
/// Total struct size must be multiple of largest alignment (16 for vec3).
#[repr(C)]
#[derive(Copy, Clone)]
struct TerrainParams {
    terrain_size: [f32; 2],  // offset 0, size 8
    chunk_count: [f32; 2],   // offset 8, size 8
    texture_tile_scale: f32, // offset 16, size 4
    debug_mode: f32, // offset 20, size 4 (0=normal, 1=alpha, 2=in-chunk UV, 3=raw atlas, 4=terrain UV)
    _padding2: f32,  // offset 24, size 4
    _padding3: f32,  // offset 28, size 4
                     // Total: 32 bytes
}

// SAFETY: TerrainParams is repr(C) with all f32 fields, safe to cast as bytes
unsafe impl bytemuck::Pod for TerrainParams {}
unsafe impl bytemuck::Zeroable for TerrainParams {}

/// GPU tessellation shader parameters.
/// Contains data needed to decode packed positions/normals in the shader.
#[repr(C)]
#[derive(Copy, Clone)]
struct GpuTessParams {
    /// Atlas mid point for position decoding [x, y, z, pad]
    mid: [f32; 4],
    /// Atlas range for position decoding [x, y, z, pad]
    range: [f32; 4],
    /// Terrain size [num_verts_per_axis, tile_scale, num_patches_x, num_patches_z]
    terrain_info: [f32; 4],
    /// World bounds min [x, y, z, pad]
    world_min: [f32; 4],
    /// World bounds max [x, y, z, pad]
    world_max: [f32; 4],
}

// SAFETY: GpuTessParams is repr(C) with all f32 fields
unsafe impl bytemuck::Pod for GpuTessParams {}
unsafe impl bytemuck::Zeroable for GpuTessParams {}

/// A single terrain texture loaded from ERA.
#[allow(dead_code)]
struct TerrainTexture {
    /// Texture name (e.g., "grass_01")
    name: String,
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
    /// RGBA pixel data.
    pixels: Vec<u8>,
    /// U scale from XTT.
    u_scale: i32,
    /// V scale from XTT.
    v_scale: i32,
}

/// Splat data for a single terrain chunk.
#[derive(Clone)]
#[allow(dead_code)]
struct ChunkSplatData {
    /// Grid X position (0-15 for 16x16 grid).
    grid_x: i32,
    /// Grid Z position (0-15 for 16x16 grid).
    grid_z: i32,
    /// Indices into terrain_textures for this chunk's layers.
    layer_texture_ids: Vec<i32>,
    /// Alpha maps for layers 1..n (layer 0 has no alpha, it's the base).
    /// Each is 64x64 = 4096 bytes.
    alpha_maps: Vec<Vec<u8>>,
}

/// Tessellation mode for terrain rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TessellationMode {
    /// No tessellation - use raw XTD mesh.
    None,
    /// CPU tessellation - subdivide mesh on CPU (slow but accurate).
    Cpu,
    /// GPU tessellation - use instanced patches with vertex shader displacement (fast).
    Gpu,
}

impl TessellationMode {
    fn next(self) -> Self {
        match self {
            TessellationMode::None => TessellationMode::Gpu,
            TessellationMode::Gpu => TessellationMode::Cpu,
            TessellationMode::Cpu => TessellationMode::None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            TessellationMode::None => "None",
            TessellationMode::Gpu => "GPU",
            TessellationMode::Cpu => "CPU",
        }
    }
}

/// Raw XTD vertex data for GPU tessellation (before decoding to world positions).
struct RawXtdData {
    /// Packed position data (R10G10B10A2 format).
    packed_positions: Vec<u32>,
    /// Packed normal data.
    packed_normals: Vec<u32>,
    /// Number of vertices per axis (e.g., 1025).
    num_verts_per_axis: u32,
    /// Atlas mid point for decoding.
    mid: [f32; 3],
    /// Atlas range for decoding.
    range: [f32; 3],
    /// Tile scale for world position.
    tile_scale: f32,
}

/// The terrain viewer application.
struct TerrainViewer {
    xtd_path: Option<PathBuf>,
    terrain: Option<TerrainMesh>,
    albedo: Option<AlbedoData>,
    terrain_textures: Vec<TerrainTexture>,
    chunk_splat_data: Vec<ChunkSplatData>,
    camera: Camera,
    show_info: bool,
    wireframe: bool,
    load_error: Option<String>,
    gpu: Option<GpuResources>,
    surface_format: wgpu::TextureFormat,
    /// Debug mode: 0=normal, 1=alpha values, 2=in-chunk UV, 3=raw atlas, 4=terrain UV
    debug_mode: u32,
    /// Tessellation mode: None, CPU, or GPU.
    tessellation_mode: TessellationMode,
    /// Tessellation data from XTD file.
    tessellation_data: Option<TessellationData>,
    /// Raw XTD data for GPU tessellation.
    raw_xtd_data: Option<RawXtdData>,
}

impl TerrainViewer {
    fn new(xtd_path: Option<PathBuf>) -> Self {
        Self {
            xtd_path,
            terrain: None,
            albedo: None,
            terrain_textures: Vec::new(),
            chunk_splat_data: Vec::new(),
            camera: Camera::default(),
            show_info: true,
            wireframe: false,
            load_error: None,
            gpu: None,
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
            debug_mode: 9, // Default to XTT albedo with HD detail
            tessellation_mode: TessellationMode::Gpu, // Default to GPU tessellation (fast)
            tessellation_data: None,
            raw_xtd_data: None,
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
                            self.raw_xtd_data = Some(RawXtdData {
                                packed_positions: raw.packed_positions,
                                packed_normals: raw.packed_normals,
                                num_verts_per_axis: raw.num_verts_per_axis,
                                mid: raw.mid,
                                range: raw.range,
                                tile_scale: raw.tile_scale,
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

        // Debug: print first few ERA entries to understand the path format
        log::debug!("First 10 ERA entries:");
        for (i, entry) in archive.iter().take(10).enumerate() {
            if let Some(name) = &entry.filename {
                log::debug!("  [{}] {}", i, name);
            }
        }

        // Load each texture
        for tex_info in active_textures {
            // Convert texture name to ERA path
            // XTT stores "sw interior\grass_01", we need "art/terrain/sw interior/grass_01_df.ddx"
            let tex_name = tex_info.filename.replace('\\', "/");
            let ddx_path = format!("art/terrain/{}_df.ddx", tex_name);

            log::debug!("Looking for texture: {}", ddx_path);

            // Find the file in the archive (normalize slashes for comparison)
            let ddx_path_normalized = ddx_path.replace('/', "\\").to_lowercase();
            let file_index = archive.iter().enumerate().find(|(_, e)| {
                e.filename
                    .as_ref()
                    .map(|n| n.replace('/', "\\").to_lowercase() == ddx_path_normalized)
                    .unwrap_or(false)
            });

            match file_index {
                Some((idx, _)) => match archive.read_entry(idx) {
                    Ok(data) => match DdxTexture::from_bytes(&data) {
                        Ok(ddx) => match ddx.decode_to_rgba() {
                            Ok(decoded) => {
                                log::info!(
                                    "Loaded terrain texture: {} ({}x{})",
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
                            }
                            Err(e) => {
                                log::warn!("Failed to decode {}: {}", ddx_path, e);
                            }
                        },
                        Err(e) => {
                            log::warn!("Failed to parse DDX {}: {}", ddx_path, e);
                        }
                    },
                    Err(e) => {
                        log::warn!("Failed to read {} from ERA: {}", ddx_path, e);
                    }
                },
                None => {
                    log::debug!("Terrain texture not found in ERA: {}", ddx_path);
                }
            }
        }

        if self.terrain_textures.is_empty() {
            log::info!("No terrain textures loaded from ERA");
        } else {
            log::info!("Loaded {} terrain textures", self.terrain_textures.len());
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
            log::info!("Debug mode: 9 (XTT albedo - original pre-composited from game)");
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
                    ui.label("  T - Toggle tessellation");
                    ui.label("  0-9 - Debug modes");
                    ui.label("  Escape - Quit");
                });
        }
    }

    fn render(&mut self, _ctx: &FrameContext) -> Color {
        // Sky blue clear color
        Color::new(0.4, 0.6, 0.9, 1.0)
    }
}

// Terrain shader with texture splatting support
const TERRAIN_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Terrain texture array (up to 8 textures)
@group(1) @binding(0)
var t_terrain_array: texture_2d_array<f32>;
@group(1) @binding(1)
var s_terrain: sampler;

// Alpha atlas for all chunks (16x16 chunks, 64x64 per chunk = 1024x1024)
// Stores up to 4 alpha channels per texture (RGBA)
@group(1) @binding(2)
var t_alpha_atlas: texture_2d<f32>;

// Per-chunk layer data (256 chunks * 8 layer IDs = 2048 u32s)
@group(1) @binding(3)
var<storage, read> chunk_layers: array<u32>;

// Terrain dimensions (32 bytes total to match Rust struct)
struct TerrainParams {
    terrain_size: vec2<f32>,      // offset 0: World size of terrain (width, depth)
    chunk_count: vec2<f32>,       // offset 8: Number of chunks (16, 16)
    texture_tile_scale: f32,      // offset 16: How many times textures tile
    debug_mode: f32,              // offset 20: Debug visualization mode
    _pad2: f32,                   // offset 24: padding
    _pad3: f32,                   // offset 28: padding
};
@group(1) @binding(4)
var<uniform> params: TerrainParams;

// Pre-composited albedo atlas (all layers blended on CPU)
@group(1) @binding(5)
var t_composited: texture_2d<f32>;

// Separate sampler for alpha atlas (Nearest filtering to avoid chunk boundary bleeding)
@group(1) @binding(6)
var s_alpha: sampler;

// XTT albedo (original pre-composited from game export)
@group(1) @binding(7)
var t_xtt_albedo: texture_2d<f32>;

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

    // Determine which chunk this pixel belongs to (0-15 in each axis)
    let chunk_uv = in.uv * params.chunk_count;
    let chunk_x = u32(clamp(floor(chunk_uv.x), 0.0, params.chunk_count.x - 1.0));
    let chunk_y = u32(clamp(floor(chunk_uv.y), 0.0, params.chunk_count.y - 1.0));
    // Z-major order: index = gridZ * numChunks + gridX (matches Halo Wars linker storage order)
    let chunk_idx = chunk_y * u32(params.chunk_count.x) + chunk_x;

    // UV within the chunk (0-1) for alpha sampling
    let in_chunk_uv = fract(chunk_uv);

    // Calculate alpha atlas UV - each chunk is 64x64 in a 1024x1024 atlas
    let alpha_atlas_uv = (vec2<f32>(f32(chunk_x), f32(chunk_y)) + in_chunk_uv) / params.chunk_count;

    // Sample alpha values for this chunk (RGBA = 4 alpha channels for layers 1-4)
    // Use s_alpha (Nearest filtering) to avoid bleeding at chunk boundaries
    let alphas = textureSample(t_alpha_atlas, s_alpha, alpha_atlas_uv);

    // Calculate tiled UV for detail textures
    // Original shader: uvCoord0 = gPos / 64 (ranges 0 to numXChunks)
    // With u_scale/v_scale=1, textures tile once per chunk
    // Our in.uv is 0-1 across terrain, so multiply by chunk_count to get 0-16
    let tiled_uv = in.uv * params.chunk_count;

    // Get layer indices for this chunk (8 layers max per chunk, stored as u32s)
    let layer_base = chunk_idx * 8u;
    let layer0 = chunk_layers[layer_base];
    let layer1 = chunk_layers[layer_base + 1u];
    let layer2 = chunk_layers[layer_base + 2u];
    let layer3 = chunk_layers[layer_base + 3u];

    // DEBUG MODE: 0=splatting, 1=alpha values, 2=in-chunk UVs, 3=raw atlas, 4=terrain UVs, 5=composited
    // Press 0-5 keys to switch modes
    let debug_mode = i32(params.debug_mode);

    if (debug_mode == 1) {
        // Visualize alpha values as color (RED shows layer 1 alpha)
        return vec4<f32>(alphas.r, alphas.g, alphas.b, 1.0);
    } else if (debug_mode == 2) {
        // Visualize in-chunk UVs (should show smooth gradient within each chunk)
        return vec4<f32>(in_chunk_uv.x, in_chunk_uv.y, 0.0, 1.0);
    } else if (debug_mode == 3) {
        // Sample raw atlas using terrain UV directly (shows atlas as-is)
        let raw_alpha = textureSample(t_alpha_atlas, s_alpha, in.uv);
        return vec4<f32>(raw_alpha.r, raw_alpha.g, raw_alpha.b, 1.0);
    } else if (debug_mode == 4) {
        // Show raw terrain mesh UVs (should be smooth 0-1 gradient across entire terrain)
        return vec4<f32>(in.uv.x, in.uv.y, 0.0, 1.0);
    } else if (debug_mode == 5) {
        // Show pre-composited albedo (correct blending, no boundary issues)
        let comp_color = textureSample(t_composited, s_terrain, in.uv);
        return vec4<f32>(comp_color.rgb * lighting, 1.0);
    } else if (debug_mode == 6) {
        // Debug: Show layer IDs as colors to identify which chunks have which textures
        // Color-code by layer0 texture ID (base layer)
        var id_color = vec3<f32>(0.5, 0.5, 0.5);
        if (layer0 == 0u) { id_color = vec3<f32>(0.0, 0.5, 0.0); }      // grass_01 = dark green
        else if (layer0 == 1u) { id_color = vec3<f32>(0.4, 0.3, 0.2); } // floodmud_01 = brown
        else if (layer0 == 2u) { id_color = vec3<f32>(0.0, 0.8, 0.0); } // grass_05 = bright green
        else if (layer0 == 3u) { id_color = vec3<f32>(0.5, 0.5, 0.5); } // cliffwall_04 = gray
        else if (layer0 == 4u) { id_color = vec3<f32>(0.3, 0.6, 0.3); } // grass_04 = medium green
        else if (layer0 == 5u) { id_color = vec3<f32>(0.2, 0.5, 0.2); } // grass_03 = darker green
        else if (layer0 == 6u) { id_color = vec3<f32>(0.4, 0.7, 0.4); } // grass_06 = light green
        // Overlay layer1 color if it's non-zero
        if (layer1 > 0u) {
            var l1_color = vec3<f32>(0.0, 0.0, 0.0);
            if (layer1 == 1u) { l1_color = vec3<f32>(0.4, 0.3, 0.2); }
            else if (layer1 == 2u) { l1_color = vec3<f32>(0.0, 0.8, 0.0); }
            else if (layer1 == 3u) { l1_color = vec3<f32>(0.5, 0.5, 0.5); }
            else if (layer1 == 4u) { l1_color = vec3<f32>(0.3, 0.6, 0.3); }
            else if (layer1 == 5u) { l1_color = vec3<f32>(0.2, 0.5, 0.2); }
            else if (layer1 == 6u) { l1_color = vec3<f32>(0.4, 0.7, 0.4); }
            id_color = mix(id_color, l1_color, alphas.r);
        }
        return vec4<f32>(id_color * lighting, 1.0);
    } else if (debug_mode == 7) {
        // Debug: Show chunk grid position as colors
        // R = chunk_x / 16, G = chunk_z / 16, B = 0
        // This shows where each chunk is positioned on the terrain
        let chunk_x_f = floor(in.uv.x * params.chunk_count.x);
        let chunk_z_f = floor(in.uv.y * params.chunk_count.y);
        return vec4<f32>(chunk_x_f / 16.0, chunk_z_f / 16.0, 0.0, 1.0);
    } else if (debug_mode == 8) {
        // Debug: Show layer1 (blend layer) info
        // R = layer1 ID / 7 (should show cliffs as ~0.43 = gray)
        // G = alpha for layer1 (shows where blending should happen)
        // B = 1.0 if layer1 > 0 (marks chunks that have a blend layer)
        let has_layer1 = select(0.0, 1.0, layer1 > 0u);
        return vec4<f32>(f32(layer1) / 7.0, alphas.r, has_layer1, 1.0);
    } else if (debug_mode == 9) {
        // XTT albedo (original pre-composited from game export)
        let xtt_color = textureSample(t_xtt_albedo, s_terrain, in.uv);
        return vec4<f32>(xtt_color.rgb * lighting, 1.0);
    }

    // Mode 0: Runtime texture splatting
    var color = textureSample(t_terrain_array, s_terrain, tiled_uv, layer0).rgb;

    // Blend layers 1-3 using alpha values
    // IMPORTANT: Only blend if layer ID is non-zero (ID=0 for layers 1+ means padding/unused)
    // The original game code checks: if(layerIdsSplat[i]) before processing
    if (layer1 > 0u && alphas.r > 0.0) {
        let layer1_color = textureSample(t_terrain_array, s_terrain, tiled_uv, layer1).rgb;
        color = mix(color, layer1_color, alphas.r);
    }
    if (layer2 > 0u && alphas.g > 0.0) {
        let layer2_color = textureSample(t_terrain_array, s_terrain, tiled_uv, layer2).rgb;
        color = mix(color, layer2_color, alphas.g);
    }
    if (layer3 > 0u && alphas.b > 0.0) {
        let layer3_color = textureSample(t_terrain_array, s_terrain, tiled_uv, layer3).rgb;
        color = mix(color, layer3_color, alphas.b);
    }

    return vec4<f32>(color * lighting, 1.0);
}
"#;

/// GPU tessellation shader using instanced patches with vertex displacement.
/// Each instance is a terrain patch, and vertices sample position/normal textures.
const GPU_TESS_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Tessellation parameters for decoding packed positions
struct TessParams {
    mid: vec4<f32>,           // x, y, z, pad
    range: vec4<f32>,         // x, y, z, pad
    terrain_info: vec4<f32>,  // num_verts, tile_scale, num_patches_x, num_patches_z
    world_min: vec4<f32>,     // x, y, z, pad
    world_max: vec4<f32>,     // x, y, z, pad
};
@group(1) @binding(0)
var<uniform> tess_params: TessParams;

// Position texture (R32Uint - packed R10G10B10A2)
@group(1) @binding(1)
var t_positions: texture_2d<u32>;

// Normal texture (R32Uint - packed)
@group(1) @binding(2)
var t_normals: texture_2d<u32>;

// XTT albedo texture for coloring
@group(1) @binding(3)
var t_xtt_albedo: texture_2d<f32>;
@group(1) @binding(4)
var s_terrain: sampler;

// Terrain params (reuse from main shader for debug mode)
struct TerrainParams {
    terrain_size: vec2<f32>,
    chunk_count: vec2<f32>,
    texture_tile_scale: f32,
    debug_mode: f32,
    _pad2: f32,
    _pad3: f32,
};
@group(1) @binding(5)
var<uniform> params: TerrainParams;

struct VertexInput {
    // Per-vertex: local UV within patch [0, 1]
    @location(0) local_uv: vec2<f32>,
    // Per-instance: patch index (x + z * num_patches_x)
    @location(1) patch_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Decode packed R10G10B10A2 position
// Must match Rust decode.rs: (bits / 1023.0) * range - mid
fn unpack_position(packed: u32, mid: vec3<f32>, range: vec3<f32>) -> vec3<f32> {
    let x_bits = packed & 0x3FFu;
    let y_bits = (packed >> 10u) & 0x3FFu;
    let z_bits = (packed >> 20u) & 0x3FFu;

    // Normalize to [0, 1]
    let norm = vec3<f32>(
        f32(x_bits) / 1023.0,
        f32(y_bits) / 1023.0,
        f32(z_bits) / 1023.0
    );

    // Apply range and offset to get displacement
    // Formula: (bits / 1023) * range - mid
    return norm * range - mid;
}

// Decode packed normal
// IMPORTANT: Normal packing uses different bit layout than position!
// Must match Rust decode.rs: x = bits 22-31, y = bits 11-20, z = bits 0-9
fn unpack_normal(packed: u32) -> vec3<f32> {
    let x_bits = (packed >> 22u) & 0x3FFu;  // bits 22-31
    let y_bits = (packed >> 11u) & 0x3FFu;  // bits 11-20
    let z_bits = packed & 0x3FFu;           // bits 0-9

    // Convert to [-1, 1] range: (bits / 1023) * 2 - 1
    let norm = vec3<f32>(
        (f32(x_bits) / 1023.0) * 2.0 - 1.0,
        (f32(y_bits) / 1023.0) * 2.0 - 1.0,
        (f32(z_bits) / 1023.0) * 2.0 - 1.0
    );

    return normalize(norm);
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let num_verts = u32(tess_params.terrain_info.x);
    let tile_scale = tess_params.terrain_info.y;
    let num_patches_x = u32(tess_params.terrain_info.z);
    let num_patches_z = u32(tess_params.terrain_info.w);

    // Calculate patch position from index
    let patch_x = in.patch_index % num_patches_x;
    let patch_z = in.patch_index / num_patches_x;

    // Calculate vertices per patch (terrain has num_verts vertices, num_verts-1 quads)
    // For 64 patches across 1024 vertices: ~16 vertices per patch
    let verts_per_patch_f = f32(num_verts - 1u) / f32(num_patches_x);

    // Global UV across entire terrain [0, 1]
    let global_u = (f32(patch_x) + in.local_uv.x) / f32(num_patches_x);
    let global_v = (f32(patch_z) + in.local_uv.y) / f32(num_patches_z);

    // Convert UV to texture coordinate (pixel coordinate)
    let tex_x = u32(global_u * f32(num_verts - 1u));
    let tex_z = u32(global_v * f32(num_verts - 1u));

    // Clamp to valid range
    let clamped_x = min(tex_x, num_verts - 1u);
    let clamped_z = min(tex_z, num_verts - 1u);

    // Sample position texture (using textureLoad for integer texture)
    let packed_pos = textureLoad(t_positions, vec2<i32>(i32(clamped_x), i32(clamped_z)), 0).r;
    let packed_norm = textureLoad(t_normals, vec2<i32>(i32(clamped_x), i32(clamped_z)), 0).r;

    // Decode position
    let local_pos = unpack_position(packed_pos, tess_params.mid.xyz, tess_params.range.xyz);

    // Calculate world position using grid and displacement
    // This must match decode.rs: position = [grid_x * tile_scale + unpacked[0], unpacked[1], grid_z * tile_scale + unpacked[2]]
    let grid_x = global_u * f32(num_verts - 1u);
    let grid_z = global_v * f32(num_verts - 1u);

    let world_pos = vec3<f32>(
        grid_x * tile_scale + local_pos.x,  // X = grid*scale + displacement
        local_pos.y,                         // Y = height from texture
        grid_z * tile_scale + local_pos.z   // Z = grid*scale + displacement
    );

    // Decode normal
    let normal = unpack_normal(packed_norm);

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.normal = normal;
    out.world_pos = world_pos;
    out.uv = vec2<f32>(global_u, global_v);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let normal = normalize(in.normal);
    let diff = max(dot(normal, light_dir), 0.0);
    let ambient = 0.4;
    let lighting = ambient + diff * 0.6;

    // Sample XTT albedo texture
    let xtt_color = textureSample(t_xtt_albedo, s_terrain, in.uv);

    return vec4<f32>(xtt_color.rgb * lighting, 1.0);
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

        // Update terrain params (for debug mode changes)
        let params = TerrainParams {
            terrain_size: gpu.terrain_size,
            chunk_count: [16.0, 16.0],
            texture_tile_scale: gpu.tile_scale,
            debug_mode: self.debug_mode as f32,
            _padding2: 0.0,
            _padding3: 0.0,
        };
        ctx.queue
            .write_buffer(&gpu.params_buffer, 0, bytemuck::bytes_of(&params));

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
            _padding2: 0.0,
            _padding3: 0.0,
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

        // Separate sampler for alpha atlas - use Nearest to avoid bleeding at chunk boundaries
        let alpha_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Alpha Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
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
                    // binding 2: alpha atlas (non-filterable to use with Nearest sampler)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
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
                    // binding 6: alpha atlas sampler (nearest filtering to avoid chunk boundary bleeding)
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
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
                ],
            });

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

        // Create XTT albedo texture
        let (xtt_albedo_texture, xtt_albedo_view) =
            self.create_xtt_albedo_texture(device, queue, &albedo);

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
            _padding2: 0.0,
            _padding3: 0.0,
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Params Buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Terrain Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
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
                    // binding 0: tess params uniform
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
                    // binding 2: normal texture (R32Uint)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
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
                ],
            });

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
        let _ = xtt_albedo_texture;
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

            log::info!(
                "Creating terrain texture array: {}x{} x {} layers",
                tex_width,
                tex_height,
                layer_count
            );

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Terrain Texture Array"),
                size: wgpu::Extent3d {
                    width: tex_width,
                    height: tex_height,
                    depth_or_array_layers: layer_count,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            // Upload each layer
            for (i, tex) in self.terrain_textures.iter().enumerate() {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: i as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &tex.pixels,
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

                // Copy alpha maps to atlas
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
                        // Note: layer ID of 0 for layers 1+ means padding/unused (like original game)
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
            // Z-major order: index = gridZ * numChunks + gridX (matches Halo Wars linker storage order)
            let chunk_idx = (chunk.grid_z * 16 + chunk.grid_x) as usize;
            let base = chunk_idx * 8;

            for (i, &layer_id) in chunk.layer_texture_ids.iter().enumerate().take(8) {
                layer_data[base + i] = layer_id as u32;
            }
        }

        log::info!(
            "Creating chunk layers buffer: {} chunks",
            self.chunk_splat_data.len()
        );

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Chunk Layers Buffer"),
            contents: bytemuck::cast_slice(&layer_data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }
}

fn main() -> Result<()> {
    // Load .env file if present (ignore errors if not found)
    let _ = dotenvy::dotenv();

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
