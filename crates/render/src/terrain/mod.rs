//! Terrain rendering module for Halo Wars terrain.
//!
//! This module provides reusable components for rendering terrain from XTD/XTT files:
//! - Camera: Fly camera with WASD + mouse controls
//! - Compositing: GPU-based texture compositing for pre-baked chunk textures
//! - Mesh: Terrain mesh generation and data structures
//! - Texture: Mipmap generation and texture utilities
//! - Shaders: WGSL shader code for terrain rendering
//! - Uniforms: GPU uniform structs (TerrainParams, GpuTessParams)
//! - Types: Data types for terrain textures, decals, and foliage
//! - Loading: Functions to load textures and extract data from XTT

mod camera;
mod compositing;
pub mod loading;
mod mesh;
mod shaders;
mod texture;
pub mod types;
mod uniforms;

pub use camera::Camera;
pub use compositing::{CompositeParams, CompositingConfig, CompositorResources, LodConfig};
pub use loading::{
    extract_chunk_splat_data, extract_decal_data, extract_foliage_chunks, load_decal_textures,
    load_foliage_sets, load_terrain_textures,
};
pub use mesh::{TerrainMesh, TessellationMode};
pub use shaders::{COMPOSITE_SHADER, GPU_TESS_SHADER, TERRAIN_SHADER};
pub use texture::{generate_mipmaps, mip_dimensions, mip_level_count};
pub use types::{
    AlbedoData, ChunkDecalData, ChunkSplatData, DecalInstance, DecalTexture, FoliageBladeVertex,
    FoliageQNChunk, FoliageSet, NormalMapTexture, TerrainTexture,
};
pub use uniforms::{CameraUniform, GpuTessParams, LightingParams, TerrainParams};
