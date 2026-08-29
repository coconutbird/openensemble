//! Terrain rendering module for Halo Wars terrain.
//!
//! This module provides reusable components for rendering terrain from XTD/XTT files:
//! - Camera: Fly camera with WASD + mouse controls
//! - Compositing: GPU-based texture compositing for pre-baked chunk textures
//! - Mesh: Terrain mesh generation and data structures
//! - Texture: Mipmap generation and texture utilities
//! - Shaders: WGSL shader code for terrain rendering
//! - Uniforms: GPU uniform structs (`TerrainParams`, `GpuTessParams`)
//! - Types: Data types for terrain textures, decals, and foliage
//! - Loading: Functions to load textures and extract data from XTT

mod callouts;
mod camera;
mod compositing;
pub mod loading;
mod mesh;
mod patch;
mod scene;
mod shaders;
mod texture;
pub mod types;
mod uniforms;

pub use callouts::{ProjectedHintCallout, project_hint_callouts};
pub use camera::{Camera, SimulationCameraAdapter};
pub use compositing::{
    CompositeBindings, CompositeParams, CompositingConfig, CompositorResources, LodConfig,
};
pub use loading::{
    RoadTextures, extract_chunk_splat_data, extract_decal_data, extract_foliage_chunks,
    extract_road_data, load_decal_textures, load_foliage_sets, load_road_textures,
    load_terrain_textures,
};
pub use mesh::TerrainMesh;
pub use patch::{
    TerrainPatchError, TerrainPatchImage, TerrainPatchInstance, TerrainPatchMaterial,
    TerrainPatchRenderer, TerrainPatchRendererDescriptor, TerrainPatchShading,
    TerrainPatchWorldBindings,
};
pub use scene::TerrainScene;
pub use shaders::{
    COMPOSITE_SHADER, FOLIAGE_SHADER, GPU_TESS_SHADER, HEIGHTFIELD_SHADER, ROADS_SHADER,
    SHADOW_DEPTH_SHADER,
};
pub use texture::{generate_mipmaps, mip_dimensions, mip_level_count};
pub use types::{
    AlbedoData, AlphaTextureData, AoTextureData, ChunkDecalData, ChunkSplatData, DecalInstance,
    DecalTexture, FoliageBladeVertex, FoliageQNChunk, FoliageSet, NormalMapTexture, RawXtdData,
    RoadChunkData, SpecularMapTexture, TerrainHeightfield, TerrainTessellationData, TerrainTexture,
};
pub use uniforms::{
    CameraUniform, GpuTessParams, LightingParams, NORMALIZED_TERRAIN_Y_OFFSET, TerrainParams,
};
