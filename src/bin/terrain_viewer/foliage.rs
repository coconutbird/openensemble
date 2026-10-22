//! Foliage rendering module for terrain viewer.
//!
//! This module handles rendering of foliage (grass, bushes) on the terrain.
//! Based on the original Halo Wars terrainFoliage.fx shader.
//!
//! ## How foliage works in Halo Wars:
//!
//! 1. **Foliage Sets**: Each set defines a type of vegetation with:
//!    - Blade geometry (position/normal/UV stored as textures)
//!    - Albedo, normal, specular, and opacity textures
//!    - Configurable blade count and vertex count per blade
//!
//! 2. **Foliage QN Chunks**: Per quad-node foliage placement data:
//!    - Index buffers encoding blade positions within the 64x64 grid
//!    - Set indices for which foliage types to render
//!    - Polygon counts for each set
//!
//! 3. **Rendering**: Instanced rendering where each blade:
//!    - Gets random rotation and scale based on its index
//!    - Gets height from terrain heightmap
//!    - Uses alpha blending with distance fade

mod rendering;
mod resources;

pub(crate) use resources::parse_foliage_index_buffer;
pub use resources::{FoliageResources, FoliageWorldBindings};

/// Configuration for foliage rendering.
#[derive(Clone, Debug)]
pub struct FoliageConfig {
    /// Maximum distance at which foliage is rendered.
    pub max_render_distance: f32,
    /// Distance at which foliage starts fading out.
    pub fade_start_distance: f32,
    /// Global foliage density multiplier (0.0-1.0).
    pub _density: f32,
    /// Whether foliage rendering is enabled.
    pub enabled: bool,
}

impl Default for FoliageConfig {
    fn default() -> Self {
        Self {
            max_render_distance: 500.0,
            fade_start_distance: 400.0,
            _density: 1.0,
            enabled: true,
        }
    }
}
