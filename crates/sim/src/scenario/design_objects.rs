//! Application of retail `DesignObjects` spheres and lines to the world.

use crate::world::{DesignSphere, World};
use glam::Vec3;
use pipeline::hw1::scenario::DesignObjects;

/// Configure the world design objects from the decoded SCN design objects.
///
/// The positions stay in the authored SCN frame, which is the world frame. The spheres keep
/// their document order, like the engine sphere array. The world keeps the last line for
/// each duplicate ID.
pub(super) fn load_design_objects(world: &mut World, objects: &DesignObjects) {
    let spheres = objects
        .spheres
        .iter()
        .map(|sphere| DesignSphere {
            type_name: sphere.data.type_name.clone(),
            center: Vec3::from_array(sphere.position),
            radius: sphere.radius,
        })
        .collect();
    let lines = objects.lines.iter().map(|line| {
        let points = line.points.iter().copied().map(Vec3::from_array).collect();
        (line.id, points)
    });
    world.configure_design_objects(spheres, lines);
}

#[cfg(test)]
mod tests;
