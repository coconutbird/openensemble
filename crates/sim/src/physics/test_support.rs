//! Shared fixtures of the physics tests: a world with a flat terrain body and units with box
//! bodies.

use std::sync::Arc;

use glam::Vec3;
use pipeline::database::hw1::Vector3;
use pipeline::database::hw1::physics::{
    MotionType as InfoMotionType, PhysicsBlueprint, PhysicsInfo, PhysicsInfoFlags,
};

use super::{PhysicsBlueprintBody, PhysicsTemplate};
use crate::entity_id::EntityId;
use crate::world::{World, flat_test_terrain};

/// The height of the flat test terrain.
pub(crate) const TERRAIN_HEIGHT: f32 = 2.0;

/// A world with a flat terrain body at [`TERRAIN_HEIGHT`]: the 8 by 8 test grid with tile scale
/// 4, plus the 10-vertex heightfield skirt around it.
pub(crate) fn world_with_flat_terrain() -> World {
    let mut world = World::new();
    world.init_players(2);
    world.configure_terrain_simulation(flat_test_terrain(4.0, TERRAIN_HEIGHT));
    world
}

/// A blueprint of `mass` with friction 0.5 and no restitution, so test bodies come to rest.
fn blueprint(mass: f32) -> PhysicsBlueprint {
    PhysicsBlueprint {
        mass,
        friction: 0.5,
        restitution: 0.0,
        ..PhysicsBlueprint::default()
    }
}

/// A dynamic info with one box body of `half_extents` and `mass`, with its body origin at
/// `center_offset` above the unit.
pub(crate) fn box_template(
    name: &str,
    half_extents: Vec3,
    mass: f32,
    center_offset: Vec3,
) -> Arc<PhysicsTemplate> {
    Arc::new(PhysicsTemplate::simple(
        name,
        PhysicsBlueprintBody::with_box(name, &blueprint(mass), half_extents),
        center_offset,
    ))
}

/// A `MotionType fixed` info with one box body of `half_extents` (a building).
pub(crate) fn fixed_box_template(name: &str, half_extents: Vec3) -> Arc<PhysicsTemplate> {
    let info = PhysicsInfo {
        blueprints: vec![name.to_owned()],
        motion_type: InfoMotionType::Fixed,
        ..PhysicsInfo::default()
    };
    let body = PhysicsBlueprintBody::with_box(name, &blueprint(1.0), half_extents);
    Arc::new(PhysicsTemplate::new(name, info, vec![body]).expect("one body"))
}

/// A clamshell info: upper, lower and pelvis boxes of `half_extents`, the upper body 1 unit
/// above the pelvis and the lower body 1 unit below it.
pub(crate) fn clamshell_template(
    name: &str,
    half_extents: Vec3,
    center_offset: Vec3,
) -> Arc<PhysicsTemplate> {
    let parts = ["upper", "lower", "pelvis"];
    let bodies = parts
        .iter()
        .map(|part| PhysicsBlueprintBody::with_box(part, &blueprint(10.0), half_extents))
        .collect();
    let mut flags = PhysicsInfoFlags::default();
    flags.set(PhysicsInfoFlags::CLAMSHELL, true);
    let info = PhysicsInfo {
        blueprints: parts.iter().map(|part| (*part).to_owned()).collect(),
        center_offset: Vector3 {
            x: center_offset.x,
            y: center_offset.y,
            z: center_offset.z,
        },
        upper_height_offset: 1.0,
        lower_height_offset: -1.0,
        flags,
        ..PhysicsInfo::default()
    };
    Arc::new(PhysicsTemplate::new(name, info, bodies).expect("three clamshell bodies"))
}

/// Spawns a unit of player 1 at `position` and gives it the body of `template`.
pub(crate) fn spawn_body_unit(
    world: &mut World,
    position: Vec3,
    template: &PhysicsTemplate,
) -> EntityId {
    let unit_id = world.create_unit_at(1, position);
    assert!(world.create_physics_object(unit_id, template, false));
    unit_id
}
