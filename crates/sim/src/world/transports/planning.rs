//! Deterministic whole-squad planning shared by retail transport entry points.

use crate::entity::Entity;
use crate::scenario::population::squad_population_amounts;
use crate::spawn::database_id;
use crate::{EntityId, World};
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::{Database, ProtoObject};

/// One carrier's assigned passenger squads and formation positions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TransportGroupPlan {
    pub(crate) passenger_squad_ids: Vec<EntityId>,
    pub(crate) pickup_position: Vec3,
    pub(crate) dropoff_position: Vec3,
}

/// Retail inputs used to count carriers and fill each whole-squad load.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TransportGroupRequest<'a> {
    pub(crate) passenger_squad_ids: &'a [EntityId],
    pub(crate) formation_center: Vec3,
    pub(crate) dropoff_center: Vec3,
    pub(crate) count_accepted_object_types: &'a [String],
    pub(crate) count_capacity: f32,
    pub(crate) load_accepted_object_types: &'a [String],
    pub(crate) load_capacity: f32,
    pub(crate) maximum_transports: u32,
    pub(crate) carrier_spacing: f32,
}

/// Plan formation positions and capacity-respecting whole-squad assignments.
pub(crate) fn plan_transport_groups(
    world: &World,
    database: &Database,
    request: TransportGroupRequest<'_>,
) -> Vec<TransportGroupPlan> {
    if !request.formation_center.is_finite()
        || !request.dropoff_center.is_finite()
        || !valid_capacity(request.count_capacity)
        || !valid_capacity(request.load_capacity)
        || request.maximum_transports == 0
    {
        return Vec::new();
    }
    let mut remaining = unique_live_squads(world, request.passenger_squad_ids);
    let population = remaining
        .iter()
        .filter(|squad_id| {
            transport_accepts_squad(world, **squad_id, request.count_accepted_object_types)
        })
        .map(|squad_id| rounded_transport_population(world, database, *squad_id))
        .sum::<f32>();
    let count = (population / request.count_capacity)
        .ceil()
        .to_u32()
        .unwrap_or(u32::MAX)
        .min(request.maximum_transports);
    if count == 0 {
        return Vec::new();
    }

    let direction =
        horizontal_direction(request.dropoff_center - request.formation_center).unwrap_or(Vec3::Z);
    let right = Vec3::Y.cross(direction).normalize_or(Vec3::X);
    let center = count.saturating_sub(1).to_f32().unwrap_or(f32::MAX) * 0.5;
    let mut result = Vec::with_capacity(count.to_usize().unwrap_or_default());
    for index in 0..count {
        let lateral = index.to_f32().unwrap_or(f32::MAX) - center;
        let offset = right * (lateral * request.carrier_spacing);
        let pickup = ground_position(world, request.formation_center + offset);
        let dropoff = ground_position(world, request.dropoff_center + offset);
        remaining.sort_by(|left, right| {
            squad_distance_squared(world, *left, pickup)
                .total_cmp(&squad_distance_squared(world, *right, pickup))
                .then_with(|| left.cmp(right))
        });
        let passengers = take_transport_load(world, database, &mut remaining, request);
        if !passengers.is_empty() {
            result.push(TransportGroupPlan {
                passenger_squad_ids: passengers,
                pickup_position: pickup,
                dropoff_position: dropoff,
            });
        }
    }
    result
}

/// Average the current positions of a deterministic squad list.
pub(crate) fn average_transport_position(world: &World, squads: &[EntityId]) -> Option<Vec3> {
    let mut total = Vec3::ZERO;
    let mut count = 0_u32;
    for squad_id in squads {
        if let Some(squad) = world.get_squad(*squad_id) {
            total += squad.base.position;
            count += 1;
        }
    }
    (count > 0).then(|| ground_position(world, total / count.to_f32().unwrap_or(f32::MAX)))
}

/// Formation spacing derived from the carrier's largest obstruction radius.
pub(crate) fn transport_carrier_spacing(prototype: &ProtoObject) -> f32 {
    let radius = prototype
        .obstruction_radius_x
        .unwrap_or_default()
        .abs()
        .max(prototype.obstruction_radius_z.unwrap_or_default().abs());
    (radius * 2.0 + 2.0).max(8.0)
}

fn take_transport_load(
    world: &World,
    database: &Database,
    remaining: &mut Vec<EntityId>,
    request: TransportGroupRequest<'_>,
) -> Vec<EntityId> {
    let mut loaded = Vec::new();
    let mut population = 0.0;
    let mut index = 0;
    while index < remaining.len() {
        let squad_id = remaining[index];
        let squad_population = transport_population(world, database, squad_id);
        let accepted = transport_accepts_squad(world, squad_id, request.load_accepted_object_types);
        if accepted && population + squad_population <= request.load_capacity {
            population += squad_population;
            loaded.push(remaining.remove(index));
        } else {
            index += 1;
        }
    }
    loaded
}

fn unique_live_squads(world: &World, squad_ids: &[EntityId]) -> Vec<EntityId> {
    let mut result = Vec::with_capacity(squad_ids.len());
    for squad_id in squad_ids {
        if world.get_squad(*squad_id).is_some_and(Entity::is_alive) && !result.contains(squad_id) {
            result.push(*squad_id);
        }
    }
    result
}

fn transport_accepts_squad(world: &World, squad_id: EntityId, accepted: &[String]) -> bool {
    accepted
        .iter()
        .any(|object_type| world.squad_object_type_match(squad_id, object_type) == Some(true))
}

fn rounded_transport_population(world: &World, database: &Database, squad_id: EntityId) -> f32 {
    transport_population(world, database, squad_id).round()
}

fn transport_population(world: &World, database: &Database, squad_id: EntityId) -> f32 {
    let Some(squad) = world.get_squad(squad_id) else {
        return 0.0;
    };
    database
        .squads
        .iter()
        .enumerate()
        .find(|(index, prototype)| database_id(prototype.dbid, *index) == squad.proto_squad_id)
        .and_then(|(_, prototype)| {
            squad_population_amounts(database, prototype)
                .into_iter()
                .next()
        })
        .or_else(|| squad.population_costs.first().copied())
        .map_or(0.0, |population| population.amount.max(0.0))
}

fn ground_position(world: &World, mut position: Vec3) -> Vec3 {
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    position
}

fn squad_distance_squared(world: &World, squad_id: EntityId, location: Vec3) -> f32 {
    world.get_squad(squad_id).map_or(f32::INFINITY, |squad| {
        let delta = squad.base.position - location;
        delta.x * delta.x + delta.z * delta.z
    })
}

fn horizontal_direction(direction: Vec3) -> Option<Vec3> {
    Vec3::new(direction.x, 0.0, direction.z).try_normalize()
}

fn valid_capacity(capacity: f32) -> bool {
    capacity.is_finite() && capacity > 0.0
}
