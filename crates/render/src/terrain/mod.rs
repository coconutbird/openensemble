//! Terrain rendering module for Halo Wars terrain.
//!
//! This module provides reusable components for rendering terrain from XTD/XTT files:
//! - Camera: Fly camera with WASD + mouse controls
//! - Mesh: Terrain mesh generation and data structures  
//! - Texture: Mipmap generation and texture utilities
//! - Shaders: WGSL shader code for terrain rendering
//! - Uniforms: GPU uniform structs (TerrainParams, GpuTessParams)

mod camera;
mod mesh;
mod shaders;
mod texture;
mod uniforms;

pub use camera::Camera;
pub use mesh::{TerrainMesh, TessellationMode};
pub use shaders::{GPU_TESS_SHADER, TERRAIN_SHADER};
pub use texture::{generate_mipmaps, mip_dimensions, mip_level_count};
pub use uniforms::{CameraUniform, GpuTessParams, TerrainParams};
