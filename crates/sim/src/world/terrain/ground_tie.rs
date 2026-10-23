//! Terrain contact of ground units (`BEntity_tieToGround`).
//!
//! Retail ties each entity with `mFlagTiesToGround` to the terrain after it moves
//! (`BEntity_tieToGround` @ `0x140202EB0`). A unit with `mFlagPhysicsControl` skips the tie: its
//! Havok body collides with the terrain heightfield of the physics world.
//! [`World::tie_units_to_ground`] puts each other ground unit on the terrain. A unit with
//! `OrientUnitWithGround` also takes its up axis from the terrain normal.

use super::ground_sample::{filter_up, sample_ground};
use crate::EntityId;
use crate::entities::Unit;
use crate::entity::Entity;
use crate::world::World;
use glam::Vec3;
use std::collections::BTreeSet;

impl World {
    /// Put each ground unit that retail ties to the ground on the terrain.
    ///
    /// When `orient` is set, a unit with `OrientUnitWithGround` also takes its up axis from the
    /// terrain (`BEntity_orientWithGround` @ `0x1402032A0`). `FilterOrient` blends the new axis
    /// with the previous one (`BEntity_filterOrientation` @ `0x1402035F0`). Every other tied
    /// unit stays upright.
    pub(in crate::world) fn tie_units_to_ground(&mut self, orient: bool) {
        let Some(terrain) = self.terrain_simulation.as_ref() else {
            return;
        };
        let moving_squads = self
            .squads
            .iter()
            .filter_map(|(id, squad)| (squad.is_being_pulled() || squad.is_jumping()).then_some(id))
            .collect::<BTreeSet<EntityId>>();
        for (_, unit) in self.units.iter_mut() {
            let squad_moves_unit = unit
                .squad_id
                .is_some_and(|squad_id| moving_squads.contains(&squad_id));
            if squad_moves_unit || !ties_to_terrain(unit) {
                continue;
            }
            let position = unit.base.position;
            let height_at = |point| terrain.height(point, true);
            if unit.orients_with_ground() {
                let Some(sample) = sample_ground(position, unit.base.forward, height_at) else {
                    continue;
                };
                unit.base.position.y = sample.height;
                if orient {
                    unit.base.up = if unit.filters_ground_orientation() {
                        filter_up(unit.base.up, sample.normal, unit.base.forward)
                    } else {
                        sample.normal
                    };
                }
            } else {
                let Some(height) = height_at(position) else {
                    continue;
                };
                unit.base.position.y = height;
                unit.base.up = Vec3::Y;
            }
            unit.base.velocity.y = 0.0;
        }
    }
}

/// Return whether the world ties this unit to the terrain.
///
/// Physics-driven units, aircraft, buildings and units that another system carries are not
/// tied. Buildings take the terrain height once, when the world places them.
fn ties_to_terrain(unit: &Unit) -> bool {
    unit.is_alive()
        && unit.ties_to_ground()
        && !unit.flying
        && !unit.uses_move_air()
        && !unit.is_crashing()
        && !unit.is_physics_driven()
        && !unit.is_building()
        && !unit.is_socket()
        && !unit.is_garrisoned()
        && !unit.is_jumping()
        && !unit.is_thrown()
}

#[cfg(test)]
mod tests;
