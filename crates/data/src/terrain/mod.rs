//! Terrain data loading for Halo Wars scenarios.
//!
//! This module provides:
//! - [`ScenarioTerrain`] - Load XTD/XTT terrain data
//! - [`types`] - Pure data types for terrain textures, decals, and foliage
//! - [`loading`] - Functions to load textures and extract data from XTT
//!
//! # Example
//!
//! ```ignore
//! use data::terrain::{ScenarioTerrain, loading};
//! use data::assets::AssetSource;
//!
//! // Load terrain for a scenario
//! let terrain = ScenarioTerrain::load("blood_gulch")?;
//!
//! // Create asset source for loading textures
//! let mut source = AssetSource::for_scenario("blood_gulch")?;
//! if let Some(xtt) = &terrain.xtt {
//!     let (textures, normals) = loading::load_terrain_textures(&mut source, &xtt.active_textures);
//!     let decals = loading::load_decal_textures(&mut source, &xtt.active_decals);
//!     let foliage = loading::load_foliage_sets(&mut source, &xtt.foliage.sets);
//! }
//! ```

pub mod loading;
pub mod scenario;
pub mod types;

// Re-export main types at the module level
pub use loading::{
    extract_chunk_splat_data, extract_decal_data, extract_foliage_chunks, load_decal_textures,
    load_foliage_sets, load_terrain_textures,
};
pub use scenario::{ScenarioTerrain, TerrainError};
pub use types::{
    AlbedoData, ChunkDecalData, ChunkSplatData, DecalInstance, DecalTexture, FoliageBladeVertex,
    FoliageQNChunk, FoliageSet, NormalMapTexture, TerrainTexture,
};
