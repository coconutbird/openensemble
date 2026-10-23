//! The terrain body (`BTerrainPhysics_createHeightFieldBody`, `0x14041B500`, 2008
//! `terrainphysicsheightfield.cpp:50-109`).

use std::sync::Arc;

use pipeline::xsd;

use ::physics::{BodyDesc, HeightField, HeightFieldError, Layer, MotionType, Shape};

use super::state::SimPhysics;

/// The friction of the terrain body.
const TERRAIN_FRICTION: f32 = 1.9;
/// The restitution of the terrain body.
const TERRAIN_RESTITUTION: f32 = 1.0;

/// Makes the heightfield of the XSD sim heights (chunk `0x2222`): the decoded half floats in
/// row-major order.
///
/// # Errors
///
/// Returns an error if the grid is too small or a height is not finite.
pub(crate) fn terrain_height_field(
    terrain: &xsd::Terrain,
) -> Result<HeightField, HeightFieldError> {
    let side = terrain.height_axis;
    let mut heights = Vec::with_capacity(side * side);
    for z in 0..side {
        for x in 0..side {
            heights.push(terrain.height_sample(x, z).unwrap_or(f32::NAN));
        }
    }
    HeightField::new(side, terrain.height_tile_scale, heights)
}

impl SimPhysics {
    /// Replaces the terrain body: a fixed heightfield body at [`HeightField::body_position`] with
    /// friction 1.9, restitution 1.0 and the Terrain layer in its own system group.
    pub(crate) fn set_terrain(&mut self, heights: &Arc<HeightField>) {
        if let Some(old) = self.terrain_body() {
            self.world_mut().remove_body(old);
        }
        let filter = self.create_collision_filter_info(Layer::Terrain);
        let mut desc = BodyDesc::new(
            Arc::new(Shape::HeightField(Arc::clone(heights))),
            MotionType::Fixed,
            heights.body_position(),
        );
        desc.friction = TERRAIN_FRICTION;
        desc.restitution = TERRAIN_RESTITUTION;
        desc.filter = filter;
        let body = self.world_mut().add_body(desc);
        self.set_terrain_body(Some(body));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Vec3;

    use super::terrain_height_field;
    use crate::physics::SimPhysics;
    use crate::world::flat_test_terrain;

    #[test]
    fn the_terrain_body_is_fixed_at_the_skirt_offset() {
        let field = Arc::new(terrain_height_field(&flat_test_terrain(4.0, 12.0)).unwrap());
        let mut physics = SimPhysics::new();
        physics.set_terrain(&field);
        let body = physics.terrain_body().expect("a terrain body");
        let world = physics.world();
        assert_eq!(world.position(body), Some(Vec3::new(-40.0, 0.0, -40.0)));
        assert_eq!(world.motion_type(body), Some(::physics::MotionType::Fixed));
        assert_eq!(
            world.filter_info(body).map(::physics::FilterInfo::layer),
            Some(::physics::Layer::Terrain as u8)
        );
        assert_eq!(field.sim_height(3, 5), Some(12.0));
    }
}
