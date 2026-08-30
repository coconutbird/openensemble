//! Source-backed placement of a squad leaving a trainer.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use glam::{Quat, Vec3};
use num_traits::ToPrimitive;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use std::collections::BTreeSet;

const BIRTH_CLEARANCE: f32 = 0.015;
const MINIMUM_SEARCH_RADIUS: f32 = 0.2;
const SEARCH_SPACING_SCALE: f32 = 1.1;
const SEARCH_GROW_ATTEMPTS: usize = 4;

#[derive(Debug, Clone, Copy)]
pub(super) struct SquadBirthPlacement {
    pub(super) position: Vec3,
    pub(super) forward: Vec3,
    pub(super) obstruction_radius: f32,
    pub(super) preferred: bool,
}

pub(super) fn trained_squad_placement(
    world: &World,
    building_id: EntityId,
    squad_id: EntityId,
    database: &Database,
) -> Option<SquadBirthPlacement> {
    let building = world.get_building(building_id)?;
    let building_proto = find_object(database, &building.proto_object_name);
    let squad = world.get_squad(squad_id)?;
    let leader = squad
        .unit_ids
        .iter()
        .find_map(|unit_id| world.get_unit(*unit_id))?;
    let leader_proto = find_object(database, &leader.proto_object_name);
    let proto_squad = effective_squad_prototype(world, squad_id, database);
    let obstruction_radius = squad_birth_radius(proto_squad, leader_proto, database);
    let mut forward = planar_forward(building.base.forward);
    let exit_direction = building_proto
        .and_then(|prototype| prototype.exit_from_direction)
        .unwrap_or_default();
    let exit_degrees = 90.0 * exit_direction.to_f32().unwrap_or_default();
    forward = planar_forward(Quat::from_rotation_y(exit_degrees.to_radians()) * forward);
    let birth_on_top = building_proto.is_some_and(|prototype| has_type(prototype, "BirthOnTop"));
    let offset = if birth_on_top {
        0.0
    } else {
        building.obstruction_radius()
            + obstruction_radius
            + BIRTH_CLEARANCE
            + world.terrain_simulation_tile_scale()
    };
    let initial = building.base.position + forward * offset;
    let excluded = squad.unit_ids.iter().copied().collect::<BTreeSet<_>>();
    if let Some(position) = clear_birth_position(
        world,
        initial,
        obstruction_radius,
        building_id,
        &excluded,
        leader.flying,
    ) {
        return Some(SquadBirthPlacement {
            position,
            forward,
            obstruction_radius,
            preferred: true,
        });
    }
    let right = Vec3::Y.cross(forward).normalize_or(Vec3::X);
    let desired = initial + right * building.obstruction_radius();
    let position = search_around_trainer(
        world,
        &BirthSearch {
            center: building.base.position,
            forward,
            right,
            trainer_radius: building.obstruction_radius(),
            squad_radius: obstruction_radius,
            desired,
            building_id,
            excluded: &excluded,
            flying: leader.flying,
        },
    )
    .unwrap_or(initial);
    Some(SquadBirthPlacement {
        position,
        forward,
        obstruction_radius,
        preferred: false,
    })
}

pub(super) fn effective_squad_prototype<'a>(
    world: &World,
    squad_id: EntityId,
    database: &'a Database,
) -> Option<&'a ProtoSquad> {
    let squad = world.get_squad(squad_id)?;
    let effective_name =
        world
            .get_player(squad.base.player_id)
            .map_or(squad.proto_squad_name.as_str(), |player| {
                player
                    .technologies
                    .resolved_squad_prototype(&squad.proto_squad_name)
            });
    database
        .squads
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(effective_name))
}

fn squad_birth_radius(
    squad: Option<&ProtoSquad>,
    leader: Option<&ProtoObject>,
    database: &Database,
) -> f32 {
    let mut radius = leader.map_or(0.0, object_radius);
    let Some(squad) = squad else {
        return radius;
    };
    let size = squad.units.as_ref().map_or(0, |units| {
        units.entries.iter().map(|entry| entry.count.max(0)).sum()
    });
    if size == 2 {
        radius *= 2.0;
    } else if size > 2 {
        let flood = squad.units.as_ref().is_some_and(|units| {
            units.entries.iter().any(|entry| {
                find_object(database, entry.proto_object.trim())
                    .is_some_and(|prototype| has_type(prototype, "Flood"))
            })
        });
        let factor = if flood { size } else { (size + 1) / 2 };
        radius *= factor.to_f32().unwrap_or(f32::MAX);
    }
    radius
}

struct BirthSearch<'a> {
    center: Vec3,
    forward: Vec3,
    right: Vec3,
    trainer_radius: f32,
    squad_radius: f32,
    desired: Vec3,
    building_id: EntityId,
    excluded: &'a BTreeSet<EntityId>,
    flying: bool,
}

fn search_around_trainer(world: &World, search: &BirthSearch<'_>) -> Option<Vec3> {
    let search_radius = search.squad_radius.max(MINIMUM_SEARCH_RADIUS);
    let mut trainer_radius = search.trainer_radius;
    for attempt in 0..=SEARCH_GROW_ATTEMPTS {
        let candidates = perimeter_candidates(
            search.center,
            search.forward,
            search.right,
            trainer_radius,
            search_radius,
        );
        if let Some(position) = candidates
            .into_iter()
            .filter_map(|candidate| {
                clear_birth_position(
                    world,
                    candidate,
                    search_radius,
                    search.building_id,
                    search.excluded,
                    search.flying,
                )
            })
            .min_by(|left, right| {
                xz_distance_squared(*left, search.desired)
                    .total_cmp(&xz_distance_squared(*right, search.desired))
            })
        {
            return Some(position);
        }
        let growth_factor = (attempt + 1).to_f32().unwrap_or(f32::MAX);
        let grow = search.squad_radius * (1.0 + growth_factor);
        trainer_radius += grow;
    }
    None
}

fn perimeter_candidates(
    center: Vec3,
    forward: Vec3,
    right: Vec3,
    trainer_radius: f32,
    squad_radius: f32,
) -> Vec<Vec3> {
    let trainer_size = trainer_radius * 2.0;
    let delta = squad_radius * 2.0 * SEARCH_SPACING_SCALE;
    let reset = (trainer_radius + squad_radius) * SEARCH_SPACING_SCALE;
    let mut candidates = Vec::new();
    add_edge(
        &mut candidates,
        center + forward * reset - right * reset,
        right * delta,
        trainer_size,
        delta,
    );
    add_edge(
        &mut candidates,
        center + forward * reset + right * reset,
        -forward * delta,
        trainer_size,
        delta,
    );
    add_edge(
        &mut candidates,
        center - forward * reset + right * reset,
        -right * delta,
        trainer_size,
        delta,
    );
    add_edge(
        &mut candidates,
        center - forward * reset - right * reset,
        forward * delta,
        trainer_size,
        delta,
    );
    candidates
}

fn add_edge(
    candidates: &mut Vec<Vec3>,
    start: Vec3,
    step: Vec3,
    distance: f32,
    step_distance: f32,
) {
    candidates.push(start);
    let mut traveled = 0.0;
    let mut position = start;
    while traveled < distance {
        position += step;
        traveled += step_distance;
        candidates.push(position);
    }
}

fn clear_birth_position(
    world: &World,
    mut candidate: Vec3,
    radius: f32,
    building_id: EntityId,
    excluded: &BTreeSet<EntityId>,
    flying: bool,
) -> Option<Vec3> {
    if !candidate.is_finite() || world.is_outside_playable_bounds(candidate, true) {
        return None;
    }
    if !flying && world.has_terrain_simulation() {
        candidate.y = world.terrain_height(candidate, false)?;
    }
    let obstructed = world.units.iter().any(|(unit_id, unit)| {
        unit_id != building_id
            && !excluded.contains(&unit_id)
            && unit.is_alive()
            && !unit.is_garrisoned()
            && circle_overlaps_unit(
                candidate,
                radius,
                unit.base.position,
                unit.obstruction_half_extents,
            )
    });
    (!obstructed).then_some(candidate)
}

fn circle_overlaps_unit(center: Vec3, radius: f32, unit: Vec3, half_extents: Vec3) -> bool {
    let extents = half_extents.abs();
    let closest_x = center.x.clamp(unit.x - extents.x, unit.x + extents.x);
    let closest_z = center.z.clamp(unit.z - extents.z, unit.z + extents.z);
    let dx = center.x - closest_x;
    let dz = center.z - closest_z;
    dx * dx + dz * dz <= radius.max(0.0).powi(2)
}

fn find_object<'a>(database: &'a Database, name: &str) -> Option<&'a ProtoObject> {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name))
}

fn object_radius(prototype: &ProtoObject) -> f32 {
    valid_radius(prototype.obstruction_radius_x).max(valid_radius(prototype.obstruction_radius_z))
}

fn valid_radius(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_default()
}

fn has_type(prototype: &ProtoObject, expected: &str) -> bool {
    prototype
        .flags
        .iter()
        .chain(prototype.object_types.iter())
        .any(|value| value.eq_ignore_ascii_case(expected))
}

fn planar_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .unwrap_or(Vec3::Z)
}

fn xz_distance_squared(left: Vec3, right: Vec3) -> f32 {
    let delta = left - right;
    delta.x * delta.x + delta.z * delta.z
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

    #[test]
    fn exit_direction_rotates_and_member_count_expands_birth_radius() {
        let (world, database, building_id, squad_id) = fixture(1);

        let placement = trained_squad_placement(&world, building_id, squad_id, &database)
            .expect("birth placement");

        assert!(placement.preferred);
        assert!(placement.forward.abs_diff_eq(Vec3::X, 1.0e-6));
        assert!((placement.obstruction_radius - 2.0).abs() <= 1.0e-6);
        assert!(
            placement
                .position
                .abs_diff_eq(Vec3::new(6.015, 0.0, 0.0), 1.0e-5)
        );
    }

    #[test]
    fn obstructed_preferred_position_uses_perimeter_search() {
        let (mut world, database, building_id, squad_id) = fixture(0);
        let blocker = world.create_unit_at(2, Vec3::new(0.0, 0.0, 6.015));
        world
            .get_unit_mut(blocker)
            .unwrap()
            .obstruction_half_extents = Vec3::splat(1.0);

        let placement = trained_squad_placement(&world, building_id, squad_id, &database)
            .expect("fallback placement");

        assert!(!placement.preferred);
        assert!(
            !placement
                .position
                .abs_diff_eq(Vec3::new(0.0, 0.0, 6.015), 1.0e-5)
        );
        assert!(!circle_overlaps_unit(
            placement.position,
            placement.obstruction_radius,
            world.get_unit(blocker).unwrap().base.position,
            world.get_unit(blocker).unwrap().obstruction_half_extents,
        ));
    }

    fn fixture(exit_direction: i32) -> (World, Database, EntityId, EntityId) {
        let database = Database {
            objects: vec![
                ProtoObject {
                    name: "trainer".to_owned(),
                    exit_from_direction: Some(exit_direction),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "member".to_owned(),
                    obstruction_radius_x: Some(1.0),
                    obstruction_radius_z: Some(1.0),
                    ..ProtoObject::default()
                },
            ],
            squads: vec![ProtoSquad {
                name: "trained_squad".to_owned(),
                units: Some(UnitsWrapper {
                    entries: vec![UnitEntry {
                        proto_object: "member".to_owned(),
                        count: 4,
                        ..UnitEntry::default()
                    }],
                }),
                ..ProtoSquad::default()
            }],
            ..Database::default()
        };
        let mut world = World::new();
        world.init_players(2);
        let building_id = world.create_building_at(1, Vec3::ZERO);
        let building = world.get_building_mut(building_id).unwrap();
        building.proto_object_name = "trainer".to_owned();
        building.obstruction_half_extents = Vec3::new(4.0, 1.0, 4.0);
        building.base.set_forward(Vec3::Z);
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        world.get_squad_mut(squad_id).unwrap().proto_squad_name = "trained_squad".to_owned();
        let member = world.create_unit_at(1, Vec3::ZERO);
        world.get_unit_mut(member).unwrap().proto_object_name = "member".to_owned();
        assert!(world.attach_unit_to_squad(member, squad_id));
        (world, database, building_id, squad_id)
    }
}
