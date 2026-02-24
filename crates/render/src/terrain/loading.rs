//! Terrain texture and data loading from ERA archives.
//!
//! Provides reusable functions for loading terrain textures, decals, and foliage
//! from Halo Wars ERA archives. These functions can be used by both the viewer
//! and the engine.
//!
//! Uses parallel loading for maximum performance:
//! - Parallel ERA decompression via memory-mapped archives
//! - Parallel DDX texture decoding via rayon

use super::types::{
    ChunkDecalData, ChunkSplatData, DecalInstance, DecalTexture, FoliageQNChunk, FoliageSet,
    NormalMapTexture, TerrainTexture,
};
use data::assets::AssetSource;
use data::ddx::DdxTexture;
use data::xtt::{ActiveDecalInfo, ActiveTextureInfo, FoliageSetInfo, XttFile};
use rayon::prelude::*;

/// Load terrain textures (diffuse and normal maps) from an asset source.
///
/// Uses parallel loading for both ERA decompression and DDX decoding.
/// Returns a tuple of (terrain_textures, normal_textures).
/// Textures are loaded in the same order as active_textures to maintain index alignment.
pub fn load_terrain_textures(
    source: &AssetSource,
    active_textures: &[ActiveTextureInfo],
) -> (Vec<TerrainTexture>, Vec<NormalMapTexture>) {
    if active_textures.is_empty() {
        return (Vec::new(), Vec::new());
    }

    log::info!(
        "Loading terrain textures in parallel ({} total)",
        active_textures.len()
    );

    // Build all paths (diffuse + normal for each texture)
    let diffuse_paths: Vec<String> = active_textures
        .iter()
        .map(|t| format!("art/terrain/{}_df.ddx", t.filename.replace('\\', "/")))
        .collect();
    let normal_paths: Vec<String> = active_textures
        .iter()
        .map(|t| format!("art/terrain/{}_nm.ddx", t.filename.replace('\\', "/")))
        .collect();

    // Combine all paths for a single parallel read
    let all_paths: Vec<&str> = diffuse_paths
        .iter()
        .chain(normal_paths.iter())
        .map(|s| s.as_str())
        .collect();

    // Load all files in parallel
    let all_data = source.read_parallel(&all_paths);

    // Split results back into diffuse and normal
    let (diffuse_data, normal_data) = all_data.split_at(active_textures.len());

    // Decode diffuse textures in parallel
    let terrain_textures: Vec<TerrainTexture> = diffuse_data
        .par_iter()
        .zip(active_textures.par_iter())
        .enumerate()
        .map(|(idx, (data_opt, tex_info))| {
            if let Some(data) = data_opt {
                if let Ok(ddx) = DdxTexture::from_bytes(data) {
                    if let Ok(decoded) = ddx.decode_to_rgba() {
                        log::info!(
                            "Loaded terrain texture [{}]: {} ({}x{})",
                            idx,
                            tex_info.filename,
                            decoded.width,
                            decoded.height
                        );
                        return TerrainTexture {
                            name: tex_info.filename.clone(),
                            width: decoded.width,
                            height: decoded.height,
                            pixels: decoded.pixels,
                            u_scale: tex_info.u_scale,
                            v_scale: tex_info.v_scale,
                        };
                    }
                }
            }

            // Placeholder for failed loads
            log::warn!(
                "Failed to load texture [{}]: {} - using placeholder",
                idx,
                tex_info.filename
            );
            create_placeholder_texture(&tex_info.filename)
        })
        .collect();

    // Decode normal maps in parallel
    let normal_textures: Vec<NormalMapTexture> = normal_data
        .par_iter()
        .zip(active_textures.par_iter())
        .filter_map(|(data_opt, tex_info)| {
            data_opt.as_ref().and_then(|data| {
                DdxTexture::from_bytes(data).ok().and_then(|ddx| {
                    ddx.decode_to_rgba().ok().map(|decoded| {
                        log::info!(
                            "Loaded normal map: {} ({}x{})",
                            tex_info.filename,
                            decoded.width,
                            decoded.height
                        );
                        NormalMapTexture {
                            name: tex_info.filename.clone(),
                            width: decoded.width,
                            height: decoded.height,
                            pixels: decoded.pixels,
                        }
                    })
                })
            })
        })
        .collect();

    log::info!(
        "Loaded {} terrain textures, {} normal maps",
        terrain_textures.len(),
        normal_textures.len()
    );

    (terrain_textures, normal_textures)
}

/// Create a magenta placeholder texture for missing textures.
fn create_placeholder_texture(name: &str) -> TerrainTexture {
    let placeholder_size = 64u32;
    let mut pixels = vec![0u8; (placeholder_size * placeholder_size * 4) as usize];
    for i in 0..(placeholder_size * placeholder_size) as usize {
        pixels[i * 4] = 255; // R
        pixels[i * 4 + 1] = 0; // G
        pixels[i * 4 + 2] = 255; // B (magenta)
        pixels[i * 4 + 3] = 255; // A
    }
    TerrainTexture {
        name: format!("placeholder_{}", name),
        width: placeholder_size,
        height: placeholder_size,
        pixels,
        u_scale: 1,
        v_scale: 1,
    }
}

/// Load decal textures (diffuse and opacity) from an asset source.
///
/// Uses parallel loading for both ERA decompression and DDX decoding.
pub fn load_decal_textures(
    source: &AssetSource,
    active_decals: &[ActiveDecalInfo],
) -> Vec<DecalTexture> {
    if active_decals.is_empty() {
        log::info!("No active decals to load");
        return Vec::new();
    }

    log::info!(
        "Loading decal textures in parallel ({} total)",
        active_decals.len()
    );

    // Build all paths (diffuse + opacity for each decal)
    let diffuse_paths: Vec<String> = active_decals
        .iter()
        .map(|d| format!("art/terrain/{}_df.ddx", d.filename.replace('\\', "/")))
        .collect();
    let opacity_paths: Vec<String> = active_decals
        .iter()
        .map(|d| format!("art/terrain/{}_op.ddx", d.filename.replace('\\', "/")))
        .collect();

    // Combine all paths for a single parallel read
    let all_paths: Vec<&str> = diffuse_paths
        .iter()
        .chain(opacity_paths.iter())
        .map(|s| s.as_str())
        .collect();

    // Load all files in parallel
    let all_data = source.read_parallel(&all_paths);

    // Split results
    let (diffuse_data, opacity_data) = all_data.split_at(active_decals.len());

    // Decode and combine in parallel
    let decal_textures: Vec<DecalTexture> = (0..active_decals.len())
        .into_par_iter()
        .map(|i| {
            let decal_info = &active_decals[i];

            // Decode diffuse
            let (width, height, diffuse_pixels) =
                if let Some(data) = &diffuse_data[i] {
                    if let Ok(ddx) = DdxTexture::from_bytes(data) {
                        if let Ok(decoded) = ddx.decode_to_rgba() {
                            log::info!(
                                "Loaded decal diffuse: {} ({}x{})",
                                decal_info.filename,
                                decoded.width,
                                decoded.height
                            );
                            (decoded.width, decoded.height, decoded.pixels)
                        } else {
                            (64, 64, vec![128u8; 64 * 64 * 4])
                        }
                    } else {
                        (64, 64, vec![128u8; 64 * 64 * 4])
                    }
                } else {
                    log::warn!("Failed to load decal diffuse: {}", decal_info.filename);
                    (64, 64, vec![128u8; 64 * 64 * 4])
                };

            // Decode opacity
            let opacity_pixels = if let Some(data) = &opacity_data[i] {
                if let Ok(ddx) = DdxTexture::from_bytes(data) {
                    if let Ok(decoded) = ddx.decode_to_rgba() {
                        log::info!(
                            "Loaded decal opacity: {} ({}x{})",
                            decal_info.filename,
                            decoded.width,
                            decoded.height
                        );
                        decoded.pixels
                    } else {
                        vec![255u8; (width * height * 4) as usize]
                    }
                } else {
                    vec![255u8; (width * height * 4) as usize]
                }
            } else {
                log::warn!("Failed to load decal opacity: {}", decal_info.filename);
                vec![255u8; (width * height * 4) as usize]
            };

            DecalTexture {
                name: decal_info.filename.clone(),
                width,
                height,
                diffuse_pixels,
                opacity_pixels,
            }
        })
        .collect();

    log::info!("Loaded {} decal textures", decal_textures.len());
    decal_textures
}

/// Load foliage textures and geometry from an asset source.
///
/// Uses parallel loading for both ERA decompression and DDX decoding.
/// Each foliage set has 4 textures: albedo (_df), opacity (_op), normal (_nm), specular (_sp).
pub fn load_foliage_sets(
    source: &AssetSource,
    foliage_sets: &[FoliageSetInfo],
) -> Vec<FoliageSet> {
    if foliage_sets.is_empty() {
        log::info!("No foliage sets to load");
        return Vec::new();
    }

    log::info!(
        "Loading foliage sets in parallel ({} total)",
        foliage_sets.len()
    );

    // Build all paths (4 textures per foliage set)
    let mut all_paths: Vec<String> = Vec::with_capacity(foliage_sets.len() * 4);
    for set_info in foliage_sets {
        let base_path = format!("art/{}", set_info.filename.replace('\\', "/"));
        all_paths.push(format!("{}_df.ddx", base_path)); // albedo
        all_paths.push(format!("{}_op.ddx", base_path)); // opacity
        all_paths.push(format!("{}_nm.ddx", base_path)); // normal
        all_paths.push(format!("{}_sp.ddx", base_path)); // specular
    }

    // Load all files in parallel
    let path_refs: Vec<&str> = all_paths.iter().map(|s| s.as_str()).collect();
    let all_data = source.read_parallel(&path_refs);

    // Process each foliage set in parallel
    let loaded_sets: Vec<FoliageSet> = (0..foliage_sets.len())
        .into_par_iter()
        .map(|i| {
            let set_info = &foliage_sets[i];
            let base_idx = i * 4;

            let mut foliage_set = FoliageSet {
                name: set_info.filename.clone(),
                ..Default::default()
            };

            // Decode albedo (_df)
            if let Some(data) = &all_data[base_idx] {
                if let Ok(ddx) = DdxTexture::from_bytes(data) {
                    if let Ok(decoded) = ddx.decode_to_rgba() {
                        log::info!(
                            "  Loaded foliage albedo: {} ({}x{})",
                            set_info.filename,
                            decoded.width,
                            decoded.height
                        );
                        foliage_set.albedo_width = decoded.width;
                        foliage_set.albedo_height = decoded.height;
                        foliage_set.albedo_pixels = decoded.pixels;
                    }
                }
            }

            // Decode opacity (_op)
            if let Some(data) = &all_data[base_idx + 1] {
                if let Ok(ddx) = DdxTexture::from_bytes(data) {
                    if let Ok(decoded) = ddx.decode_to_rgba() {
                        foliage_set.opacity_width = decoded.width;
                        foliage_set.opacity_height = decoded.height;
                        foliage_set.opacity_pixels = decoded.pixels;
                    }
                }
            }

            // Decode normal (_nm)
            if let Some(data) = &all_data[base_idx + 2] {
                if let Ok(ddx) = DdxTexture::from_bytes(data) {
                    if let Ok(decoded) = ddx.decode_to_rgba() {
                        foliage_set.normal_width = decoded.width;
                        foliage_set.normal_height = decoded.height;
                        foliage_set.normal_pixels = decoded.pixels;
                    }
                }
            }

            // Decode specular (_sp)
            if let Some(data) = &all_data[base_idx + 3] {
                if let Ok(ddx) = DdxTexture::from_bytes(data) {
                    if let Ok(decoded) = ddx.decode_to_rgba() {
                        foliage_set.specular_width = decoded.width;
                        foliage_set.specular_height = decoded.height;
                        foliage_set.specular_pixels = decoded.pixels;
                    }
                }
            }

            foliage_set
        })
        .collect();

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

