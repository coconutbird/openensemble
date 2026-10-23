//! The obstruction contacts of the collision attacks.
//!
//! Ram damage does not come from Havok contacts. `BUnitActionCollisionAttack::handleCollisions`
//! (`0x1400F1FB0`, 2008 `UnitActionCollisionAttack.cpp:520-780`) tests the sim obstructions
//! (`checkForCollisions`). This check finds each pair of overlapping obstruction boxes where at
//! least one unit has a dynamic body. It does not move a unit.

use std::collections::BTreeSet;

use glam::Vec3;

use crate::entities::Unit;
use crate::entity::{Entity, EntityManager};
use crate::entity_id::EntityId;

/// One pair of overlapping obstructions and the relative speed along the contact axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UnitCollisionContact {
    pub first: EntityId,
    pub second: EntityId,
    pub projected_velocity: f32,
}

#[derive(Debug, Clone, Copy)]
struct Obstruction {
    id: EntityId,
    squad_id: Option<EntityId>,
    center: Vec3,
    half_extents: Vec3,
    velocity: Vec3,
    physics_driven: bool,
}

/// Finds the overlapping obstructions in entity id order. Units of one squad, dead or
/// garrisoned units and the units of `excluded_squads` take no part.
pub(crate) fn obstruction_contacts(
    units: &EntityManager<Unit>,
    excluded_squads: &BTreeSet<EntityId>,
) -> Vec<UnitCollisionContact> {
    let obstructions: Vec<Obstruction> = units
        .iter()
        .filter_map(|(id, unit)| {
            let half_extents = unit.obstruction_half_extents.abs();
            let excluded = !unit.is_alive()
                || unit.is_garrisoned()
                || unit
                    .squad_id
                    .is_some_and(|squad_id| excluded_squads.contains(&squad_id))
                || half_extents.x <= 0.0
                || half_extents.z <= 0.0;
            (!excluded).then(|| Obstruction {
                id,
                squad_id: unit.squad_id,
                center: unit.base.position + Vec3::Y * half_extents.y,
                half_extents,
                velocity: unit.base.velocity,
                physics_driven: unit.is_physics_driven(),
            })
        })
        .collect();
    let mut contacts = Vec::new();
    for (index, first) in obstructions.iter().enumerate() {
        for second in &obstructions[index + 1..] {
            if (first.squad_id.is_some() && first.squad_id == second.squad_id)
                || !(first.physics_driven || second.physics_driven)
            {
                continue;
            }
            if let Some(normal) = overlap_axis(first, second) {
                contacts.push(UnitCollisionContact {
                    first: first.id,
                    second: second.id,
                    projected_velocity: (second.velocity - first.velocity).dot(normal).abs(),
                });
            }
        }
    }
    contacts
}

/// The horizontal axis of the smaller overlap, from the first box toward the second, or `None`
/// if the boxes do not overlap.
fn overlap_axis(first: &Obstruction, second: &Obstruction) -> Option<Vec3> {
    let delta = second.center - first.center;
    let combined = first.half_extents + second.half_extents;
    if delta.y.abs() > combined.y {
        return None;
    }
    let overlap_x = combined.x - delta.x.abs();
    let overlap_z = combined.z - delta.z.abs();
    if overlap_x <= 0.0 || overlap_z <= 0.0 {
        return None;
    }
    let sign = |value: f32| if value < 0.0 { -1.0 } else { 1.0 };
    Some(if overlap_x <= overlap_z {
        Vec3::X * sign(delta.x)
    } else {
        Vec3::Z * sign(delta.z)
    })
}
