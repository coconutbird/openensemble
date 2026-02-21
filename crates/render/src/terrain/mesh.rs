//! Terrain mesh data structures.

use glam::Vec3;

/// Terrain mesh data for rendering.
///
/// Contains vertex data (positions, normals, UVs) and indices for rendering.
/// Can be created from XTD data with different tessellation levels.
#[derive(Clone)]
pub struct TerrainMesh {
    /// Vertex positions in world space.
    pub positions: Vec<[f32; 3]>,
    /// Vertex normals.
    pub normals: Vec<[f32; 3]>,
    /// Texture coordinates.
    pub uvs: Vec<[f32; 2]>,
    /// Triangle indices.
    pub indices: Vec<u32>,
    /// World space minimum bounds.
    pub world_min: [f32; 3],
    /// World space maximum bounds.
    pub world_max: [f32; 3],
    /// Tile scale (world units per tile).
    pub tile_scale: f32,
}

impl TerrainMesh {
    /// Create a new terrain mesh from raw vertex data.
    pub fn new(
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
        world_min: [f32; 3],
        world_max: [f32; 3],
        tile_scale: f32,
    ) -> Self {
        Self {
            positions,
            normals,
            uvs,
            indices,
            world_min,
            world_max,
            tile_scale,
        }
    }

    /// Get the center of the terrain in world space.
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.world_min[0] + self.world_max[0]) / 2.0,
            (self.world_min[1] + self.world_max[1]) / 2.0,
            (self.world_min[2] + self.world_max[2]) / 2.0,
        )
    }

    /// Get the size of the terrain in world space.
    pub fn size(&self) -> Vec3 {
        Vec3::new(
            self.world_max[0] - self.world_min[0],
            self.world_max[1] - self.world_min[1],
            self.world_max[2] - self.world_min[2],
        )
    }

    /// Get the number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Get the number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Tessellation mode for terrain rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TessellationMode {
    /// No tessellation - use raw XTD mesh.
    None,
    /// CPU tessellation - subdivide mesh on CPU (slow but accurate).
    Cpu,
    /// GPU tessellation - use instanced patches with vertex shader displacement (fast).
    #[default]
    Gpu,
}

impl TessellationMode {
    /// Cycle to the next tessellation mode.
    pub fn next(self) -> Self {
        match self {
            TessellationMode::None => TessellationMode::Gpu,
            TessellationMode::Gpu => TessellationMode::Cpu,
            TessellationMode::Cpu => TessellationMode::None,
        }
    }

    /// Get the display name for this mode.
    pub fn name(self) -> &'static str {
        match self {
            TessellationMode::None => "None",
            TessellationMode::Gpu => "GPU",
            TessellationMode::Cpu => "CPU",
        }
    }
}
