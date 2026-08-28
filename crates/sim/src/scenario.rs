//! Scenario world loading for Halo Wars maps.
//!
//! This module provides functionality to load scenario data into the simulation world.
//! The scenario data itself is parsed by the `pipeline` crate; this module handles
//! the simulation-specific logic of creating players and entities.
//!
//! # Example
//!
//! ```ignore
//! use pipeline::hw1::scenario::ScenarioData;
//! use pipeline::database::hw1::Database;
//! use sim::load_scenario_into_world;
//!
//! // Load scenario data from parsed SCN file
//! let scenario = ScenarioData::default();
//! let db = Database::new();
//! let loaded = load_scenario_into_world(&scenario, &db);
//! println!("Created {} players", loaded.world.player_count());
//! ```

use crate::entities::squads::marine::{MarineSquadSpec, is_marine_squad};
use crate::entities::squads::warthog::{WarthogSquadSpec, is_warthog_squad};
use crate::entities::units::marine::{MARINE_HITPOINTS, MarineUnitSpec, is_marine_unit};
use crate::entities::units::warthog::{WarthogUnitSpec, is_warthog_unit};
use crate::entities::{BaseId, SquadArchetype, SquadFormation, UnitArchetype};
use crate::entity_id::EntityId;
use crate::physics::{BoxCollider, PhysicsBody};
use crate::player::{PlayerId, PlayerType};
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use pipeline::source::{AssetSource, StdFileProvider};
use std::collections::{BTreeMap, HashMap};

mod coordinates;
mod starts;

// Re-export scenario types from pipeline for convenience
pub use coordinates::ScenarioPositionAxes;
pub use pipeline::hw1::scenario::{ScenarioData, ScenarioObject, ScenarioPlayer, ScenarioPosition};

/// Result of loading a scenario into a world.
///
/// Contains the world plus mappings for authored objects and initial bases.
pub struct LoadedScenario {
    /// The populated simulation world.
    pub world: World,
    /// Mapping from scenario object IDs to simulation entity IDs.
    pub scenario_id_to_entity_id: HashMap<i32, EntityId>,
    /// Initial base assigned to each active player with a scenario start.
    pub initial_base_ids: BTreeMap<PlayerId, BaseId>,
}

/// A scenario's authoritative simulation state and the assets used to build it.
///
/// The pipeline world is retained for renderer-facing presentation data such as
/// visuals, terrain, and lighting. Gameplay code should read and mutate
/// [`simulation`](Self::simulation), never reconstruct state from `content`.
pub struct LoadedGameScenario {
    /// Authoritative game state created from the active scenario and database.
    pub simulation: LoadedScenario,
    /// Parsed database and presentation assets from the same layered ERA source.
    pub content: pipeline::hw1::World,
    /// Layered base-game plus scenario asset source.
    pub source: AssetSource<StdFileProvider>,
}

/// Failure while loading a scenario and its database into the simulation.
#[derive(Debug, thiserror::Error)]
pub enum ScenarioAssetLoadError {
    /// No matching scenario archive exists in the requested game directory.
    #[error("scenario archive for '{scenario}' was not found in '{game_dir}'")]
    ScenarioArchiveNotFound {
        /// Scenario name, ERA filename, or SCN path supplied by the caller.
        scenario: String,
        /// Game directory searched for the scenario archive.
        game_dir: String,
    },
    /// The layered database or another required asset could not be parsed.
    #[error("failed to load the scenario database and assets: {0}")]
    Pipeline(#[from] pipeline::Error),
    /// The archive loaded, but it did not resolve to parsed SCN data.
    #[error("scenario archive for '{scenario}' did not contain resolvable SCN data")]
    ScenarioDataNotFound {
        /// Requested scenario identifier.
        scenario: String,
    },
}

impl LoadedScenario {
    /// Get entity ID from scenario object ID.
    #[must_use]
    pub fn get_entity_id(&self, scenario_id: i32) -> Option<EntityId> {
        self.scenario_id_to_entity_id.get(&scenario_id).copied()
    }

    /// Get the initial base assigned to an active player.
    #[must_use]
    pub fn get_initial_base_id(&self, player_id: PlayerId) -> Option<BaseId> {
        self.initial_base_ids.get(&player_id).copied()
    }
}

/// Load a scenario ERA, its database files, and its authoritative sim state.
///
/// The scenario archive is layered onto the base-game archives *before* the
/// pipeline database is parsed. Consequently, database files supplied by the
/// scenario participate in normal last-loaded-wins asset resolution. The same
/// parsed database then creates the simulation entities, while the returned
/// pipeline content remains available to presentation systems.
///
/// # Errors
///
/// Returns an error when the scenario archive cannot be found, required
/// database files cannot be parsed, or the archive contains no resolvable SCN.
pub fn load_scenario_from_game_dir(
    game_dir: &str,
    scenario: &str,
) -> Result<LoadedGameScenario, ScenarioAssetLoadError> {
    let mut source = pipeline::hw1::loader::load_game_dir(game_dir);
    if !source.load_scenario(scenario) {
        return Err(ScenarioAssetLoadError::ScenarioArchiveNotFound {
            scenario: scenario.to_owned(),
            game_dir: game_dir.to_owned(),
        });
    }

    // This must happen after load_scenario: Database::load resolves the highest
    // priority copy of every table, including scenario-local replacements.
    let content = pipeline::hw1::World::load_from_source(&mut source)?;
    let scenario_data = content.scenario_data.as_ref().ok_or_else(|| {
        ScenarioAssetLoadError::ScenarioDataNotFound {
            scenario: scenario.to_owned(),
        }
    })?;
    let max_players = content
        .scenario
        .as_ref()
        .map(|descriptor| descriptor.max_players)
        .filter(|maximum| *maximum > 0);
    let simulation =
        load_scenario_into_world_with_max_players(scenario_data, &content.database, max_players);

    Ok(LoadedGameScenario {
        simulation,
        content,
        source,
    })
}

/// Load a scenario into a new [`World`], creating players and entities.
///
/// Returns a [`LoadedScenario`] containing the populated world and
/// a mapping from scenario object IDs to entity IDs.
///
/// # Example
///
/// ```ignore
/// use pipeline::hw1::scenario::ScenarioData;
/// use pipeline::database::hw1::Database;
/// use sim::load_scenario_into_world;
///
/// let scenario = ScenarioData::default();
/// let db = Database::new();
/// let loaded = load_scenario_into_world(&scenario, &db);
/// println!("Created {} players", loaded.world.player_count());
/// ```
#[must_use]
pub fn load_scenario_into_world(scenario: &ScenarioData, db: &Database) -> LoadedScenario {
    load_scenario_into_world_with_max_players(scenario, db, None)
}

fn load_scenario_into_world_with_max_players(
    scenario: &ScenarioData,
    db: &Database,
    max_players: Option<u32>,
) -> LoadedScenario {
    let mut world = World::new();
    let mut scenario_id_to_entity_id = HashMap::new();
    let players = scenario.players.as_ref().map_or(&[][..], |w| &w.entries);
    let objects = scenario.objects.as_ref().map_or(&[][..], |w| &w.entries);
    let player_count =
        u8::try_from(players.len().min(crate::world::MAX_PLAYERS)).unwrap_or(u8::MAX);
    world.init_players(player_count);
    configure_players(&mut world, players, db);

    for obj in objects {
        let Some(entity_id) = create_scenario_object(&mut world, obj, db) else {
            continue;
        };
        if obj.id >= 0 {
            scenario_id_to_entity_id.insert(obj.id, entity_id);
        }
    }
    let initial_base_ids =
        starts::create_initial_bases(&mut world, scenario, db, player_count, max_players);

    LoadedScenario {
        world,
        scenario_id_to_entity_id,
        initial_base_ids,
    }
}

fn configure_players(world: &mut World, players: &[ScenarioPlayer], db: &Database) {
    for (index, scenario_player) in players.iter().take(crate::world::MAX_PLAYERS).enumerate() {
        let player_id = u8::try_from(index + 1).unwrap_or(u8::MAX);
        let Some(player) = world.get_player_mut(player_id) else {
            continue;
        };
        player.name.clone_from(&scenario_player.name);
        player.civ_id = find_name_index(&db.civs, &scenario_player.civ, |civ| &civ.name);
        player.leader_id =
            find_name_index(&db.leaders, &scenario_player.leader1, |leader| &leader.name);
        player.team_id = u8::try_from(scenario_player.team).unwrap_or_default();
        player.player_type = if scenario_player.controllable {
            PlayerType::Human
        } else {
            PlayerType::ComputerAi
        };
        if scenario_player.supplies != 0.0 || scenario_player.power != 0.0 {
            player.resources.set(0, scenario_player.supplies);
            player.resources.set(1, scenario_player.power);
        }
    }
}

fn find_name_index<T>(values: &[T], name: &str, key: impl Fn(&T) -> &str) -> i32 {
    values
        .iter()
        .position(|value| key(value).eq_ignore_ascii_case(name))
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1)
}

fn create_scenario_object(
    world: &mut World,
    object: &ScenarioObject,
    db: &Database,
) -> Option<EntityId> {
    if object.is_squad {
        return Some(create_scenario_squad(world, object, db));
    }
    create_scenario_unit(world, object, db)
}

fn create_scenario_squad(world: &mut World, object: &ScenarioObject, db: &Database) -> EntityId {
    let player_id = u8::try_from(object.player).unwrap_or_default();
    let position = scenario_position(object);
    let squad_id = world.create_squad_at(player_id, position);
    let proto_name = object.proto_name.trim();
    let proto = find_proto_squad(db, proto_name);
    let forward = scenario_forward(object);
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad.base.set_forward(forward);
        squad.proto_squad_id = proto.map_or(-1, |(index, squad)| database_id(squad.dbid, index));
        proto_name.clone_into(&mut squad.proto_squad_name);
        squad.archetype = if is_warthog_squad(proto_name) {
            SquadArchetype::Warthog
        } else if is_marine_squad(proto_name) {
            SquadArchetype::Marine
        } else {
            SquadArchetype::Generic
        };
        if squad.archetype == SquadArchetype::Warthog {
            let stock = WarthogSquadSpec::default();
            squad.turn_radius = stock.turn_radius;
            squad.min_turn_radius = stock.min_turn_radius;
            squad.max_turn_radius = stock.max_turn_radius;
        }
        if squad.archetype == SquadArchetype::Marine {
            squad.formation = SquadFormation::Flock;
        }
        if proto
            .and_then(|(_, squad)| squad.formation_type.as_deref())
            .is_some_and(|formation| formation.eq_ignore_ascii_case("Flock"))
        {
            squad.formation = SquadFormation::Flock;
        }
        if let Some(turn_radius) = proto.and_then(|(_, squad)| squad.turn_radius.as_ref()) {
            squad.turn_radius = valid_nonnegative(Some(turn_radius.value)).unwrap_or_default();
            squad.min_turn_radius = valid_nonnegative(turn_radius.min).unwrap_or_default();
            let fallback_max = valid_nonnegative(Some(turn_radius.value))
                .unwrap_or(squad.min_turn_radius)
                .max(squad.min_turn_radius);
            squad.max_turn_radius = valid_nonnegative(turn_radius.max)
                .unwrap_or(fallback_max)
                .max(squad.min_turn_radius);
        }
    }
    if let Some((_, proto)) = proto {
        create_squad_members(world, squad_id, player_id, position, proto, db);
    }
    apply_squad_member_movement_settings(world, squad_id);
    apply_squad_physics_settings(world, squad_id);
    squad_id
}

fn create_squad_members(
    world: &mut World,
    squad_id: EntityId,
    player_id: u8,
    position: Vec3,
    proto: &ProtoSquad,
    db: &Database,
) {
    let Some(units) = &proto.units else {
        return;
    };
    let archetype = world
        .get_squad(squad_id)
        .map_or(SquadArchetype::Generic, |squad| squad.archetype);
    let marine_squad = MarineSquadSpec::default();
    let mut slot = 0_usize;
    for entry in &units.entries {
        for _ in 0..entry.count.max(0) {
            let unit_id = world.create_unit_at(player_id, position);
            configure_unit(world, unit_id, entry.proto_object.trim(), db);
            let attached = world.attach_unit_to_squad(unit_id, squad_id);
            debug_assert!(attached, "new squad member should attach");
            if archetype == SquadArchetype::Marine
                && let Some(offset) = marine_squad.initial_formation_offset(slot)
            {
                let assigned = world.set_squad_member_formation_offset(unit_id, offset);
                debug_assert!(assigned, "attached Marine should accept a formation offset");
            }
            slot += 1;
        }
    }
}

fn create_scenario_unit(
    world: &mut World,
    object: &ScenarioObject,
    db: &Database,
) -> Option<EntityId> {
    let proto_name = object.proto_name.trim();
    let (proto_index, proto) = find_proto_object(db, proto_name)?;
    let player_id = u8::try_from(object.player).unwrap_or_default();
    let position = scenario_position(object);
    let kind = classify_proto_object(proto)?;
    let unit_id = match kind {
        PlacedUnitKind::Mobile => world.create_unit_at(player_id, position),
        PlacedUnitKind::Building => world.create_building_at(player_id, position),
    };
    configure_unit_from_proto(world, unit_id, proto_name, proto_index, proto);
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.base.set_forward(scenario_forward(object));
    }
    if kind == PlacedUnitKind::Building && creates_base(proto) {
        let _base_id = world.register_base(unit_id);
    }
    Some(unit_id)
}

fn configure_unit(world: &mut World, unit_id: EntityId, proto_name: &str, db: &Database) {
    let Some((proto_index, proto)) = find_proto_object(db, proto_name) else {
        if let Some(unit) = world.get_unit_mut(unit_id) {
            proto_name.clone_into(&mut unit.proto_object_name);
            if is_warthog_unit(proto_name, None) {
                configure_warthog(unit, WarthogUnitSpec::default());
            } else if is_marine_unit(proto_name) {
                unit.set_max_hitpoints(MARINE_HITPOINTS);
                configure_marine(unit, MarineUnitSpec::default());
            }
        }
        return;
    };
    configure_unit_from_proto(world, unit_id, proto_name, proto_index, proto);
}

fn configure_unit_from_proto(
    world: &mut World,
    unit_id: EntityId,
    proto_name: &str,
    proto_index: usize,
    proto: &ProtoObject,
) {
    let Some(unit) = world.get_unit_mut(unit_id) else {
        return;
    };
    unit.proto_object_id = database_id(proto.dbid, proto_index);
    proto_name.clone_into(&mut unit.proto_object_name);
    if let Some(hitpoints) = proto.hitpoints {
        unit.set_max_hitpoints(hitpoints);
    }
    if !unit.is_building()
        && let Some(speed) = proto.max_velocity.or(proto.velocity)
        && speed.is_finite()
        && speed >= 0.0
    {
        unit.speed = speed;
    }
    unit.acceleration = valid_nonnegative(proto.acceleration).unwrap_or_default();
    unit.turn_rate_degrees = valid_nonnegative(proto.turn_rate).unwrap_or_default();
    unit.obstruction_half_extents = obstruction_half_extents(proto).unwrap_or(Vec3::ZERO);
    if is_warthog_unit(proto_name, proto.physics_info.as_deref()) {
        configure_warthog(unit, warthog_spec_from_proto(proto));
    } else if is_marine_unit(proto_name) {
        configure_marine(unit, marine_spec_from_proto(proto));
    } else if unit.is_building()
        && let Some(collider) = obstruction_collider(proto)
    {
        unit.physics = Some(PhysicsBody::static_obstruction(collider));
    }
}

fn configure_warthog(unit: &mut crate::entities::Unit, spec: WarthogUnitSpec) {
    unit.archetype = UnitArchetype::Warthog;
    unit.speed = spec.max_speed;
    unit.acceleration = spec.acceleration;
    unit.turn_rate_degrees = spec.turn_rate_degrees;
    unit.obstruction_half_extents = spec.half_extents;
    unit.physics = Some(spec.physics_body(unit.base.position.y));
}

fn configure_marine(unit: &mut crate::entities::Unit, spec: MarineUnitSpec) {
    unit.archetype = UnitArchetype::Marine;
    unit.speed = spec.max_speed;
    unit.acceleration = spec.acceleration;
    unit.turn_rate_degrees = spec.turn_rate_degrees;
    unit.obstruction_half_extents = spec.half_extents;
    unit.physics = None;
}

fn warthog_spec_from_proto(proto: &ProtoObject) -> WarthogUnitSpec {
    let mut spec = WarthogUnitSpec::default();
    spec.max_speed =
        valid_nonnegative(proto.max_velocity.or(proto.velocity)).unwrap_or(spec.max_speed);
    spec.acceleration = valid_nonnegative(proto.acceleration).unwrap_or(spec.acceleration);
    spec.turn_rate_degrees = valid_nonnegative(proto.turn_rate).unwrap_or(spec.turn_rate_degrees);
    spec.half_extents.x = valid_positive(proto.obstruction_radius_x).unwrap_or(spec.half_extents.x);
    spec.half_extents.y = valid_positive(proto.obstruction_radius_y).unwrap_or(spec.half_extents.y);
    spec.half_extents.z = valid_positive(proto.obstruction_radius_z).unwrap_or(spec.half_extents.z);
    spec
}

fn marine_spec_from_proto(proto: &ProtoObject) -> MarineUnitSpec {
    let mut spec = MarineUnitSpec::default();
    spec.max_speed =
        valid_nonnegative(proto.max_velocity.or(proto.velocity)).unwrap_or(spec.max_speed);
    spec.acceleration = valid_nonnegative(proto.acceleration).unwrap_or(spec.acceleration);
    spec.turn_rate_degrees = valid_nonnegative(proto.turn_rate).unwrap_or(spec.turn_rate_degrees);
    spec.half_extents = obstruction_half_extents(proto).unwrap_or(spec.half_extents);
    spec
}

fn obstruction_half_extents(proto: &ProtoObject) -> Option<Vec3> {
    let x = valid_positive(proto.obstruction_radius_x)?;
    let z = valid_positive(proto.obstruction_radius_z)?;
    let y = valid_positive(proto.obstruction_radius_y).unwrap_or(1.0);
    Some(Vec3::new(x, y, z))
}

fn obstruction_collider(proto: &ProtoObject) -> Option<BoxCollider> {
    Some(BoxCollider::new(
        obstruction_half_extents(proto)?,
        Vec3::ZERO,
    ))
}

fn apply_squad_member_movement_settings(world: &mut World, squad_id: EntityId) {
    let Some(unit_ids) = world
        .get_squad(squad_id)
        .map(|squad| squad.unit_ids.clone())
    else {
        return;
    };
    let mut members = unit_ids.iter().filter_map(|&id| world.get_unit(id));
    let Some(first) = members.next() else {
        return;
    };
    let mut speed = first.speed;
    let mut acceleration = first.acceleration;
    let mut turn_rate_degrees = first.turn_rate_degrees;
    for member in members {
        speed = speed.min(member.speed);
        acceleration = acceleration.min(member.acceleration);
        turn_rate_degrees = turn_rate_degrees.min(member.turn_rate_degrees);
    }
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad.speed = speed;
        squad.acceleration = acceleration;
        squad.turn_rate_degrees = turn_rate_degrees;
    }
}

fn apply_squad_physics_settings(world: &mut World, squad_id: EntityId) {
    let Some(squad) = world.get_squad(squad_id) else {
        return;
    };
    let minimum = squad.min_turn_radius;
    let maximum = squad.max_turn_radius;
    let unit_ids = squad.unit_ids.clone();
    for unit_id in unit_ids {
        let Some(body) = world
            .get_unit_mut(unit_id)
            .and_then(|unit| unit.physics.as_mut())
        else {
            continue;
        };
        if maximum > 0.0 {
            body.set_turn_radius_range(minimum, maximum);
        }
    }
}

fn valid_positive(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

fn valid_nonnegative(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlacedUnitKind {
    Mobile,
    Building,
}

fn classify_proto_object(proto: &ProtoObject) -> Option<PlacedUnitKind> {
    if proto
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Building"))
        || proto
            .select_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("Building"))
        || creates_base(proto)
    {
        return Some(PlacedUnitKind::Building);
    }
    proto
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Unit"))
        .then_some(PlacedUnitKind::Mobile)
}

fn creates_base(proto: &ProtoObject) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("KBCreatesBase"))
}

fn find_proto_object<'a>(db: &'a Database, name: &str) -> Option<(usize, &'a ProtoObject)> {
    db.objects
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name))
}

fn find_proto_squad<'a>(db: &'a Database, name: &str) -> Option<(usize, &'a ProtoSquad)> {
    db.squads
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name))
}

fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

/// Convert an authored SCN object position into canonical terrain-world axes.
#[must_use]
pub const fn scenario_object_position_to_world(position: [f32; 3]) -> [f32; 3] {
    [position[2], position[1], position[0]]
}

/// Convert an authored SCN object direction into canonical terrain-world axes.
#[must_use]
pub const fn scenario_object_direction_to_world(direction: [f32; 3]) -> [f32; 3] {
    scenario_object_position_to_world(direction)
}

fn scenario_position(object: &ScenarioObject) -> Vec3 {
    Vec3::from_array(scenario_object_position_to_world(object.position_vec3()))
}

fn scenario_forward(object: &ScenarioObject) -> Vec3 {
    Vec3::from_array(scenario_object_direction_to_world(object.forward_vec3()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SCENARIO: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Scenario>
    <Positions>
        <Position Number="1" Position="100.0,0.0,100.0" Forward="0.0,0.0,1.0" />
        <Position Number="2" Position="200.0,0.0,200.0" Forward="0.0,0.0,-1.0" />
    </Positions>
    <Players>
        <Player Name="Player1" Civ="UNSC" Leader1="Cutter" Team="1" Color="0" />
        <Player Name="Player2" Civ="Covenant" Leader1="Arbiter" Team="2" Color="1" />
    </Players>
    <Objects>
        <Object IsSquad="true" Player="1" ID="0" Position="100.0,0.0,100.0" Forward="0.0,0.0,1.0">
            unsc_inf_marine_01
        </Object>
        <Object IsSquad="true" Player="1" ID="1" Position="110.0,0.0,100.0">
            unsc_inf_marine_01
        </Object>
        <Object IsSquad="true" Player="2" ID="2" Position="200.0,0.0,200.0">
            cov_inf_grunt_01
        </Object>
    </Objects>
</Scenario>"#;

    #[test]
    fn test_load_into_world() {
        let scenario = ScenarioData::from_xml_str(SAMPLE_SCENARIO).unwrap();
        let db = Database::new(); // empty db — names won't resolve
        let loaded = load_scenario_into_world(&scenario, &db);

        // Check players (Gaia + 2 players)
        assert_eq!(loaded.world.player_count(), 3);

        // Check player 1
        let p1 = loaded.world.get_player(1).unwrap();
        assert_eq!(p1.name, "Player1");
        assert_eq!(p1.team_id, 1);
        // civ_id/leader_id are -1 since db is empty (no civs/leaders loaded)
        assert_eq!(p1.civ_id, -1);

        // Check player 2
        let p2 = loaded.world.get_player(2).unwrap();
        assert_eq!(p2.name, "Player2");
        assert_eq!(p2.team_id, 2);

        // Check squads were created (count objects with is_squad=true)
        let squad_count = scenario
            .objects
            .as_ref()
            .map_or(0, |o| o.entries.iter().filter(|e| e.is_squad).count());
        assert_eq!(squad_count, 3);
        assert_eq!(loaded.world.squads.len(), squad_count);

        // Check scenario ID mapping
        let entity_id = loaded.get_entity_id(0).unwrap();
        let squad = loaded.world.get_squad(entity_id).unwrap();
        assert!((squad.position().x - 100.0).abs() < 0.01);
    }

    #[test]
    fn loads_squad_members_buildings_and_base_anchors() {
        use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

        let scenario = ScenarioData::from_xml_str(
            r#"<Scenario>
                <Players><Player Name="P1" Team="1" /></Players>
                <Objects>
                    <Object IsSquad="true" Player="1" ID="10">marine_squad</Object>
                    <Object Player="1" ID="20" Position="5,0,7">unsc_base</Object>
                </Objects>
            </Scenario>"#,
        )
        .unwrap();
        let mut db = Database::new();
        db.objects.push(ProtoObject {
            name: "marine".to_owned(),
            dbid: Some(101),
            object_class: Some("Unit".to_owned()),
            hitpoints: Some(75.0),
            ..ProtoObject::default()
        });
        db.objects.push(ProtoObject {
            name: "unsc_base".to_owned(),
            dbid: Some(202),
            object_class: Some("Building".to_owned()),
            flags: vec!["KBCreatesBase".to_owned()],
            hitpoints: Some(1_000.0),
            ..ProtoObject::default()
        });
        db.squads.push(ProtoSquad {
            name: "marine_squad".to_owned(),
            dbid: Some(303),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "marine".to_owned(),
                    count: 2,
                    role: None,
                }],
            }),
            ..ProtoSquad::default()
        });

        let loaded = load_scenario_into_world(&scenario, &db);
        let squad_id = loaded.get_entity_id(10).unwrap();
        let building_id = loaded.get_entity_id(20).unwrap();
        let squad = loaded.world.get_squad(squad_id).unwrap();
        let building = loaded.world.get_building(building_id).unwrap();

        assert_eq!(squad.proto_squad_id, 303);
        assert_eq!(squad.unit_ids.len(), 2);
        assert_eq!(building.proto_object_id, 202);
        assert!((building.hitpoints - 1_000.0).abs() < f32::EPSILON);
        assert_eq!(loaded.world.bases().count(), 1);
        assert!(building.base_id.is_some());
    }
}
