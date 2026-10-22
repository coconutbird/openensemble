//! Application of retail `DesignObjects/Lines` path geometry to the world.

use super::scenario_object_position_to_world;
use crate::world::World;
use glam::Vec3;
use pipeline::hw1::scenario::DesignLine;

/// Configure the world design lines from the decoded SCN lines.
///
/// The points change from SCN axes to world axes. The world keeps the last
/// line for each duplicate ID.
pub(super) fn load_design_lines(world: &mut World, lines: &[DesignLine]) {
    world.configure_design_lines(lines.iter().map(|line| {
        let points = line
            .points
            .iter()
            .map(|&point| Vec3::from_array(scenario_object_position_to_world(point)))
            .collect();
        (line.id, points)
    }));
}

#[cfg(test)]
mod tests;
