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
    NormalMapTexture, RoadChunkData, TerrainTexture,
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
            if let Some(data) = data_opt
                && let Ok(ddx) = DdxTexture::from_bytes(data)
                && let Ok(decoded) = ddx.decode_to_rgba()
            {
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
            let (width, height, diffuse_pixels) = if let Some(data) = &diffuse_data[i] {
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
pub fn load_foliage_sets(source: &AssetSource, foliage_sets: &[FoliageSetInfo]) -> Vec<FoliageSet> {
    if foliage_sets.is_empty() {
        log::info!("No foliage sets to load");
        return Vec::new();
    }

    log::info!(
        "Loading foliage sets in parallel ({} total)",
        foliage_sets.len()
    );

    // Build all paths (4 textures + 1 XML per foliage set = 5 per set)
    let files_per_set = 5;
    let mut all_paths: Vec<String> = Vec::with_capacity(foliage_sets.len() * files_per_set);
    for set_info in foliage_sets {
        let base_path = format!("art/{}", set_info.filename.replace('\\', "/"));
        let paths = [
            format!("{}_df.ddx", base_path),
            format!("{}_op.ddx", base_path),
            format!("{}_nm.ddx", base_path),
            format!("{}_sp.ddx", base_path),
            format!("{}.xml", base_path),
        ];
        for p in &paths {
            log::info!("  Foliage asset path: {}", p);
        }
        all_paths.extend(paths);
    }

    // Load all files in parallel
    let path_refs: Vec<&str> = all_paths.iter().map(|s| s.as_str()).collect();
    let all_data = source.read_parallel(&path_refs);

    // Log which files were found vs missing
    for (i, path) in all_paths.iter().enumerate() {
        match &all_data[i] {
            Some(data) => log::info!("  Found: {} ({} bytes)", path, data.len()),
            None => log::warn!("  MISSING: {}", path),
        }
    }

    // Process each foliage set in parallel
    let loaded_sets: Vec<FoliageSet> = (0..foliage_sets.len())
        .into_par_iter()
        .map(|i| {
            let set_info = &foliage_sets[i];
            let base_idx = i * files_per_set;

            let mut foliage_set = FoliageSet {
                name: set_info.filename.clone(),
                ..Default::default()
            };

            // Decode albedo (_df)
            if let Some(data) = &all_data[base_idx] {
                log::info!("  Decoding foliage albedo DDX for '{}' ({} bytes)", set_info.filename, data.len());
                match DdxTexture::from_bytes(data) {
                    Ok(ddx) => match ddx.decode_to_rgba() {
                        Ok(decoded) => {
                            log::info!("    Decoded albedo: {}x{}, {} bytes", decoded.width, decoded.height, decoded.pixels.len());
                            foliage_set.albedo_width = decoded.width;
                            foliage_set.albedo_height = decoded.height;
                            foliage_set.albedo_pixels = decoded.pixels;
                        }
                        Err(e) => log::error!("    Failed to decode albedo DDX: {}", e),
                    },
                    Err(e) => log::error!("    Failed to parse albedo DDX: {}", e),
                }
            } else {
                log::warn!("  No albedo data found for '{}'", set_info.filename);
            }

            // Decode opacity (_op)
            if let Some(data) = &all_data[base_idx + 1]
                && let Ok(ddx) = DdxTexture::from_bytes(data)
                && let Ok(decoded) = ddx.decode_to_rgba()
            {
                log::info!(
                    "  Loaded foliage opacity: {} ({}x{})",
                    set_info.filename,
                    decoded.width,
                    decoded.height
                );
                foliage_set.opacity_width = decoded.width;
                foliage_set.opacity_height = decoded.height;
                foliage_set.opacity_pixels = decoded.pixels;
            } else {
                log::warn!("  No foliage opacity texture for: {}", set_info.filename);
            }

            // Decode normal (_nm)
            if let Some(data) = &all_data[base_idx + 2]
                && let Ok(ddx) = DdxTexture::from_bytes(data)
                && let Ok(decoded) = ddx.decode_to_rgba()
            {
                foliage_set.normal_width = decoded.width;
                foliage_set.normal_height = decoded.height;
                foliage_set.normal_pixels = decoded.pixels;
            }

            // Decode specular (_sp)
            if let Some(data) = &all_data[base_idx + 3]
                && let Ok(ddx) = DdxTexture::from_bytes(data)
                && let Ok(decoded) = ddx.decode_to_rgba()
            {
                foliage_set.specular_width = decoded.width;
                foliage_set.specular_height = decoded.height;
                foliage_set.specular_pixels = decoded.pixels;
            }

            // Parse blade geometry from XML (.xml.xmb)
            if let Some(xml_data) = &all_data[base_idx + 4] {
                parse_foliage_blade_xml(xml_data, &mut foliage_set);
            } else {
                log::warn!("  No foliage blade XML for: {}", set_info.filename);
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

/// Extract road data from XTT file.
/// Returns a list of road chunks (one per road in the scenario).
pub fn extract_road_data(xtt: &XttFile) -> Vec<RoadChunkData> {
    if xtt.road_data.is_empty() {
        log::info!("No road data in XTT");
        return Vec::new();
    }

    match data::xtt::decode_road_data(&xtt.road_data) {
        Ok(road) => {
            let total_verts: usize = road.qn_chunks.iter().map(|qn| qn.vertices.len()).sum();
            log::info!(
                "Decoded road '{}': {} QN chunks, {} total vertices",
                road.texture_name,
                road.qn_chunks.len(),
                total_verts
            );

            // Flatten all QN chunk vertices into a single vertex list
            let mut positions = Vec::with_capacity(total_verts);
            let mut uvs = Vec::with_capacity(total_verts);

            for qn in &road.qn_chunks {
                for vert in &qn.vertices {
                    positions.push(vert.position);
                    uvs.push(vert.uv);
                }
            }

            vec![RoadChunkData {
                texture_name: road.texture_name,
                positions,
                uvs,
            }]
        }
        Err(e) => {
            log::warn!("Failed to decode road data: {}", e);
            Vec::new()
        }
    }
}

/// Load road textures (albedo, normal, specular) from an asset source.
/// Returns (albedo_pixels, normal_pixels, specular_pixels, width, height) or None.
pub fn load_road_textures(source: &AssetSource, texture_name: &str) -> Option<RoadTextures> {
    let base = format!("art/{}", texture_name.replace('\\', "/"));
    let paths = [
        format!("{}_df.ddx", base),
        format!("{}_nm.ddx", base),
        format!("{}_sp.ddx", base),
    ];

    let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    let file_data = source.read_parallel(&path_refs);

    let albedo = file_data[0].as_ref().and_then(|d| {
        DdxTexture::from_bytes(d)
            .ok()
            .and_then(|t| t.decode_to_rgba().ok())
    });
    let normal = file_data[1].as_ref().and_then(|d| {
        DdxTexture::from_bytes(d)
            .ok()
            .and_then(|t| t.decode_to_rgba().ok())
    });
    let specular = file_data[2].as_ref().and_then(|d| {
        DdxTexture::from_bytes(d)
            .ok()
            .and_then(|t| t.decode_to_rgba().ok())
    });

    if let Some(albedo_tex) = albedo {
        let width = albedo_tex.width;
        let height = albedo_tex.height;
        log::info!(
            "Loaded road textures for '{}': {}x{} (normal={}, specular={})",
            texture_name,
            width,
            height,
            normal.is_some(),
            specular.is_some()
        );
        Some(RoadTextures {
            width,
            height,
            albedo_pixels: albedo_tex.pixels,
            normal_pixels: normal
                .map(|t| t.pixels)
                .unwrap_or_else(|| vec![128u8; (width * height * 4) as usize]),
            specular_pixels: specular
                .map(|t| t.pixels)
                .unwrap_or_else(|| vec![0u8; (width * height * 4) as usize]),
        })
    } else {
        log::warn!("Failed to load road albedo texture: {}_df.ddx", base);
        None
    }
}

/// Road texture data (albedo, normal, specular).
pub struct RoadTextures {
    pub width: u32,
    pub height: u32,
    pub albedo_pixels: Vec<u8>,
    pub normal_pixels: Vec<u8>,
    pub specular_pixels: Vec<u8>,
}

/// Parse foliage blade geometry from an XMB (compiled XML) file.
///
/// The XML format (from TerrainFoliage.cpp) is:
/// ```xml
/// <foliageset typecount="N" numVertsPerType="10" backsideShadowScalar="1.0">
///   <setElements>
///     <setElement>
///       <elementVerts>
///         <vert pos="x,y,z" norm="x,y,z" uv="u,v"/>
///         ...
///       </elementVerts>
///     </setElement>
///   </setElements>
/// </foliageset>
/// ```
///
/// Data is stored as:
/// - positions texture: [pos.x, pos.y, pos.z, uv.x]
/// - normals texture: [norm.x, norm.y, norm.z, uv.y]
fn parse_foliage_blade_xml(xml_data: &[u8], foliage_set: &mut FoliageSet) {
    use data::xmb::{Document as XmbDocument, Reader as XmbReader};

    // Try XMB binary first, then fall back to raw XML text
    let xmb = match XmbReader::read(xml_data) {
        Ok(xmb) => xmb,
        Err(_) => {
            // Try as raw XML text
            let xml_str = match std::str::from_utf8(xml_data) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("  Foliage XML is not valid UTF-8: {}", e);
                    return;
                }
            };
            match XmbDocument::from_xml(xml_str) {
                Ok(xmb) => xmb,
                Err(e) => {
                    log::warn!("  Failed to parse foliage XML: {}", e);
                    return;
                }
            }
        }
    };

    let root = match xmb.root() {
        Some(r) => r,
        None => {
            log::warn!("  Foliage XMB has no root node");
            return;
        }
    };

    // Read attributes from root <foliageset> node
    let mut num_blade_types: u32 = 0;
    let mut num_verts_per_type: u32 = 10;

    if let Some(attr) = root.get_attribute("typecount") {
        num_blade_types = attr.value_string().parse().unwrap_or(0);
    }
    if let Some(attr) = root.get_attribute("numVertsPerType") {
        num_verts_per_type = attr.value_string().parse().unwrap_or(10);
    }
    if let Some(attr) = root.get_attribute("backsideShadowScalar") {
        foliage_set.backside_shadow_scalar = attr.value_string().parse().unwrap_or(1.0);
    }

    if num_blade_types == 0 || num_verts_per_type == 0 {
        log::warn!(
            "  Foliage XMB has invalid blade counts: types={}, verts={}",
            num_blade_types,
            num_verts_per_type
        );
        return;
    }

    let total_verts = (num_blade_types * num_verts_per_type) as usize;
    let mut positions: Vec<[f32; 4]> = Vec::with_capacity(total_verts);
    let mut normals: Vec<[f32; 4]> = Vec::with_capacity(total_verts);

    // Find <setElements> node
    let set_elements = match root.children.iter().find(|n| n.name == "setElements") {
        Some(n) => n,
        None => {
            log::warn!("  Foliage XMB missing <setElements>");
            return;
        }
    };

    // Iterate <setElement> → <elementVerts> → <vert>
    for set_element in &set_elements.children {
        if set_element.name != "setElement" {
            continue;
        }
        for child in &set_element.children {
            if child.name != "elementVerts" {
                continue;
            }
            for vert_node in &child.children {
                if vert_node.name != "vert" {
                    continue;
                }
                if positions.len() >= total_verts {
                    break;
                }

                let pos = parse_vector_attr(vert_node, "pos");
                let nrm = parse_vector_attr(vert_node, "norm");
                let uv = parse_vector2_attr(vert_node, "uv");

                positions.push([pos[0], pos[1], pos[2], uv[0]]);
                normals.push([nrm[0], nrm[1], nrm[2], uv[1]]);
            }
        }
    }

    log::info!(
        "  Loaded foliage blade geometry: {} ({} blade types, {} verts/blade, {} total verts)",
        foliage_set.name,
        num_blade_types,
        num_verts_per_type,
        positions.len()
    );

    foliage_set.num_blade_types = num_blade_types;
    foliage_set.num_verts_per_blade = num_verts_per_type;
    foliage_set.blade_positions = positions;
    foliage_set.blade_normals = normals;
}

/// Parse a "x,y,z" vector attribute from an XMB node.
fn parse_vector_attr(node: &data::xmb::Node, attr_name: &str) -> [f32; 3] {
    if let Some(attr) = node.get_attribute(attr_name) {
        let s = attr.value_string();
        let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if parts.len() >= 3 {
            return [parts[0], parts[1], parts[2]];
        }
    }
    [0.0, 0.0, 0.0]
}

/// Parse a "u,v" vector2 attribute from an XMB node.
fn parse_vector2_attr(node: &data::xmb::Node, attr_name: &str) -> [f32; 2] {
    if let Some(attr) = node.get_attribute(attr_name) {
        let s = attr.value_string();
        let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if parts.len() >= 2 {
            return [parts[0], parts[1]];
        }
    }
    [0.0, 0.0]
}
