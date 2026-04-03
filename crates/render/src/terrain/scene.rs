//! TerrainScene — all decoded terrain assets bundled into a single struct.
//!
//! `TerrainScene::load` extracts data from XTD/XTT files and loads textures
//! from the ERA asset source in parallel, replacing the scattered extract/load
//! calls that were previously spread across the terrain viewer.

use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xtd::{TessellationData, XtdFile};
use pipeline::xtt::XttFile;

use super::loading;
use super::mesh::TessellationMode;
use super::types::*;
use super::TerrainMesh;

/// All decoded terrain data needed for rendering.
///
/// This bundles mesh data (from XTD), texture/splat/decal/foliage/road data
/// (from XTT + ERA textures) into a single struct. Create one via
/// [`TerrainScene::load`].
pub struct TerrainScene {
    // -- XTD data --
    /// Decoded terrain mesh (positions, normals, UVs, indices).
    pub mesh: TerrainMesh,
    /// Tessellation data from XTD (patch levels).
    pub tessellation_data: Option<TessellationData>,
    /// Raw packed vertex data for GPU tessellation.
    pub raw_xtd_data: Option<RawXtdData>,

    // -- XTT data --
    /// Decoded albedo atlas from XTT.
    pub albedo: Option<AlbedoData>,
    /// Terrain diffuse textures loaded from ERA.
    pub terrain_textures: Vec<TerrainTexture>,
    /// Normal map textures loaded from ERA.
    pub normal_textures: Vec<NormalMapTexture>,
    /// Per-chunk splat layer data.
    pub chunk_splat_data: Vec<ChunkSplatData>,
    /// Decal textures loaded from ERA.
    pub decal_textures: Vec<DecalTexture>,
    /// Decal placement instances from XTT.
    pub decal_instances: Vec<DecalInstance>,
    /// Per-chunk decal layer data.
    pub chunk_decal_data: Vec<ChunkDecalData>,
    /// Foliage texture + geometry sets loaded from ERA.
    pub foliage_sets: Vec<FoliageSet>,
    /// Per quad-node foliage chunk data from XTT.
    pub foliage_qn_chunks: Vec<FoliageQNChunk>,
    /// Road vertex data extracted from XTT.
    pub road_chunks: Vec<RoadChunkData>,
}

impl TerrainScene {
    /// Load a complete terrain scene from XTD + optional XTT + asset source.
    ///
    /// This performs all extraction and parallel texture loading in one call:
    /// 1. Decodes XTD mesh, tessellation data, and raw GPU data
    /// 2. If XTT is present: decodes albedo, extracts splat/decal/foliage/road
    ///    data, and loads textures from the asset source in parallel
    pub fn load(
        xtd: &XtdFile,
        xtt: Option<&XttFile>,
        source: Option<&mut AssetSource<StdFileProvider>>,
        tessellation_mode: TessellationMode,
    ) -> Result<Self, String> {
        // -- XTD processing --
        let tessellation_data = xtd.decode_tessellation();
        let raw_xtd_data = Self::extract_raw_xtd(xtd);
        let mesh = Self::build_mesh(xtd, tessellation_mode, tessellation_data.as_ref())?;

        // -- XTT processing --
        let (albedo, terrain_textures, normal_textures, chunk_splat_data,
             decal_textures, decal_instances, chunk_decal_data,
             foliage_sets, foliage_qn_chunks, road_chunks) =
            if let Some(xtt) = xtt {
                Self::process_xtt(xtt, source)
            } else {
                Default::default()
            };

        Ok(Self {
            mesh,
            tessellation_data,
            raw_xtd_data,
            albedo,
            terrain_textures,
            normal_textures,
            chunk_splat_data,
            decal_textures,
            decal_instances,
            chunk_decal_data,
            foliage_sets,
            foliage_qn_chunks,
            road_chunks,
        })
    }


    /// Extract raw XTD data for GPU tessellation.
    fn extract_raw_xtd(xtd: &XtdFile) -> Option<RawXtdData> {
        let raw = xtd.extract_raw_data().ok()?;

        let ao_data = xtd.decode_ao().ok().map(|ao| AoTextureData {
            values: ao.values,
            width: ao.width as u32,
            height: ao.height as u32,
        });

        let alpha_data = xtd.decode_alpha().ok().map(|alpha| AlphaTextureData {
            values: alpha.values,
            width: alpha.width as u32,
            height: alpha.height as u32,
        });

        Some(RawXtdData {
            packed_positions: raw.packed_positions,
            packed_normals: raw.packed_normals,
            num_verts_per_axis: raw.num_verts_per_axis,
            mid: raw.mid,
            range: raw.range,
            tile_scale: raw.tile_scale,
            ao_data,
            alpha_data,
        })
    }

    /// Build a terrain mesh from XTD with the selected tessellation mode.
    fn build_mesh(
        xtd: &XtdFile,
        mode: TessellationMode,
        tess_data: Option<&TessellationData>,
    ) -> Result<TerrainMesh, String> {
        let vertices = xtd
            .decode_vertices()
            .map_err(|e| format!("Failed to decode vertices: {}", e))?;

        let (positions, normals, uvs, indices) = match mode {
            TessellationMode::Cpu => {
                if let Some(tess) = tess_data {
                    log::info!("Applying CPU tessellation...");
                    let start = std::time::Instant::now();
                    let t = vertices.tessellate(tess);
                    log::info!(
                        "Tessellation complete ({:.1}s)",
                        start.elapsed().as_secs_f32()
                    );
                    (t.positions, t.normals, t.uvs, t.indices)
                } else {
                    let idx = vertices.generate_indices();
                    (
                        vertices.positions.clone(),
                        vertices.normals.clone(),
                        vertices.uvs.clone(),
                        idx,
                    )
                }
            }
            TessellationMode::Gpu | TessellationMode::None => {
                let idx = vertices.generate_indices();
                (
                    vertices.positions.clone(),
                    vertices.normals.clone(),
                    vertices.uvs.clone(),
                    idx,
                )
            }
        };

        Ok(TerrainMesh::new(
            positions,
            normals,
            uvs,
            indices,
            xtd.header.world_min,
            xtd.header.world_max,
            xtd.header.tile_scale,
        ))
    }

    /// Process XTT file: extract all data and load textures from ERA.
    #[allow(clippy::type_complexity)]
    fn process_xtt(
        xtt: &XttFile,
        mut source: Option<&mut AssetSource<StdFileProvider>>,
    ) -> (
        Option<AlbedoData>,
        Vec<TerrainTexture>,
        Vec<NormalMapTexture>,
        Vec<ChunkSplatData>,
        Vec<DecalTexture>,
        Vec<DecalInstance>,
        Vec<ChunkDecalData>,
        Vec<FoliageSet>,
        Vec<FoliageQNChunk>,
        Vec<RoadChunkData>,
    ) {
        log::info!(
            "XTT: {} textures, {} linker chunks",
            xtt.header.num_active_textures,
            xtt.linkers.len()
        );

        // Decode albedo atlas
        let albedo = xtt.decode_albedo().ok().map(|atlas| {
            log::info!("Albedo atlas: {}x{}", atlas.width, atlas.height);
            AlbedoData {
                width: atlas.width,
                height: atlas.height,
                pixels: atlas.pixels,
            }
        });

        // Extract splat, decal, foliage, road data
        let chunk_splat_data = loading::extract_chunk_splat_data(xtt);
        let (decal_instances, chunk_decal_data) = loading::extract_decal_data(xtt);
        let foliage_qn_chunks = loading::extract_foliage_chunks(xtt);
        let road_chunks = loading::extract_road_data(xtt);

        // Load textures from ERA (requires asset source)
        let (terrain_textures, normal_textures, decal_textures, foliage_sets) =
            if let Some(src) = source.as_deref_mut() {
                let (tex, nrm) = loading::load_terrain_textures(src, &xtt.active_textures);
                let dec = loading::load_decal_textures(src, &xtt.active_decals);
                let fol = loading::load_foliage_sets(src, &xtt.foliage.sets);
                (tex, nrm, dec, fol)
            } else {
                (Vec::new(), Vec::new(), Vec::new(), Vec::new())
            };

        (
            albedo,
            terrain_textures,
            normal_textures,
            chunk_splat_data,
            decal_textures,
            decal_instances,
            chunk_decal_data,
            foliage_sets,
            foliage_qn_chunks,
            road_chunks,
        )
    }
}