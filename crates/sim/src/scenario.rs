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
use crate::entities::{BaseId, ShieldCoverage, SquadArchetype, SquadFormation, UnitArchetype};
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::physics::{BoxCollider, PhysicsBody};
use crate::player::{DEFAULT_PLAYER_DIFFICULTY, PlayerId, PlayerType};
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use pipeline::source::{AssetSource, StdFileProvider};
use std::collections::{BTreeMap, HashMap};

mod garrison;

mod coordinates;
mod design_lines;
mod forbids;
mod objectives;
pub(crate) mod placed;
pub(crate) mod population;
mod prototypes;
mod resources;
mod sockets;
mod starts;
mod triggers;

// Re-export scenario types from pipeline for convenience
pub use coordinates::ScenarioPositionAxes;
pub use design_lines::DesignLineLoadError;
pub use objectives::ObjectiveLoadError;
pub use pipeline::hw1::scenario::{ScenarioData, ScenarioObject, ScenarioPlayer, ScenarioPosition};
use prototypes::{
    PlacedUnitKind, classify_proto_object, creates_base, database_id, find_proto_object,
    find_proto_squad, is_class_zero_object, prototype_has_flag,
};

/// Result of loading a scenario into a world.
///
/// Contains the world plus mappings for authored objects and initial bases.
pub struct LoadedScenario {
    /// The populated simulation world.
    pub world: World,
    /// Immutable gameplay definitions resolved from the scenario-layered source.
    pub gameplay: GameplayCatalog,
    /// Mapping from scenario object IDs to simulation entity IDs.
    pub scenario_id_to_entity_id: HashMap<i32, EntityId>,
    /// Mapping from scenario object IDs to their representative simulation unit.
    pub scenario_id_to_unit_id: HashMap<i32, EntityId>,
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
    /// The selected SCN could not be reopened to load its trigger systems.
    #[error("selected scenario definition '{path}' was not found in the layered asset source")]
    ScenarioDefinitionNotFound {
        /// Canonical SCN path selected by the scenario descriptor.
        path: String,
    },
    /// A scenario trigger system was malformed.
    #[error("failed to load a scenario trigger system: {0}")]
    Trigger(#[from] crate::trigger::LoadError),
    /// The scenario's synchronized XSD simulation terrain was malformed.
    #[error("failed to load scenario simulation terrain: {0}")]
    Terrain(#[from] crate::world::TerrainLoadError),
    /// Scenario-authored design-line path geometry was malformed.
    #[error("failed to load scenario design lines: {0}")]
    DesignLines(#[from] DesignLineLoadError),
    /// Scenario-authored objective state was malformed.
    #[error("failed to load scenario objectives: {0}")]
    Objectives(#[from] ObjectiveLoadError),
}

impl LoadedScenario {
    /// Get entity ID from scenario object ID.
    #[must_use]
    pub fn get_entity_id(&self, scenario_id: i32) -> Option<EntityId> {
        self.scenario_id_to_entity_id.get(&scenario_id).copied()
    }

    /// Get the representative unit ID for a scenario object or squad placement.
    #[must_use]
    pub fn get_unit_id(&self, scenario_id: i32) -> Option<EntityId> {
        self.scenario_id_to_unit_id.get(&scenario_id).copied()
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
    let content = pipeline::hw1::World::load_from_source_with_options(
        &mut source,
        pipeline::hw1::WorldLoadOptions::runtime(),
    )?;
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
    let gameplay = GameplayCatalog::load_from_source(&content.database, &mut source);
    let scenario_path = content
        .scenario
        .as_ref()
        .map(pipeline::hw1::scenario::ScenarioDescriptor::scn_path)
        .ok_or_else(|| ScenarioAssetLoadError::ScenarioDataNotFound {
            scenario: scenario.to_owned(),
        })?;
    let trigger_document = source.read_xmb(&scenario_path).ok_or_else(|| {
        ScenarioAssetLoadError::ScenarioDefinitionNotFound {
            path: scenario_path.clone(),
        }
    })?;
    let mut simulation = load_scenario_into_world_with_max_players(
        scenario_data,
        &content.database,
        max_players,
        gameplay,
    );
    if let Some(terrain) = &content.terrain_data {
        let minimum = Vec3::from_array(terrain.header.world_min);
        let maximum = Vec3::from_array(terrain.header.world_max);
        let _configured = simulation.world.configure_terrain_bounds(minimum, maximum);
    }
    if let Some(xsd_path) = content
        .scenario
        .as_ref()
        .and_then(pipeline::hw1::scenario::ScenarioDescriptor::xtd_path)
        .and_then(|path| path.strip_suffix(".xtd").map(|base| format!("{base}.xsd")))
        && let Some(xsd) = source.resolve_exact(&xsd_path)
    {
        simulation.world.configure_terrain_simulation(&xsd)?;
    }
    objectives::load_objectives(&mut simulation.world, &trigger_document)?;
    design_lines::load_design_lines(&mut simulation.world, &trigger_document)?;
    forbids::apply_scenario_forbids(&mut simulation.world, &content.database, &trigger_document);
    triggers::load_trigger_systems(&mut simulation, &content.database, &trigger_document)?;
    let animation_requests = triggers::scripted_animation_requests(&simulation);
    simulation.gameplay.load_scripted_animation_requests(
        &content.database,
        &mut source,
        animation_requests,
    );

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
    load_scenario_into_world_with_max_players(scenario, db, None, GameplayCatalog::default())
}

fn load_scenario_into_world_with_max_players(
    scenario: &ScenarioData,
    db: &Database,
    max_players: Option<u32>,
    gameplay: GameplayCatalog,
) -> LoadedScenario {
    let mut world = World::new();
    world.configure_prototype_catalogs(db);
    world.set_construction_damage_multiplier(
        db.game_data
            .as_ref()
            .and_then(|game_data| game_data.construction_damage_multiplier),
    );
    let mut scenario_id_to_entity_id = HashMap::new();
    let mut scenario_id_to_unit_id = HashMap::new();
    let players = scenario.players.as_ref().map_or(&[][..], |w| &w.entries);
    let objects = scenario.objects.as_ref().map_or(&[][..], |w| &w.entries);
    let player_count =
        u8::try_from(players.len().min(crate::world::MAX_PLAYERS)).unwrap_or(u8::MAX);
    world.init_players(player_count);
    objectives::configure_objective_references(&mut world, scenario.objectives());
    configure_players(&mut world, players, db);

    for obj in objects {
        let Some(entity_id) = create_scenario_object(&mut world, obj, db) else {
            continue;
        };
        if obj.id >= 0 {
            if let Some(unit_id) = representative_unit_id(&world, entity_id) {
                scenario_id_to_unit_id.insert(obj.id, unit_id);
            }
            scenario_id_to_entity_id.insert(obj.id, entity_id);
        }
    }
    let initial_base_ids =
        starts::create_initial_bases(&mut world, scenario, db, player_count, max_players);
    world.configure_unit_revivals(&gameplay);

    LoadedScenario {
        world,
        gameplay,
        scenario_id_to_entity_id,
        scenario_id_to_unit_id,
        initial_base_ids,
    }
}

fn representative_unit_id(world: &World, entity_id: EntityId) -> Option<EntityId> {
    if world.get_unit(entity_id).is_some() {
        return Some(entity_id);
    }
    world
        .get_squad(entity_id)
        .and_then(|squad| squad.unit_ids.first().copied())
}

fn configure_players(world: &mut World, players: &[ScenarioPlayer], db: &Database) {
    let default_difficulty = db
        .game_data
        .as_ref()
        .map_or(DEFAULT_PLAYER_DIFFICULTY, |data| {
            data.difficulty_default.unwrap_or(DEFAULT_PLAYER_DIFFICULTY)
        });
    let population_names = db
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.pops.as_ref())
        .map_or(&[][..], |pops| pops.entries.as_slice());
    let rate_count = db
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.rates.as_ref())
        .map_or(0, |rates| rates.entries.len());
    for player in world.players_mut() {
        player.difficulty = default_difficulty;
        player.configure_population_slots(population_names.len());
        player.configure_rate_slots(rate_count);
    }
    for (index, scenario_player) in players.iter().take(crate::world::MAX_PLAYERS).enumerate() {
        let player_id = u8::try_from(index + 1).unwrap_or(u8::MAX);
        let Some(player) = world.get_player_mut(player_id) else {
            continue;
        };
        player.name.clone_from(&scenario_player.name);
        player.civ_id = find_name_index(&db.civs, &scenario_player.civ, |civ| &civ.name);
        let leader_id =
            find_name_index(&db.leaders, &scenario_player.leader1, |leader| &leader.name);
        player.leader_id = leader_id;
        apply_leader_population(player, db, leader_id, population_names);
        resources::apply_leader_starting_resources(player, db, leader_id);
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
        player.initialize_resource_totals();
    }
    world.configure_standard_team_relations();
}

/// Apply a lobby-selected runtime leader and its authored population limits.
///
/// Skirmish SCN files contain player slots but generally leave civilization and
/// leader selection to the pregame lobby. Call this before placing lobby-owned
/// starting forces when `ScenarioPlayer/Leader1` did not provide the choice.
#[must_use]
pub fn configure_player_leader(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    leader_id: i32,
) -> bool {
    let Some(leader) = usize::try_from(leader_id)
        .ok()
        .and_then(|index| database.leaders.get(index))
    else {
        return false;
    };
    let population_names = database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.pops.as_ref())
        .map_or(&[][..], |pops| pops.entries.as_slice());
    let civilization_id = leader.civ.as_deref().map_or(-1, |civilization| {
        find_name_index(&database.civs, civilization, |entry| &entry.name)
    });
    let live_cap_additions = world
        .units
        .iter()
        .filter(|(_, unit)| unit.base.player_id == player_id && unit.built)
        .flat_map(|(_, unit)| unit.population_cap_additions.iter().copied())
        .collect::<Vec<_>>();
    let Some(player) = world.get_player_mut(player_id) else {
        return false;
    };
    player.configure_population_slots(population_names.len());
    for population_id in 0..player.population.len() {
        let _configured = player.set_population_limits(population_id, 0.0, 0.0);
    }
    player.leader_id = leader_id;
    player.civ_id = civilization_id;
    apply_leader_population(player, database, leader_id, population_names);
    resources::apply_leader_starting_resources(player, database, leader_id);
    player.adjust_population_cap(&live_cap_additions, true);
    true
}

fn apply_leader_population(
    player: &mut crate::player::Player,
    database: &Database,
    leader_id: i32,
    population_names: &[String],
) {
    let Some(leader) = usize::try_from(leader_id)
        .ok()
        .and_then(|index| database.leaders.get(index))
    else {
        return;
    };
    for population in &leader.pops {
        let Some(population_id) = population_names
            .iter()
            .position(|name| name.trim().eq_ignore_ascii_case(population.pop_type.trim()))
        else {
            continue;
        };
        let maximum = population.max.unwrap_or(population.count);
        let _configured = player.set_population_limits(population_id, population.count, maximum);
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
    placed::create_scenario_object(world, object, db)
}

fn create_scenario_squad(world: &mut World, object: &ScenarioObject, db: &Database) -> EntityId {
    let player_id = u8::try_from(object.player).unwrap_or_default();
    let position = scenario_position(object);
    create_squad_from_prototype(
        world,
        player_id,
        position,
        scenario_forward(object),
        object.proto_name.trim(),
        db,
    )
}

pub(crate) fn create_squad_from_prototype(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_name: &str,
    db: &Database,
) -> EntityId {
    let squad_id = world.create_squad_at(player_id, position);
    let logical_proto = find_proto_squad(db, proto_name);
    let effective_proto_name = world
        .get_player(player_id)
        .map_or(proto_name, |player| {
            player.technologies.resolved_squad_prototype(proto_name)
        })
        .to_owned();
    let proto = find_proto_squad(db, &effective_proto_name).or(logical_proto);
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad.base.set_forward(forward);
        squad.proto_squad_id =
            logical_proto.map_or(-1, |(index, squad)| database_id(squad.dbid, index));
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
            let stock = MarineSquadSpec::default();
            squad.formation = SquadFormation::Flock;
            squad.aggro_distance = stock.aggro_distance;
            squad.leash_distance = stock.leash_distance;
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
        if let Some(authored_aggro) =
            valid_nonnegative(proto.and_then(|(_, squad)| squad.aggro_distance))
        {
            squad.aggro_distance = authored_aggro;
        }
        if let Some(authored_leash) =
            valid_nonnegative(proto.and_then(|(_, squad)| squad.leash_distance))
        {
            squad.leash_distance = authored_leash;
        }
    }
    if let Some((_, proto)) = proto {
        create_squad_members(world, squad_id, proto, db);
        population::apply_squad_population(world, squad_id, db, proto);
    }
    refresh_squad_member_settings(world, squad_id);
    squad_id
}

fn create_squad_members(world: &mut World, squad_id: EntityId, proto: &ProtoSquad, db: &Database) {
    let Some(units) = &proto.units else {
        return;
    };
    for entry in &units.entries {
        for _ in 0..entry.count.max(0) {
            let _unit_id =
                add_squad_member_from_prototype(world, squad_id, entry.proto_object.trim(), db);
        }
    }
}

pub(crate) fn add_squad_member_from_prototype(
    world: &mut World,
    squad_id: EntityId,
    proto_object_name: &str,
    db: &Database,
) -> Option<EntityId> {
    let (player_id, position, archetype, slot) = world.get_squad(squad_id).map(|squad| {
        (
            squad.base.player_id,
            squad.base.position,
            squad.archetype,
            squad.unit_ids.len(),
        )
    })?;
    let unit_id = world.create_unit_at(player_id, position);
    configure_unit(world, unit_id, proto_object_name, db);
    if !world.attach_unit_to_squad(unit_id, squad_id) {
        let _removed = world.remove_unit(unit_id);
        return None;
    }
    if archetype == SquadArchetype::Marine
        && let Some(offset) = MarineSquadSpec::default().initial_formation_offset(slot)
    {
        let assigned = world.set_squad_member_formation_offset(unit_id, offset);
        debug_assert!(assigned, "attached Marine should accept a formation offset");
    }
    if let Some((_, prototype)) = find_proto_object(db, proto_object_name) {
        sockets::materialize_authored_sockets(world, unit_id, prototype, db);
    }
    Some(unit_id)
}

pub(crate) fn create_object_from_prototype(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_name: &str,
    db: &Database,
) -> Option<EntityId> {
    let (proto_index, proto) = find_proto_object(db, proto_name)?;
    let kind = classify_proto_object(proto)?;
    let unit_id = match kind {
        PlacedUnitKind::Mobile => world.create_unit_at(player_id, position),
        PlacedUnitKind::Building => world.create_building_at(player_id, position),
    };
    configure_unit_from_proto(world, unit_id, proto_name, proto_index, proto);
    population::apply_object_population(world, unit_id, db, proto);
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.base.set_forward(forward);
    }
    sockets::materialize_authored_sockets(world, unit_id, proto, db);
    if kind == PlacedUnitKind::Building && creates_base(proto) {
        let _base_id = world.register_base(unit_id);
    }
    Some(unit_id)
}

pub(crate) fn create_unbuilt_building_from_prototype(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_name: &str,
    db: &Database,
) -> Option<EntityId> {
    let (proto_index, proto) = find_proto_object(db, proto_name)?;
    if classify_proto_object(proto) != Some(PlacedUnitKind::Building) {
        return None;
    }
    let unit_id = world.create_building_at(player_id, position);
    configure_unit_from_proto(world, unit_id, proto_name, proto_index, proto);
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.built = false;
        unit.base.set_forward(forward);
    }
    sockets::materialize_authored_sockets(world, unit_id, proto, db);
    population::initialize_object_population(world, unit_id, db, proto, false);
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
    let shield_coverage = if proto
        .damage_type
        .as_deref()
        .is_some_and(|damage_type| damage_type.eq_ignore_ascii_case("Shielded"))
    {
        ShieldCoverage::Full
    } else {
        ShieldCoverage::None
    };
    let technologies = world
        .get_unit(unit_id)
        .and_then(|unit| world.get_player(unit.base.player_id))
        .map(|player| &player.technologies);
    let adjusted_hitpoints = proto
        .hitpoints
        .map(|base| technologies.map_or(base, |state| state.hitpoints(proto_name, base)));
    let base_shieldpoints = valid_nonnegative(proto.shieldpoints)
        .filter(|_| shield_coverage != ShieldCoverage::None)
        .unwrap_or_default();
    let shield_settings = technologies.map_or((base_shieldpoints, 1.0, 1.0), |state| {
        (
            state.shieldpoints(proto_name, base_shieldpoints),
            state.unit_shield_regen_rate(proto_name),
            state.unit_shield_regen_delay(proto_name),
        )
    });
    let adjusted_velocity = valid_nonnegative(proto.max_velocity.or(proto.velocity))
        .map(|base| technologies.map_or(base, |state| state.maximum_velocity(proto_name, base)));
    let Some(unit) = world.get_unit_mut(unit_id) else {
        return;
    };
    unit.proto_object_id = database_id(proto.dbid, proto_index);
    proto_name.clone_into(&mut unit.proto_object_name);
    let prototype_non_mobile = unit.is_building() || prototype_has_flag(proto, "Immoveable");
    unit.base.configure_prototype_mobility(prototype_non_mobile);
    unit.set_auto_attackable(!prototype_has_flag(proto, "DontAutoAttackMe"));
    garrison::configure_unit(unit, proto);
    if let Some(hitpoints) = adjusted_hitpoints {
        unit.set_max_hitpoints(hitpoints);
    }
    unit.shields.configure(shield_coverage, shield_settings.0);
    unit.shields
        .set_regen_scalars(shield_settings.1, shield_settings.2);
    if !unit.is_building()
        && let Some(speed) = adjusted_velocity
    {
        unit.speed = speed;
    }
    unit.acceleration = valid_nonnegative(proto.acceleration).unwrap_or_default();
    unit.turn_rate_degrees = valid_nonnegative(proto.turn_rate).unwrap_or_default();
    unit.obstruction_half_extents = obstruction_half_extents(proto).unwrap_or(Vec3::ZERO);
    if is_warthog_unit(proto_name, proto.physics_info.as_deref()) {
        configure_warthog(unit, warthog_spec_from_proto(proto, adjusted_velocity));
    } else if is_marine_unit(proto_name) {
        configure_marine(unit, marine_spec_from_proto(proto, adjusted_velocity));
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

fn warthog_spec_from_proto(proto: &ProtoObject, max_speed: Option<f32>) -> WarthogUnitSpec {
    let mut spec = WarthogUnitSpec::default();
    spec.max_speed = max_speed.unwrap_or(spec.max_speed);
    spec.acceleration = valid_nonnegative(proto.acceleration).unwrap_or(spec.acceleration);
    spec.turn_rate_degrees = valid_nonnegative(proto.turn_rate).unwrap_or(spec.turn_rate_degrees);
    spec.half_extents.x = valid_positive(proto.obstruction_radius_x).unwrap_or(spec.half_extents.x);
    spec.half_extents.y = valid_positive(proto.obstruction_radius_y).unwrap_or(spec.half_extents.y);
    spec.half_extents.z = valid_positive(proto.obstruction_radius_z).unwrap_or(spec.half_extents.z);
    spec
}

fn marine_spec_from_proto(proto: &ProtoObject, max_speed: Option<f32>) -> MarineUnitSpec {
    let mut spec = MarineUnitSpec::default();
    spec.max_speed = max_speed.unwrap_or(spec.max_speed);
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
    let prototype_non_mobile = !first.base.is_ever_mobile();
    for member in members {
        speed = speed.min(member.speed);
        acceleration = acceleration.min(member.acceleration);
        turn_rate_degrees = turn_rate_degrees.min(member.turn_rate_degrees);
    }
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad
            .base
            .configure_prototype_mobility(prototype_non_mobile);
        squad.speed = speed;
        squad.acceleration = acceleration;
        squad.turn_rate_degrees = turn_rate_degrees;
    }
}

pub(crate) fn refresh_squad_member_settings(world: &mut World, squad_id: EntityId) {
    apply_squad_member_movement_settings(world, squad_id);
    apply_squad_physics_settings(world, squad_id);
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
mod tests;
