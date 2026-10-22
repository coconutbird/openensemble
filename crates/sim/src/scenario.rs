//! Scenario world loading for Halo Wars maps.
//!
//! This module loads scenario data into the simulation world like `BScenario_load` @
//! `0x140377BE0`. The `pipeline` crate parses the scenario data. This module creates the
//! players, entities and trigger scripts in the engine order:
//!
//! 1. The `.scn` root elements in document order: `Players` (player setup), `Objects`,
//!    each `TriggerSystem` and each `ExternalTriggerScript`.
//! 2. The `.sc2` objects, then the `.sc3` objects.
//! 3. The starting units of the players.
//! 4. The simulation bounds.
//! 5. The game mode of a skirmish: its NPC player and tech, the world script and one player
//!    script for each human and computer AI player (`game_mode`).
//! 6. The skirmish AI script of each computer AI player (`player_ais`).
//!
//! # Example
//!
//! ```ignore
//! use pipeline::hw1::scenario::ScenarioData;
//! use pipeline::database::hw1::Database;
//! use sim::{GameSettings, load_scenario_into_world};
//!
//! let scenario = ScenarioData::default();
//! let db = Database::new();
//! let loaded = load_scenario_into_world(&scenario, &db, &GameSettings::skirmish_1v1(0))?;
//! println!("Created {} players", loaded.world.player_count());
//! ```

use crate::entities::squads::marine::{MarineSquadSpec, is_marine_squad};
use crate::entities::squads::warthog::{WarthogSquadSpec, is_warthog_squad};
use crate::entities::{BaseId, SquadArchetype, SquadFormation};
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;
use crate::world::World;
use glam::Vec3;
#[cfg(test)]
use pipeline::database::hw1::ProtoObject;
use pipeline::database::hw1::{Database, Squad as ProtoSquad};
use pipeline::hw1::scenario::{ScenarioLoadStep, scenario_load_steps};
use pipeline::source::{AssetSource, StdFileProvider};
use std::collections::{BTreeMap, BTreeSet, HashMap};

mod garrison;

mod animation_requests;
pub(crate) mod child_objects;
mod design_objects;
mod forbids;
mod game_mode;
mod game_settings;
mod objectives;
mod objects;
pub(crate) mod parking_lots;
pub(crate) mod placed;
mod placement;
mod player_ais;
mod player_setup;
pub(crate) mod population;
mod prototypes;
mod resources;
mod settings;
mod starting_units;
mod technology;
mod triggers;
mod units;

pub(crate) use units::{
    add_squad_member_from_prototype, configure_unit_from_player_proto, configure_unit_from_proto,
    create_object_from_prototype, create_unbuilt_building_from_prototype,
    create_unit_squad_from_prototype,
};

pub use game_settings::{GameSettings, PlayerSettings, SKIRMISH_GAME_MODE, SlotType};
pub use pipeline::hw1::scenario::{ScenarioData, ScenarioObject, ScenarioPlayer, ScenarioPosition};
pub use player_setup::ScenarioSetupError;
pub(crate) use prototypes::{PlacedUnitKind, classify_proto_object};
use prototypes::{creates_base, find_proto_object, find_proto_squad, is_class_zero_object};

use objects::{StartMarkers, VisualVariations};
use player_setup::PlayerSetup;

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
    /// Base that the starting units of each player created (the base of its starting HQ).
    pub initial_base_ids: BTreeMap<PlayerId, BaseId>,
    /// World player of each scenario player number, which trigger `Player` values read.
    scenario_players: [Option<PlayerId>; player_setup::OWNER_SLOTS],
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
    /// The scenario players could not be set up.
    #[error("failed to set up the scenario players: {0}")]
    Setup(#[from] ScenarioSetupError),
    /// A scenario trigger system was malformed.
    #[error("failed to load a scenario trigger system: {0}")]
    Trigger(#[from] crate::trigger::LoadError),
    /// The scenario's synchronized XSD simulation terrain was malformed.
    #[error("failed to load scenario simulation terrain: {0}")]
    Terrain(#[from] pipeline::xsd::Error),
    /// Scenario-authored design spheres or lines were malformed.
    #[error("failed to load scenario design objects: {0}")]
    DesignObjects(#[from] pipeline::hw1::scenario::DesignObjectError),
    /// Scenario-authored objective state was malformed.
    #[error("failed to load scenario objectives: {0}")]
    Objectives(#[from] pipeline::hw1::scenario::ObjectiveError),
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

    /// Get the base that the starting units of a player created.
    #[must_use]
    pub fn get_initial_base_id(&self, player_id: PlayerId) -> Option<BaseId> {
        self.initial_base_ids.get(&player_id).copied()
    }

    /// Get the world player of a `.scn` player number, or `None` for a number that names no
    /// world player (for example the co-op slot of a single-player campaign).
    #[must_use]
    pub fn world_player(&self, scenario_player: i32) -> Option<PlayerId> {
        usize::try_from(scenario_player)
            .ok()
            .and_then(|index| self.scenario_players.get(index).copied().flatten())
    }
}

/// Load a scenario ERA, its database files, and its authoritative sim state.
///
/// The scenario archive is layered onto the base-game archives *before* the
/// pipeline database is parsed. Consequently, database files supplied by the
/// scenario participate in normal last-loaded-wins asset resolution. The same
/// parsed database then creates the simulation entities, while the returned
/// pipeline content remains available to presentation systems. `settings` is the lobby
/// input of the game.
///
/// # Errors
///
/// Returns an error when the scenario archive cannot be found, required
/// database files cannot be parsed, the archive contains no resolvable SCN, or the
/// scenario players cannot be set up.
pub fn load_scenario_from_game_dir(
    game_dir: &str,
    scenario: &str,
    settings: &GameSettings,
) -> Result<LoadedGameScenario, ScenarioAssetLoadError> {
    let mut source = pipeline::hw1::loader::load_game_dir(game_dir);
    let config_symbols = pipeline::hw1::startup::load_startup_config(&mut source);
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
    let database = &content.database;
    let scenario_path = content
        .scenario
        .as_ref()
        .map(pipeline::hw1::scenario::ScenarioDescriptor::scn_path)
        .ok_or_else(|| ScenarioAssetLoadError::ScenarioDataNotFound {
            scenario: scenario.to_owned(),
        })?;
    let scenario_document = source.read_xmb(&scenario_path).ok_or_else(|| {
        ScenarioAssetLoadError::ScenarioDefinitionNotFound {
            path: scenario_path.clone(),
        }
    })?;
    let visual_variation_indices = settings::visual_variation_indices(scenario_data.objects());
    let mut builder = ScenarioBuilder::new(
        scenario_data,
        database,
        settings,
        WorldSetup {
            gameplay: GameplayCatalog::load_from_source(database, &mut source),
            allows_veterancy: scenario_data.allow_veterancy,
            config_symbols: Some(&config_symbols),
        },
    );
    let world = &mut builder.loaded.world;
    if let Some(terrain) = &content.terrain_data {
        let minimum = Vec3::from_array(terrain.header.world_min);
        let maximum = Vec3::from_array(terrain.header.world_max);
        let _configured = world.configure_terrain_bounds(minimum, maximum);
    }
    if let Some(descriptor) = &content.scenario
        && let Some(terrain) = descriptor.read_simulation_terrain(&mut source)?
    {
        world.configure_terrain_simulation(terrain);
    }
    let root = scenario_document
        .root()
        .ok_or_else(|| crate::trigger::LoadError::MissingElement("Scenario root".into()))?;
    for step in scenario_load_steps(root) {
        match step {
            ScenarioLoadStep::Players => builder.players()?,
            ScenarioLoadStep::Objects => builder.objects(
                scenario_data.objects(),
                VisualVariations::ById(&visual_variation_indices),
            ),
            ScenarioLoadStep::TriggerSystem(node) => {
                triggers::create_trigger_system(&mut builder.loaded, database, node)?;
            }
            ScenarioLoadStep::ExternalTriggerScript(file_name) => {
                let _script = builder.loaded.launch_trigger_script(
                    &mut source,
                    database,
                    &file_name,
                    crate::trigger::TriggerExternals::default(),
                )?;
            }
        }
    }
    for file in [&content.scenario_sc2, &content.scenario_sc3]
        .into_iter()
        .flatten()
    {
        builder.objects(file.objects(), VisualVariations::Authored);
    }
    let mut simulation = builder.finish();
    let objectives = pipeline::hw1::scenario::parse_objectives(&scenario_document)?;
    objectives::load_objectives(&mut simulation.world, &objectives);
    let design_objects = pipeline::hw1::scenario::parse_design_objects(&scenario_document)?;
    design_objects::load_design_objects(&mut simulation.world, &design_objects);
    let mut load_script = |scenario: &mut LoadedScenario, file_name: &str| {
        scenario.load_trigger_script(&mut source, database, file_name)
    };
    game_mode::start_game_mode(
        &mut simulation,
        database,
        &settings.game_mode,
        &mut load_script,
    )?;
    player_ais::start_skirmish_ai_scripts(&mut simulation, database, &mut load_script)?;
    triggers::preload_trigger_scripts(&mut simulation, &mut source, database);
    let animation_requests = triggers::scripted_animation_requests(&simulation)
        .into_iter()
        .chain(animation_requests::trained_birth_animation_requests(
            database,
        ))
        .chain(animation_requests::squad_mode_animation_requests(database))
        .chain(simulation.gameplay.energy_shield_animation_requests());
    simulation
        .gameplay
        .load_scripted_animation_requests(database, &mut source, animation_requests);

    Ok(LoadedGameScenario {
        simulation,
        content,
        source,
    })
}

/// Load a scenario into a new [`World`], creating players and entities.
///
/// The load runs the `Players` and `Objects` steps of [`load_scenario_from_game_dir`] and
/// the starting units, without trigger scripts, terrain or `.sc2` and `.sc3` objects.
///
/// # Errors
///
/// Returns an error if the scenario players cannot be set up.
pub fn load_scenario_into_world(
    scenario: &ScenarioData,
    db: &Database,
    settings: &GameSettings,
) -> Result<LoadedScenario, ScenarioSetupError> {
    let mut builder = ScenarioBuilder::new(
        scenario,
        db,
        settings,
        WorldSetup {
            gameplay: GameplayCatalog::default(),
            allows_veterancy: true,
            config_symbols: None,
        },
    );
    builder.players()?;
    builder.objects(scenario.objects(), VisualVariations::Authored);
    Ok(builder.finish())
}

/// World options that the scenario load applies before it creates entities.
struct WorldSetup<'a> {
    gameplay: GameplayCatalog,
    allows_veterancy: bool,
    config_symbols: Option<&'a BTreeSet<String>>,
}

/// State of one scenario load between its steps.
struct ScenarioBuilder<'a> {
    loaded: LoadedScenario,
    scenario: &'a ScenarioData,
    database: &'a Database,
    settings: &'a GameSettings,
    setup: PlayerSetup,
    markers: StartMarkers,
}

impl<'a> ScenarioBuilder<'a> {
    fn new(
        scenario: &'a ScenarioData,
        database: &'a Database,
        settings: &'a GameSettings,
        options: WorldSetup<'_>,
    ) -> Self {
        let mut world = World::new();
        if let Some(config_symbols) = options.config_symbols {
            world.configure_startup_configs(config_symbols.iter().map(String::as_str));
        }
        world.seed_sim_random(settings.random_seed);
        world.set_game_type(settings.game_type);
        world.set_game_mode(
            database
                .game_modes
                .iter()
                .find(|mode| mode.name.eq_ignore_ascii_case(&settings.game_mode))
                .map(|mode| mode.name.clone()),
        );
        world.set_scenario_allows_veterancy(options.allows_veterancy);
        world.configure_prototype_catalogs(database);
        world.configure_prototype_damage_profiles(&options.gameplay);
        world.configure_prototype_vehicle_physics(&options.gameplay);
        world.set_construction_damage_multiplier(
            database
                .game_data
                .as_ref()
                .and_then(|game_data| game_data.construction_damage_multiplier),
        );
        world.set_game_over_delay(
            database
                .game_data
                .as_ref()
                .and_then(|game_data| game_data.game_over_delay),
        );
        world.init_players(0);
        objectives::configure_objective_references(&mut world, scenario.objectives());
        Self {
            loaded: LoadedScenario {
                world,
                gameplay: options.gameplay,
                scenario_id_to_entity_id: HashMap::new(),
                scenario_id_to_unit_id: HashMap::new(),
                initial_base_ids: BTreeMap::new(),
                scenario_players: PlayerSetup::default().owners(),
            },
            scenario,
            database,
            settings,
            setup: PlayerSetup::default(),
            markers: StartMarkers::default(),
        }
    }

    /// The `Players` step: `BScenario_setupPlayers` and the player forbids.
    fn players(&mut self) -> Result<(), ScenarioSetupError> {
        let world = &mut self.loaded.world;
        self.setup =
            player_setup::setup_players(world, self.scenario, self.database, self.settings)?;
        forbids::apply_scenario_forbids(world, self.database, self.scenario.players(), &self.setup);
        self.loaded.scenario_players = self.setup.owners();
        Ok(())
    }

    /// An object list: `.scn` `Objects`, `.sc2` or `.sc3`.
    fn objects(&mut self, objects: &[ScenarioObject], visual_variations: VisualVariations<'_>) {
        objects::load_objects(
            &mut self.loaded,
            objects,
            self.database,
            &self.setup,
            &mut self.markers,
            visual_variations,
        );
    }

    /// The starting units (step `CC`) and the simulation bounds (step `DD`).
    fn finish(mut self) -> LoadedScenario {
        let world = &mut self.loaded.world;
        self.loaded.initial_base_ids = starting_units::create_starting_units(
            world,
            self.database,
            self.scenario,
            &self.setup,
            &self.markers,
        );
        set_sim_bounds(world, self.scenario);
        world.configure_unit_revivals(&self.loaded.gameplay);
        self.loaded
    }
}

/// Apply the top-level `SimBounds*` elements like `BWorld_setSimBounds` @ `0x1405D2210`.
///
/// The engine clamps the bounds to the terrain. A world without terrain keeps no bounds.
fn set_sim_bounds(world: &mut World, scenario: &ScenarioData) {
    if world.terrain_bounds().is_none() {
        return;
    }
    let [min_x, min_z, max_x, max_z] = scenario.sim_bounds();
    let first = Vec3::from_array(scenario_object_position_to_world([min_x, 0.0, min_z]));
    let second = Vec3::from_array(scenario_object_position_to_world([max_x, 0.0, max_z]));
    let _set = world.set_playable_bounds(first, second);
}

fn apply_scenario_visual_variation(world: &mut World, entity_id: EntityId, index: i32) {
    if let Some(object) = world.get_object_mut(entity_id) {
        object.object_state.set_visual_variation_index(index);
        return;
    }
    if let Some(unit) = world.get_unit_mut(entity_id) {
        unit.object_state.set_visual_variation_index(index);
        return;
    }
    let member_ids = world
        .get_squad(entity_id)
        .map(|squad| squad.unit_ids.clone())
        .unwrap_or_default();
    for member_id in member_ids {
        if let Some(unit) = world.get_unit_mut(member_id) {
            unit.object_state.set_visual_variation_index(index);
        }
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

/// Copy the population counts and limits of a leader to a player (`BPlayer_setup`).
fn apply_leader_population(
    player: &mut crate::player::Player,
    database: &Database,
    leader_id: i32,
) {
    let Some(leader) = usize::try_from(leader_id)
        .ok()
        .and_then(|index| database.leaders.get(index))
    else {
        return;
    };
    let population_names = database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.pops.as_ref())
        .map_or(&[][..], |pops| pops.entries.as_slice());
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

pub(crate) fn create_squad_from_prototype(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_name: &str,
    db: &Database,
) -> EntityId {
    create_squad_from_prototype_internal(world, player_id, position, forward, proto_name, db, true)
}

pub(crate) fn create_empty_squad_from_prototype(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_name: &str,
    db: &Database,
) -> EntityId {
    create_squad_from_prototype_internal(world, player_id, position, forward, proto_name, db, false)
}

fn create_squad_from_prototype_internal(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_name: &str,
    db: &Database,
    create_members: bool,
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
    let veterancy_level = world.veterancy_enabled().then(|| {
        proto
            .and_then(|(_, prototype)| prototype.level)
            .unwrap_or_default()
    });
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad.base.set_forward(forward);
        squad.proto_squad_id =
            logical_proto.map_or(-1, |(index, _)| crate::prototypes::proto_id(index));
        proto_name.clone_into(&mut squad.proto_squad_name);
        squad.set_veterancy_level(veterancy_level.unwrap_or_default());
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
            squad.configure_leash_profile(stock.leash_deadzone, stock.leash_recall_delay_ms);
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
        let leash_deadzone =
            valid_nonnegative(proto.and_then(|(_, prototype)| prototype.leash_deadzone))
                .unwrap_or_else(|| squad.leash_deadzone());
        let leash_recall_delay = proto
            .and_then(|(_, prototype)| prototype.leash_recall_delay)
            .unwrap_or_else(|| squad.leash_recall_delay_ms());
        squad.configure_leash_profile(leash_deadzone, leash_recall_delay);
    }
    if let Some((_, proto)) = proto {
        if create_members {
            create_squad_members(world, squad_id, proto, db);
        }
        population::apply_squad_population(world, squad_id, db, proto);
    }
    world.refresh_squad_ammunition(squad_id, db);
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
    world.refresh_squad_ai_membership(squad_id);
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

fn valid_nonnegative(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

/// Convert an authored scenario position into canonical terrain-world axes.
///
/// `BScenario_loadPosition` @ `0x14037F430` and `BScenario_loadObject` @ `0x14037D6A0` read
/// start positions and objects in one frame (`sub_1406217A0`). The simulation swaps the X
/// and Z axes of that frame for both.
#[must_use]
pub const fn scenario_object_position_to_world(position: [f32; 3]) -> [f32; 3] {
    [position[2], position[1], position[0]]
}

/// Convert an authored scenario direction into canonical terrain-world axes.
#[must_use]
pub const fn scenario_object_direction_to_world(direction: [f32; 3]) -> [f32; 3] {
    scenario_object_position_to_world(direction)
}

/// Load a test scenario with eight default lobby slots, so that each controllable player
/// is active and keeps its `.scn` civ, leader and team.
#[cfg(test)]
pub(crate) fn load_test_scenario(scenario: &ScenarioData, database: &Database) -> LoadedScenario {
    let settings = GameSettings::skirmish(vec![PlayerSettings::default(); 8], 0);
    load_scenario_into_world(scenario, database, &settings).expect("test players set up")
}

#[cfg(test)]
mod tests;
