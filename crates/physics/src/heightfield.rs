//! Terrain heightfield samples.
//!
//! Retail builds the terrain body in `BTerrainPhysics_createHeightFieldBody` (`0x14041B500`).
//! The shape reads the XSD sim heights (chunk `0x2222`) and adds a skirt of
//! [`HeightField::SKIRT_VERTICES`] vertices on each side that repeats the edge heights.

mod query;
mod ray;

use glam::Vec3;
use half::f16;
use thiserror::Error;

/// An error in the input of [`HeightField::new`].
#[derive(Clone, Debug, PartialEq, Error)]
pub enum HeightFieldError {
    /// The grid has fewer than two vertices on a side.
    #[error("the heightfield needs at least 2 vertices on a side, not {0}")]
    TooSmall(usize),
    /// The number of heights is not the square of the side length.
    #[error("the heightfield needs {expected} heights, not {actual}")]
    WrongSampleCount {
        /// The square of the side length.
        expected: usize,
        /// The number of heights given.
        actual: usize,
    },
    /// The tile scale is not a positive finite number.
    #[error("the tile scale must be positive and finite, not {0}")]
    BadTileScale(f32),
    /// A height is not finite.
    #[error("the height at vertex ({x}, {z}) is not finite")]
    BadHeight {
        /// The X vertex index.
        x: usize,
        /// The Z vertex index.
        z: usize,
    },
}

/// The terrain heights that the physics world collides with.
///
/// Vertex `(x, z)` of the sim grid sits at world `(x * tile_scale, height, z * tile_scale)`.
/// The shape space adds the skirt: shape vertex `(i, j)` maps to sim vertex
/// `(i - 10, j - 10)`, clamped to the grid. The terrain body sits at
/// [`HeightField::body_position`], so the shape space and the world space line up.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightField {
    vertices_per_side: usize,
    tile_scale: f32,
    /// Row-major heights: index `z * vertices_per_side + x`.
    heights: Vec<f32>,
}

impl HeightField {
    /// The number of skirt vertices on each side (`cNumSkirtVerts`).
    pub const SKIRT_VERTICES: usize = 10;

    /// Makes a heightfield from row-major sim heights (index `z * vertices_per_side + x`).
    ///
    /// The heights must be the decoded half floats of the XSD; this constructor does not round
    /// them.
    ///
    /// # Errors
    ///
    /// Returns an error if the grid is smaller than 2 × 2, if the number of heights does not
    /// match the grid, if the tile scale is not positive and finite or if a height is not finite.
    pub fn new(
        vertices_per_side: usize,
        tile_scale: f32,
        heights: Vec<f32>,
    ) -> Result<Self, HeightFieldError> {
        if vertices_per_side < 2 {
            return Err(HeightFieldError::TooSmall(vertices_per_side));
        }
        let expected = vertices_per_side * vertices_per_side;
        if heights.len() != expected {
            return Err(HeightFieldError::WrongSampleCount {
                expected,
                actual: heights.len(),
            });
        }
        if !(tile_scale.is_finite() && tile_scale > 0.0) {
            return Err(HeightFieldError::BadTileScale(tile_scale));
        }
        if let Some(index) = heights.iter().position(|height| !height.is_finite()) {
            return Err(HeightFieldError::BadHeight {
                x: index % vertices_per_side,
                z: index / vertices_per_side,
            });
        }
        Ok(Self {
            vertices_per_side,
            tile_scale,
            heights,
        })
    }

    /// Returns the number of sim vertices on each side of the grid.
    #[must_use]
    pub fn vertices_per_side(&self) -> usize {
        self.vertices_per_side
    }

    /// Returns the distance between two neighbouring vertices.
    #[must_use]
    pub fn tile_scale(&self) -> f32 {
        self.tile_scale
    }

    /// Returns the number of shape vertices on each side, including both skirts
    /// (`m_xRes` and `m_zRes`).
    #[must_use]
    pub fn resolution(&self) -> usize {
        self.vertices_per_side + 2 * Self::SKIRT_VERTICES
    }

    /// Returns the world position of the terrain body: `(-10 * s, 0, -10 * s)`.
    #[must_use]
    pub fn body_position(&self) -> Vec3 {
        let offset = SKIRT_OFFSET * self.tile_scale;
        Vec3::new(-offset, 0.0, -offset)
    }

    /// Returns the height of shape vertex `(x, z)` (`getHeightAt`).
    ///
    /// The indices include the skirt. Indices outside the sim grid clamp to its edge.
    #[must_use]
    pub fn height_at(&self, x: usize, z: usize) -> f32 {
        let last = self.vertices_per_side - 1;
        let sim_x = x.saturating_sub(Self::SKIRT_VERTICES).min(last);
        let sim_z = z.saturating_sub(Self::SKIRT_VERTICES).min(last);
        self.heights[sim_z * self.vertices_per_side + sim_x]
    }

    /// Returns the height of sim vertex `(x, z)`, or `None` outside the grid.
    #[must_use]
    pub fn sim_height(&self, x: usize, z: usize) -> Option<f32> {
        (x < self.vertices_per_side && z < self.vertices_per_side)
            .then(|| self.heights[z * self.vertices_per_side + x])
    }

    /// Writes the height of sim vertex `(x, z)`, rounded to a half float as the XSD stores it
    /// (`BTerrainSimRep_flattenInstant`). Returns `false` and changes nothing outside the grid
    /// or for a height that is not finite.
    pub fn set_sim_height(&mut self, x: usize, z: usize, height: f32) -> bool {
        if x >= self.vertices_per_side || z >= self.vertices_per_side || !height.is_finite() {
            return false;
        }
        self.heights[z * self.vertices_per_side + x] = f16::from_f32(height).to_f32();
        true
    }
}

/// [`HeightField::SKIRT_VERTICES`] as a float, for the body offset.
const SKIRT_OFFSET: f32 = 10.0;

#[cfg(test)]
mod tests {
    use super::{HeightField, HeightFieldError};

    fn ramp() -> HeightField {
        // Height = x + 10 * z on a 3 × 3 grid.
        let heights = (0..9u8)
            .map(|index| f32::from(index % 3) + 10.0 * f32::from(index / 3))
            .collect();
        HeightField::new(3, 4.0, heights).expect("valid grid")
    }

    #[test]
    fn the_skirt_repeats_the_edge_heights() {
        let field = ramp();
        assert_eq!(field.resolution(), 23);
        assert!((field.height_at(0, 0) - 0.0).abs() < f32::EPSILON);
        assert!((field.height_at(11, 10) - 1.0).abs() < f32::EPSILON);
        assert!((field.height_at(22, 22) - 22.0).abs() < f32::EPSILON);
        assert!((field.height_at(22, 0) - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_body_offset_aligns_the_skirt_with_the_world() {
        let position = ramp().body_position();
        assert!((position.x + 40.0).abs() < f32::EPSILON);
        assert!((position.z + 40.0).abs() < f32::EPSILON);
    }

    #[test]
    fn written_heights_round_to_half_floats() {
        let mut field = ramp();
        assert!(field.set_sim_height(1, 1, 10.123_456));
        let stored = field.sim_height(1, 1).expect("inside the grid");
        assert!((stored - 10.125).abs() < f32::EPSILON, "{stored}");
        assert!(!field.set_sim_height(3, 0, 1.0));
    }

    #[test]
    fn bad_input_is_rejected() {
        assert_eq!(
            HeightField::new(2, 1.0, vec![0.0; 3]),
            Err(HeightFieldError::WrongSampleCount {
                expected: 4,
                actual: 3
            })
        );
        assert_eq!(
            HeightField::new(2, 0.0, vec![0.0; 4]),
            Err(HeightFieldError::BadTileScale(0.0))
        );
    }
}
