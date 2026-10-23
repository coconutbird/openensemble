//! Creation of `.scn`, `.sc2` and `.sc3` objects like `BScenario_loadObject` @ `0x14037D6A0`.

use std::collections::BTreeMap;

use glam::Vec3;
use pipeline::database::hw1::Database;

use super::player_setup::PlayerSetup;
use super::prototypes::code_proto_object_index;
use super::{
    LoadedScenario, ScenarioObject, apply_scenario_visual_variation, placed, representative_unit_id,
};
use crate::prototypes::{ScenarioProto, scenario_proto};
use crate::world::World;

/// Gamedata code proto object of the unit start markers.
const UNIT_START_OBJECT: &str = "UnitStart";
/// Gamedata code proto object of the rally start markers.
const RALLY_START_OBJECT: &str = "RallyStart";

/// Unit start and rally start markers that object loading consumed.
///
/// `BScenario_loadObject` does not create an object of the gamedata `UnitStart` or
/// `RallyStart` proto. It stores the object ID and position in `gScenarioUnitStartObjects`
/// or `gScenarioRallyStartObjects` for `BScenario_createStartingUnits`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct StartMarkers {
    /// Unit start markers in load order: object ID and authored position.
    pub(super) unit_starts: Vec<(i32, [f32; 3])>,
    /// Rally start markers in load order: object ID and authored position.
    pub(super) rally_starts: Vec<(i32, [f32; 3])>,
}

impl StartMarkers {
    /// Position of the first marker with `id`.
    pub(super) fn find(markers: &[(i32, [f32; 3])], id: i32) -> Option<[f32; 3]> {
        (id != -1)
            .then(|| markers.iter().find(|(marker, _)| *marker == id))
            .flatten()
            .map(|(_, position)| *position)
    }
}

/// Visual variation source of the objects of one file.
#[derive(Debug, Clone, Copy)]
pub(super) enum VisualVariations<'a> {
    /// The value of the object itself, `-1` when absent.
    Authored,
    /// Indexed values by object ID, `-1` for an object without an entry.
    ById(&'a BTreeMap<i32, i32>),
}

/// Create the objects of one object list in document order.
pub(super) fn load_objects(
    scenario: &mut LoadedScenario,
    objects: &[ScenarioObject],
    database: &Database,
    setup: &PlayerSetup,
    markers: &mut StartMarkers,
    visual_variations: VisualVariations<'_>,
) {
    let unit_start = code_proto_object_index(database, UNIT_START_OBJECT);
    let rally_start = code_proto_object_index(database, RALLY_START_OBJECT);
    for object in objects {
        // The engine skips an object without an owner, a negative ID or a position.
        let Some(owner) = setup.object_owner(object.player) else {
            continue;
        };
        let Some(authored_position) = object.position_vec3().filter(|_| object.id >= 0) else {
            continue;
        };
        let Some(proto) = scenario_proto(database, &object.proto_name, object.is_squad) else {
            continue;
        };
        if let ScenarioProto::Object(index, _) = proto {
            if Some(index) == unit_start {
                markers.unit_starts.push((object.id, authored_position));
                continue;
            }
            if Some(index) == rally_start {
                markers.rally_starts.push((object.id, authored_position));
                continue;
            }
        }
        let mut position = Vec3::from_array(authored_position);
        if !object.no_tie_to_ground() {
            position.y = ground_height(&scenario.world, position);
        }
        let forward = Vec3::from_array(object.forward_vec3());
        let Some(entity_id) = placed::create_scenario_object(
            &mut scenario.world,
            database,
            proto,
            &object.proto_name,
            placed::Placement {
                player_id: owner,
                position,
                forward,
            },
        ) else {
            continue;
        };
        if object.no_tie_to_ground() {
            clear_ground_tie(&mut scenario.world, entity_id);
        }
        let visual_variation_index = match visual_variations {
            VisualVariations::Authored => object.visual_variation_index.unwrap_or(-1).max(-1),
            VisualVariations::ById(indices) => indices.get(&object.id).copied().unwrap_or(-1),
        };
        apply_scenario_visual_variation(&mut scenario.world, entity_id, visual_variation_index);
        if let Some(unit_id) = representative_unit_id(&scenario.world, entity_id) {
            scenario.scenario_id_to_unit_id.insert(object.id, unit_id);
        }
        scenario
            .scenario_id_to_entity_id
            .insert(object.id, entity_id);
    }
}

/// Terrain height below a world position, or the position height without terrain.
///
/// This is the ground tie of a new object (`BTerrainSimRep_getHeight` @ `0x140062EB0` with
/// clamping).
pub(super) fn ground_height(world: &World, position: Vec3) -> f32 {
    world.terrain_height(position, true).unwrap_or(position.y)
}

/// Clear `mFlagTiesToGround` on a scenario object with the `NoTieToGround` flag.
///
/// `BScenario_loadObject` @ `0x14037D6A0` sets the `NoTieToGround` bit on the new object.
fn clear_ground_tie(world: &mut World, entity_id: crate::EntityId) {
    let unit_ids = world
        .get_squad(entity_id)
        .map_or_else(|| vec![entity_id], |squad| squad.unit_ids.clone());
    for unit_id in unit_ids {
        if let Some(unit) = world.get_unit_mut(unit_id) {
            unit.set_ties_to_ground(false);
        }
    }
}
