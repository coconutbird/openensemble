//! Terrain texture and data loading from ERA archives.
//!
//! Provides reusable functions for loading terrain textures, decals, and foliage
//! from Halo Wars ERA archives. These functions can be used by both the viewer
//! and the engine.

use super::types::{
    ChunkDecalData, ChunkSplatData, DecalInstance, DecalTexture, FoliageQNChunk, FoliageSet,
    NormalMapTexture, TerrainTexture,
};
use data::assets::AssetSource;
use data::ddx::DdxTexture;
use data::xtt::{ActiveDecalInfo, ActiveTextureInfo, FoliageSetInfo, XttFile};

/// Load terrain textures (diffuse and normal maps) from an asset source.
///
/// Returns a tuple of (terrain_textures, normal_textures).
/// Textures are loaded in the same order as active_textures to maintain index alignment.
pub fn load_terrain_textures(
    source: &mut AssetSource,
    active_textures: &[ActiveTextureInfo],
) -> (Vec<TerrainTexture>, Vec<NormalMapTexture>) {
    let mut terrain_textures = Vec::new();
    let mut normal_textures = Vec::new();

    log::info!("Loading terrain textures ({} total)", active_textures.len());

    for tex_info in active_textures {
        let tex_name = tex_info.filename.replace('\\', "/");

        // Load diffuse texture (_df.ddx)
        let ddx_path = format!("art/terrain/{}_df.ddx", tex_name);

        let mut loaded = false;
        if let Ok(data) = source.read(&ddx_path)
            && let Ok(ddx) = DdxTexture::from_bytes(&data)
            && let Ok(decoded) = ddx.decode_to_rgba()
        {
            log::info!(
                "Loaded terrain texture [{}]: {} ({}x{})",
                terrain_textures.len(),
                tex_info.filename,
                decoded.width,
                decoded.height
            );
            terrain_textures.push(TerrainTexture {
                name: tex_info.filename.clone(),
                width: decoded.width,
                height: decoded.height,
                pixels: decoded.pixels,
                u_scale: tex_info.u_scale,
                v_scale: tex_info.v_scale,
            });
            loaded = true;
        }

        // Always maintain index alignment - add placeholder if loading failed
        if !loaded {
            log::warn!(
                "Failed to load texture [{}]: {} - using placeholder",
                terrain_textures.len(),
                tex_info.filename
            );
            let placeholder_size = 64u32;
            let mut pixels = vec![0u8; (placeholder_size * placeholder_size * 4) as usize];
            for i in 0..(placeholder_size * placeholder_size) as usize {
                pixels[i * 4] = 255; // R
                pixels[i * 4 + 1] = 0; // G
                pixels[i * 4 + 2] = 255; // B (magenta)
                pixels[i * 4 + 3] = 255; // A
            }
            terrain_textures.push(TerrainTexture {
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

        if let Ok(data) = source.read(&nm_path)
            && let Ok(ddx) = DdxTexture::from_bytes(&data)
            && let Ok(decoded) = ddx.decode_to_rgba()
        {
            log::info!(
                "Loaded normal map: {} ({}x{})",
                tex_info.filename,
                decoded.width,
                decoded.height
            );
            normal_textures.push(NormalMapTexture {
                name: tex_info.filename.clone(),
                width: decoded.width,
                height: decoded.height,
                pixels: decoded.pixels,
            });
        }
    }

    log::info!(
        "Loaded {} terrain textures, {} normal maps",
        terrain_textures.len(),
        normal_textures.len()
    );

    (terrain_textures, normal_textures)
}

/// Load decal textures (diffuse and opacity) from an asset source.
pub fn load_decal_textures(
    source: &mut AssetSource,
    active_decals: &[ActiveDecalInfo],
) -> Vec<DecalTexture> {
    if active_decals.is_empty() {
        log::info!("No active decals to load");
        return Vec::new();
    }

    let mut decal_textures = Vec::new();

    log::info!("Loading decal textures ({} total)", active_decals.len());

    for decal_info in active_decals {
        let decal_name = decal_info.filename.replace('\\', "/");

        // Load diffuse texture (_df.ddx)
        let df_path = format!("art/terrain/{}_df.ddx", decal_name);

        let mut diffuse_pixels = Vec::new();
        let mut width = 64u32;
        let mut height = 64u32;

        if let Ok(data) = source.read(&df_path)
            && let Ok(ddx) = DdxTexture::from_bytes(&data)
            && let Ok(decoded) = ddx.decode_to_rgba()
        {
            width = decoded.width;
            height = decoded.height;
            diffuse_pixels = decoded.pixels;
            log::info!(
                "Loaded decal diffuse: {} ({}x{})",
                decal_info.filename,
                width,
                height
            );
        }

        if diffuse_pixels.is_empty() {
            log::warn!("Failed to load decal diffuse: {}", decal_info.filename);
            diffuse_pixels = vec![128u8; (width * height * 4) as usize];
        }

        // Load opacity texture (_op.ddx)
        let op_path = format!("art/terrain/{}_op.ddx", decal_name);

        let mut opacity_pixels = Vec::new();

        if let Ok(data) = source.read(&op_path)
            && let Ok(ddx) = DdxTexture::from_bytes(&data)
            && let Ok(decoded) = ddx.decode_to_rgba()
        {
            opacity_pixels = decoded.pixels;
            log::info!(
                "Loaded decal opacity: {} ({}x{})",
                decal_info.filename,
                decoded.width,
                decoded.height
            );
        }

        if opacity_pixels.is_empty() {
            log::warn!("Failed to load decal opacity: {}", decal_info.filename);
            opacity_pixels = vec![255u8; (width * height * 4) as usize];
        }

        decal_textures.push(DecalTexture {
            name: decal_info.filename.clone(),
            width,
            height,
            diffuse_pixels,
            opacity_pixels,
        });
    }

    log::info!("Loaded {} decal textures", decal_textures.len());
    decal_textures
}

/// Load foliage textures and geometry from an asset source.
pub fn load_foliage_sets(
    source: &mut AssetSource,
    foliage_sets: &[FoliageSetInfo],
) -> Vec<FoliageSet> {
    if foliage_sets.is_empty() {
        log::info!("No foliage sets to load");
        return Vec::new();
    }

    let mut loaded_sets = Vec::new();

    log::info!("Loading foliage sets ({} total)", foliage_sets.len());

    for set_info in foliage_sets {
        let set_name = set_info.filename.replace('\\', "/");
        let mut foliage_set = FoliageSet {
            name: set_info.filename.clone(),
            ..Default::default()
        };

        let base_path = format!("art/{}", set_name);

        // Load albedo/diffuse texture (_df.ddx)
        let df_path = format!("{}_df.ddx", base_path);
        if let Ok(data) = source.read(&df_path)
            && let Ok(ddx) = DdxTexture::from_bytes(&data)
            && let Ok(decoded) = ddx.decode_to_rgba()
        {
            foliage_set.albedo_width = decoded.width;
            foliage_set.albedo_height = decoded.height;
            foliage_set.albedo_pixels = decoded.pixels;
            log::info!(
                "  Loaded foliage albedo: {} ({}x{})",
                set_info.filename,
                decoded.width,
                decoded.height
            );
        }

        // Load opacity texture (_op.ddx)
        let op_path = format!("{}_op.ddx", base_path);
        if let Ok(data) = source.read(&op_path)
            && let Ok(ddx) = DdxTexture::from_bytes(&data)
            && let Ok(decoded) = ddx.decode_to_rgba()
        {
            foliage_set.opacity_width = decoded.width;
            foliage_set.opacity_height = decoded.height;
            foliage_set.opacity_pixels = decoded.pixels;
        }

        // Load normal map texture (_nm.ddx)
        let nm_path = format!("{}_nm.ddx", base_path);
        if let Ok(data) = source.read(&nm_path)
            && let Ok(ddx) = DdxTexture::from_bytes(&data)
            && let Ok(decoded) = ddx.decode_to_rgba()
        {
            foliage_set.normal_width = decoded.width;
            foliage_set.normal_height = decoded.height;
            foliage_set.normal_pixels = decoded.pixels;
        }

        // Load specular texture (_sp.ddx)
        let sp_path = format!("{}_sp.ddx", base_path);
        if let Ok(data) = source.read(&sp_path)
            && let Ok(ddx) = DdxTexture::from_bytes(&data)
            && let Ok(decoded) = ddx.decode_to_rgba()
        {
            foliage_set.specular_width = decoded.width;
            foliage_set.specular_height = decoded.height;
            foliage_set.specular_pixels = decoded.pixels;
        }

        loaded_sets.push(foliage_set);
    }

    log::info!(
        "Loaded {} foliage sets ({} with albedo textures)",
        loaded_sets.len(),
        loaded_sets
            .iter()
            .filter(|s| !s.albedo_pixels.is_empty())
            .count()
    );

    loaded_sets
}

/// Extract chunk splat data from XTT linkers.
pub fn extract_chunk_splat_data(xtt: &XttFile) -> Vec<ChunkSplatData> {
    let mut chunk_splat_data = Vec::new();

    for linker in &xtt.linkers {
        // Decode alpha maps for this chunk
        let alpha_maps = match linker.decode_splat_alpha() {
            Ok(alpha_data) => alpha_data.alpha_maps,
            Err(e) => {
                log::warn!(
                    "Failed to decode alpha for chunk ({}, {}): {}",
                    linker.grid_x,
                    linker.grid_z,
                    e
                );
                Vec::new()
            }
        };

        chunk_splat_data.push(ChunkSplatData {
            grid_x: linker.grid_x,
            grid_z: linker.grid_z,
            layer_texture_ids: linker.splat_layer_ids.clone(),
            alpha_maps,
        });
    }

    log::info!("Extracted splat data for {} chunks", chunk_splat_data.len());
    chunk_splat_data
}

/// Extract decal instances and chunk decal data from XTT.
pub fn extract_decal_data(xtt: &XttFile) -> (Vec<DecalInstance>, Vec<ChunkDecalData>) {
    let mut decal_instances = Vec::new();
    let mut chunk_decal_data = Vec::new();

    // Extract decal instances
    for instance in &xtt.decal_instances {
        decal_instances.push(DecalInstance {
            decal_index: instance.active_decal_index,
            rotation: instance.rotation,
            tile_center_x: instance.tile_center_x,
            tile_center_y: instance.tile_center_y,
            u_scale: instance.u_scale,
            v_scale: instance.v_scale,
        });
    }

    // Extract per-chunk decal data
    for linker in &xtt.linkers {
        if linker.num_decal_layers > 0 {
            let alpha_maps = match linker.decode_decal_alpha() {
                Ok(alpha_data) => alpha_data.alpha_maps,
                Err(e) => {
                    log::warn!(
                        "Failed to decode decal alpha for chunk ({}, {}): {}",
                        linker.grid_x,
                        linker.grid_z,
                        e
                    );
                    Vec::new()
                }
            };

            chunk_decal_data.push(ChunkDecalData {
                grid_x: linker.grid_x,
                grid_z: linker.grid_z,
                decal_layer_ids: linker.decal_layer_ids.clone(),
                alpha_maps,
            });
        }
    }

    log::info!(
        "Extracted {} decal instances, {} chunks with decals",
        decal_instances.len(),
        chunk_decal_data.len()
    );

    (decal_instances, chunk_decal_data)
}

/// Extract foliage QN chunks from XTT.
pub fn extract_foliage_chunks(xtt: &XttFile) -> Vec<FoliageQNChunk> {
    let mut foliage_chunks = Vec::new();

    for qn in &xtt.foliage.qn_chunks {
        foliage_chunks.push(FoliageQNChunk {
            qn_parent_index: qn.qn_parent_index,
            num_sets: qn.num_sets,
            set_indices: qn.set_indices.clone(),
            set_poly_counts: qn.set_poly_counts.clone(),
            index_buffers: qn.index_buffers.clone(),
        });
    }

    log::info!("Extracted {} foliage QN chunks", foliage_chunks.len());
    foliage_chunks
}

