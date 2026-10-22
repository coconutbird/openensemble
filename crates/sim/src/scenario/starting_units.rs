//! Starting units like `BScenario_createStartingUnits` @ `0x140384760`.

use std::collections::BTreeMap;

use glam::Vec3;
use pipeline::database::hw1::{Database, Leader};
use pipeline::hw1::scenario::ScenarioPosition;

use super::objects::{StartMarkers, ground_height};
use super::placement::engine_positions;
use super::player_setup::{ActivePlayer, PlayerSetup};
use super::prototypes::code_proto_object;
use super::{
    ScenarioData, create_squad_from_prototype, placed, scenario_object_direction_to_world,
    scenario_object_position_to_world,
};
use crate::entities::BaseId;
use crate::player::{GAIA_PLAYER, PlayerId};
use crate::prototypes::{object_entry_by_name_trim_both, proto_id, squad_by_name_trim_query};
use crate::world::World;

/// Gamedata code proto object that marks a start position without a player.
const SKIRMISH_EMPTY_BASE_OBJECT: &str = "SkirmishEmptyBaseObject";

/// Object type of the building that takes the rally start point.
const BASE_OBJECT_TYPE: &str = "Base";

/// Create the starting units of each active player and the empty bases.
///
/// Returns the base that the starting units of each player created.
pub(super) fn create_starting_units(
    world: &mut World,
    database: &Database,
    scenario: &ScenarioData,
    setup: &PlayerSetup,
    markers: &StartMarkers,
) -> BTreeMap<PlayerId, BaseId> {
    let positions = engine_positions(scenario);
    let mut used = vec![false; positions.len()];
    let mut bases = BTreeMap::new();
    for active in &setup.players {
        let Some(start) = active.start.and_then(|index| positions.get(index).copied()) else {
            continue;
        };
        let start_position = start.position_vec3();
        // The engine marks the first position with the same coordinates.
        if let Some(index) = positions
            .iter()
            .position(|position| position.position_vec3() == start_position)
        {
            used[index] = true;
        }
        let units_created = scenario.players()[active.scenario_index].use_starting_units
            && create_player_units(world, database, active, start, markers, &mut bases);
        set_rally_start(world, active.player_id, start, markers, units_created);
    }
    create_empty_bases(world, database, &positions, &used);
    bases
}

/// Engine-frame transform of a start position: origin, forward and right
/// (`right = normalize(up × forward)`, or zero for a zero forward).
#[derive(Debug, Clone, Copy)]
struct StartFrame {
    origin: Vec3,
    forward: Vec3,
    right: Vec3,
}

impl StartFrame {
    fn new(position: &ScenarioPosition) -> Option<Self> {
        let origin = Vec3::from_array(position.position_vec3()?);
        let forward = Vec3::from_array(position.forward_vec3());
        let right = Vec3::Y.cross(forward).normalize_or_zero();
        Some(Self {
            origin,
            forward,
            right,
        })
    }

    /// `origin + offset.x * right + offset.y * up + offset.z * forward`.
    fn offset(self, offset: Option<&pipeline::database::hw1::types::Vector3>) -> Vec3 {
        let offset = offset.map_or(Vec3::ZERO, |offset| Vec3::new(offset.x, offset.y, offset.z));
        self.origin + offset.x * self.right + offset.y * Vec3::Y + offset.z * self.forward
    }
}

/// Spawn points in the engine frame: the start position, then the unit start markers.
/// Each new starting unit or squad takes the next point, then uses its offset.
struct SpawnPoints {
    points: Vec<Vec3>,
    next: usize,
}

impl SpawnPoints {
    fn take(
        &mut self,
        frame: StartFrame,
        offset: Option<&pipeline::database::hw1::types::Vector3>,
    ) -> Vec3 {
        let point = self
            .points
            .get(self.next)
            .copied()
            .unwrap_or_else(|| frame.offset(offset));
        self.next += 1;
        point
    }
}

/// World position (ground-tied) and forward of an engine-frame spawn point.
fn world_transform(world: &World, point: Vec3, forward: Vec3) -> (Vec3, Vec3) {
    let mut position = Vec3::from_array(scenario_object_position_to_world(point.to_array()));
    position.y = ground_height(world, position);
    let forward = Vec3::from_array(scenario_object_direction_to_world(forward.to_array()));
    (position, forward)
}

/// Create the leader starting units and squads of one player. Returns `false` if the player
/// has no valid civ or leader.
fn create_player_units(
    world: &mut World,
    database: &Database,
    active: &ActivePlayer,
    start: &ScenarioPosition,
    markers: &StartMarkers,
    bases: &mut BTreeMap<PlayerId, BaseId>,
) -> bool {
    let Some(player) = world.get_player(active.player_id) else {
        return false;
    };
    let has_civ = usize::try_from(player.civ_id).is_ok_and(|index| index < database.civs.len());
    let leader = usize::try_from(player.leader_id)
        .ok()
        .and_then(|index| database.leaders.get(index));
    let (Some(leader), true, Some(frame)) = (leader, has_civ, StartFrame::new(start)) else {
        return false;
    };
    let mut spawn_points = SpawnPoints {
        points: std::iter::once(frame.origin)
            .chain(
                start
                    .unit_start_objects()
                    .into_iter()
                    .filter_map(|id| StartMarkers::find(&markers.unit_starts, id))
                    .map(Vec3::from_array),
            )
            .collect(),
        next: 0,
    };
    create_leader_units(
        world,
        database,
        active.player_id,
        leader,
        frame,
        &mut spawn_points,
        bases,
    );
    for squad in &leader.starting_squads {
        let point = spawn_points.take(frame, squad.offset.as_ref());
        if squad_by_name_trim_query(database, &squad.proto_squad).is_none() {
            continue;
        }
        let (position, forward) = world_transform(world, point, frame.forward);
        // A `FlyIn` squad uses the civ transport in retail unless `NoBirthAnims` is defined.
        // No retail leader sets `FlyIn`, so the squad spawns in place.
        let _squad_id = create_squad_from_prototype(
            world,
            active.player_id,
            position,
            forward,
            squad.proto_squad.trim(),
            database,
        );
    }
    true
}

/// Create the leader `StartingUnit` objects and start their `BuildOther` work.
fn create_leader_units(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    leader: &Leader,
    frame: StartFrame,
    spawn_points: &mut SpawnPoints,
    bases: &mut BTreeMap<PlayerId, BaseId>,
) {
    for unit in &leader.starting_units {
        let point = spawn_points.take(frame, unit.offset.as_ref());
        let Some((prototype_index, prototype)) =
            object_entry_by_name_trim_both(database, &unit.proto_object)
        else {
            continue;
        };
        let (position, forward) = world_transform(world, point, frame.forward);
        let Some((_, Some(unit_id))) = placed::create_object_entity(
            world,
            database,
            prototype_index,
            prototype,
            placed::Placement {
                player_id,
                position,
                forward,
            },
        ) else {
            continue;
        };
        let Some((target_index, _)) = unit
            .build_other
            .as_deref()
            .and_then(|name| object_entry_by_name_trim_both(database, name))
        else {
            continue;
        };
        // `DoppleOnStart` only adds a presentation entry to the building action
        // (`sub_1404FAA10`); the simulation state does not change.
        let Ok(target_id) = world.start_build_other_without_cost(
            player_id,
            unit_id,
            database,
            proto_id(target_index),
        ) else {
            continue;
        };
        if let Some(base_id) = world
            .get_building(target_id)
            .and_then(|target| target.base_id)
        {
            bases.entry(player_id).or_insert(base_id);
        }
    }
}

/// Apply the `RallyStartObject` marker of a start position.
///
/// If the starting units ran and the player has exactly one `Base` building, the building
/// gets the rally point (`sub_1404D4260`). Otherwise the player gets it (`sub_1402F3470`).
fn set_rally_start(
    world: &mut World,
    player_id: PlayerId,
    start: &ScenarioPosition,
    markers: &StartMarkers,
    units_created: bool,
) {
    let Some(point) = StartMarkers::find(&markers.rally_starts, start.rally_start_object) else {
        return;
    };
    let mut position = Vec3::from_array(scenario_object_position_to_world(point));
    position.y = ground_height(world, position);
    let single_base = {
        let mut bases = world
            .units
            .iter()
            .filter(|(_, unit)| {
                unit.base.player_id == player_id && unit.is_object_type(BASE_OBJECT_TYPE)
            })
            .map(|(id, _)| id);
        match (bases.next(), bases.next()) {
            (Some(base_id), None) => Some(base_id),
            _ => None,
        }
    };
    if units_created && let Some(base_id) = single_base {
        let _set = world.set_unit_rally_point(base_id, player_id, position, None);
        return;
    }
    let _set = world.set_player_rally_point(player_id, position, None);
}

/// Create the gamedata `SkirmishEmptyBaseObject` for Gaia on each unused start position.
fn create_empty_bases(
    world: &mut World,
    database: &Database,
    positions: &[&ScenarioPosition],
    used: &[bool],
) {
    let Some((prototype_index, prototype)) =
        code_proto_object(database, SKIRMISH_EMPTY_BASE_OBJECT)
    else {
        return;
    };
    for (position, _) in positions.iter().zip(used).filter(|(_, used)| !**used) {
        let Some(frame) = StartFrame::new(position) else {
            continue;
        };
        let (position, forward) = world_transform(world, frame.origin, frame.forward);
        let _created = placed::create_object_entity(
            world,
            database,
            prototype_index,
            prototype,
            placed::Placement {
                player_id: GAIA_PLAYER,
                position,
                forward,
            },
        );
    }
}

#[cfg(test)]
#[path = "starting_units/tests.rs"]
mod tests;
