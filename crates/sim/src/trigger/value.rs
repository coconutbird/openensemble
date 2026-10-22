//! Trigger variable values - type-safe storage for trigger data.

use crate::entity::Entity;
use crate::entity_id::EntityClass;
use crate::sync::SyncChecksum;
use crate::{EntityId, World};

use super::VarId;

/// Ordered runtime predicates used by retail entity-list filter effects.
///
/// Filters are appended by the `EntityFilterAdd*` effects and combined with
/// logical AND in insertion order. Payload lists are copied when a predicate
/// is appended, matching retail's owned filter objects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntityFilterSet {
    filters: Vec<EntityFilterPredicate>,
}

impl EntityFilterSet {
    /// Return the number of predicates currently attached to this set.
    #[must_use]
    pub fn filter_count(&self) -> usize {
        self.filters.len()
    }

    pub(crate) fn clear(&mut self) {
        self.filters.clear();
    }

    pub(crate) fn push(&mut self, predicate: EntityFilterPredicate) {
        self.filters.push(predicate);
    }

    /// Test one live entity against every appended retail predicate.
    pub(crate) fn matches_entity(&self, entity_id: EntityId, world: &World) -> bool {
        match entity_id.class() {
            Some(EntityClass::Object) => self.matches_object(entity_id, world),
            Some(EntityClass::Unit) => self.matches_unit(entity_id, world),
            Some(EntityClass::Squad) => self.matches_squad(entity_id, world),
            Some(EntityClass::Projectile) => self.matches_projectile(entity_id, world),
            _ => false,
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.filters.len()).unwrap_or(u32::MAX));
        for predicate in &self.filters {
            predicate.hash_state(checksum);
        }
    }

    fn matches_object(&self, entity_id: EntityId, world: &World) -> bool {
        let Some(object) = world.get_object(entity_id) else {
            return false;
        };
        self.filters.iter().all(|predicate| {
            let (matches, invert) = match predicate {
                EntityFilterPredicate::IsAlive { invert } => (object.is_alive(), *invert),
                EntityFilterPredicate::IsIdle { invert }
                | EntityFilterPredicate::ProtoSquads { invert, .. } => (false, *invert),
                EntityFilterPredicate::InList { invert, entities } => {
                    (entities.contains(&entity_id), *invert)
                }
                EntityFilterPredicate::Players { invert, players } => {
                    (players.contains(&i32::from(object.base.player_id)), *invert)
                }
                EntityFilterPredicate::Teams { invert, teams } => (
                    world
                        .get_player(object.base.player_id)
                        .is_some_and(|player| teams.contains(&i32::from(player.team_id))),
                    *invert,
                ),
                EntityFilterPredicate::ProtoObjects { invert, prototypes } => {
                    (prototypes.contains(&object.proto_object_id), *invert)
                }
                EntityFilterPredicate::ObjectTypes {
                    invert,
                    object_types,
                } => (
                    object_types.iter().any(|object_type| {
                        world.entity_object_type_match(entity_id, object_type) == Some(true)
                    }),
                    *invert,
                ),
                EntityFilterPredicate::Diplomacy {
                    invert,
                    relation_type,
                    reference_team,
                } => (
                    diplomacy_matches(
                        world,
                        object.base.player_id,
                        *reference_team,
                        *relation_type,
                    ),
                    *invert,
                ),
            };
            matches != invert
        })
    }

    fn matches_unit(&self, entity_id: EntityId, world: &World) -> bool {
        let Some(unit) = world.get_unit(entity_id) else {
            return false;
        };
        self.filters.iter().all(|predicate| {
            let (matches, invert) = match predicate {
                EntityFilterPredicate::IsAlive { invert } => {
                    (world.is_entity_trigger_alive(entity_id), *invert)
                }
                EntityFilterPredicate::IsIdle { invert } => (unit.has_idle_action(), *invert),
                EntityFilterPredicate::InList { invert, entities } => {
                    (entities.contains(&entity_id), *invert)
                }
                EntityFilterPredicate::Players { invert, players } => {
                    (players.contains(&i32::from(unit.base.player_id)), *invert)
                }
                EntityFilterPredicate::Teams { invert, teams } => (
                    world
                        .get_player(unit.base.player_id)
                        .is_some_and(|player| teams.contains(&i32::from(player.team_id))),
                    *invert,
                ),
                EntityFilterPredicate::ProtoObjects { invert, prototypes } => {
                    (prototypes.contains(&unit.proto_object_id), *invert)
                }
                EntityFilterPredicate::ProtoSquads { invert, prototypes } => (
                    unit.squad_id
                        .and_then(|squad_id| world.get_squad(squad_id))
                        .is_some_and(|squad| prototypes.contains(&squad.proto_squad_id)),
                    *invert,
                ),
                EntityFilterPredicate::ObjectTypes {
                    invert,
                    object_types,
                } => (
                    object_types
                        .iter()
                        .any(|object_type| unit.is_object_type(object_type)),
                    *invert,
                ),
                EntityFilterPredicate::Diplomacy {
                    invert,
                    relation_type,
                    reference_team,
                } => (
                    diplomacy_matches(world, unit.base.player_id, *reference_team, *relation_type),
                    *invert,
                ),
            };
            matches != invert
        })
    }

    fn matches_squad(&self, entity_id: EntityId, world: &World) -> bool {
        let Some(squad) = world.get_squad(entity_id) else {
            return false;
        };
        self.filters.iter().all(|predicate| {
            let (matches, invert) = match predicate {
                EntityFilterPredicate::IsAlive { invert } => {
                    (world.is_entity_trigger_alive(entity_id), *invert)
                }
                EntityFilterPredicate::IsIdle { invert } => (
                    squad.unit_ids.iter().all(|unit_id| {
                        world
                            .get_unit(*unit_id)
                            .is_none_or(crate::entities::Unit::has_idle_action)
                    }),
                    *invert,
                ),
                EntityFilterPredicate::InList { invert, entities } => {
                    (entities.contains(&entity_id), *invert)
                }
                EntityFilterPredicate::Players { invert, players } => {
                    (players.contains(&i32::from(squad.base.player_id)), *invert)
                }
                EntityFilterPredicate::Teams { invert, teams } => (
                    world
                        .get_player(squad.base.player_id)
                        .is_some_and(|player| teams.contains(&i32::from(player.team_id))),
                    *invert,
                ),
                EntityFilterPredicate::ProtoObjects { invert, prototypes } => (
                    squad.unit_ids.iter().all(|unit_id| {
                        world
                            .get_unit(*unit_id)
                            .is_none_or(|unit| prototypes.contains(&unit.proto_object_id))
                    }),
                    *invert,
                ),
                EntityFilterPredicate::ProtoSquads { invert, prototypes } => {
                    (prototypes.contains(&squad.proto_squad_id), *invert)
                }
                EntityFilterPredicate::ObjectTypes {
                    invert,
                    object_types,
                } => (
                    squad.unit_ids.iter().all(|unit_id| {
                        world.get_unit(*unit_id).is_none_or(|unit| {
                            object_types
                                .iter()
                                .any(|object_type| unit.is_object_type(object_type))
                        })
                    }),
                    *invert,
                ),
                EntityFilterPredicate::Diplomacy {
                    invert,
                    relation_type,
                    reference_team,
                } => (
                    diplomacy_matches(world, squad.base.player_id, *reference_team, *relation_type),
                    *invert,
                ),
            };
            matches != invert
        })
    }

    fn matches_projectile(&self, entity_id: EntityId, world: &World) -> bool {
        let Some(projectile) = world.get_projectile(entity_id) else {
            return false;
        };
        self.filters.iter().all(|predicate| {
            let (matches, invert) = match predicate {
                EntityFilterPredicate::IsAlive { invert } => (projectile.base.alive, *invert),
                EntityFilterPredicate::InList { invert, entities } => {
                    (entities.contains(&entity_id), *invert)
                }
                EntityFilterPredicate::Players { invert, players } => (
                    players.contains(&i32::from(projectile.base.player_id)),
                    *invert,
                ),
                EntityFilterPredicate::Teams { invert, teams } => (
                    world
                        .get_player(projectile.base.player_id)
                        .is_some_and(|player| teams.contains(&i32::from(player.team_id))),
                    *invert,
                ),
                EntityFilterPredicate::Diplomacy {
                    invert,
                    relation_type,
                    reference_team,
                } => (
                    diplomacy_matches(
                        world,
                        projectile.base.player_id,
                        *reference_team,
                        *relation_type,
                    ),
                    *invert,
                ),
                EntityFilterPredicate::IsIdle { invert }
                | EntityFilterPredicate::ProtoObjects { invert, .. }
                | EntityFilterPredicate::ProtoSquads { invert, .. }
                | EntityFilterPredicate::ObjectTypes { invert, .. } => (false, *invert),
            };
            matches != invert
        })
    }
}

fn diplomacy_matches(world: &World, owner_id: u8, reference_team: i32, relation_type: i32) -> bool {
    let entity_team = world
        .get_player(owner_id)
        .map_or(-1, |player| i32::from(player.team_id));
    let actual = u8::try_from(entity_team)
        .ok()
        .zip(u8::try_from(reference_team).ok())
        .map_or(4, |(entity_team, reference_team)| {
            i32::from(world.team_relation(entity_team, reference_team) as u8)
        });
    actual == relation_type
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntityFilterPredicate {
    IsAlive {
        invert: bool,
    },
    IsIdle {
        invert: bool,
    },
    InList {
        invert: bool,
        entities: Vec<EntityId>,
    },
    Players {
        invert: bool,
        players: Vec<PlayerId>,
    },
    Teams {
        invert: bool,
        teams: Vec<TeamId>,
    },
    ProtoObjects {
        invert: bool,
        prototypes: Vec<ProtoObjectId>,
    },
    ProtoSquads {
        invert: bool,
        prototypes: Vec<ProtoSquadId>,
    },
    ObjectTypes {
        invert: bool,
        object_types: Vec<String>,
    },
    Diplomacy {
        invert: bool,
        relation_type: i32,
        reference_team: i32,
    },
}

impl EntityFilterPredicate {
    fn hash_state(&self, checksum: &mut SyncChecksum) {
        match self {
            Self::IsAlive { invert } => hash_filter_header(checksum, 0, *invert),
            Self::IsIdle { invert } => hash_filter_header(checksum, 7, *invert),
            Self::InList { invert, entities } => {
                hash_filter_header(checksum, 1, *invert);
                checksum.hash_u32(u32::try_from(entities.len()).unwrap_or(u32::MAX));
                for entity in entities {
                    checksum.hash_u32(entity.as_u32());
                }
            }
            Self::Players { invert, players } => {
                hash_filter_header(checksum, 2, *invert);
                hash_i32_values(checksum, players);
            }
            Self::Teams { invert, teams } => {
                hash_filter_header(checksum, 3, *invert);
                hash_i32_values(checksum, teams);
            }
            Self::ProtoObjects { invert, prototypes } => {
                hash_filter_header(checksum, 4, *invert);
                hash_i32_values(checksum, prototypes);
            }
            Self::ProtoSquads { invert, prototypes } => {
                hash_filter_header(checksum, 5, *invert);
                hash_i32_values(checksum, prototypes);
            }
            Self::ObjectTypes {
                invert,
                object_types,
            } => {
                hash_filter_header(checksum, 6, *invert);
                checksum.hash_u32(u32::try_from(object_types.len()).unwrap_or(u32::MAX));
                for object_type in object_types {
                    checksum.hash_u32(u32::try_from(object_type.len()).unwrap_or(u32::MAX));
                    checksum.hash_bytes(object_type.as_bytes());
                }
            }
            Self::Diplomacy {
                invert,
                relation_type,
                reference_team,
            } => {
                hash_filter_header(checksum, 8, *invert);
                checksum.hash_i32(*relation_type);
                checksum.hash_i32(*reference_team);
            }
        }
    }
}

fn hash_filter_header(checksum: &mut SyncChecksum, predicate_type: u32, invert: bool) {
    checksum.hash_u32(predicate_type);
    checksum.hash_u32(u32::from(invert));
}

fn hash_i32_values(checksum: &mut SyncChecksum, values: &[i32]) {
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_i32(*value);
    }
}

/// Runtime state for a retail trigger-list iterator.
///
/// Iterator effects attach this value to a list variable and reset its visited
/// sets. `NextPlayer`, `NextTeam`, `NextUnit`, and `NextSquad` conditions then
/// consume the first current list member that has not yet been visited.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TriggerIterator {
    source_list_id: Option<VarId>,
    visited_players: Vec<PlayerId>,
    visited_teams: Vec<TeamId>,
    visited_units: Vec<EntityId>,
    visited_squads: Vec<EntityId>,
    visited_objects: Vec<EntityId>,
    visited_vectors: Vec<Vec3>,
}

impl TriggerIterator {
    /// Return the ID of the list variable that this iterator walks, if any.
    #[must_use]
    pub fn source_list_id(&self) -> Option<VarId> {
        self.source_list_id
    }

    pub(crate) fn attach(&mut self, source_list_id: VarId) {
        self.source_list_id = Some(source_list_id);
        self.visited_players.clear();
        self.visited_teams.clear();
        self.visited_units.clear();
        self.visited_squads.clear();
        self.visited_objects.clear();
        self.visited_vectors.clear();
    }

    pub(crate) fn is_player_visited(&self, player_id: PlayerId) -> bool {
        self.visited_players.contains(&player_id)
    }

    /// Return the number of players that the iterator visited.
    #[must_use]
    pub fn visited_player_count(&self) -> usize {
        self.visited_players.len()
    }

    pub(crate) fn visit_player(&mut self, player_id: PlayerId) {
        if !self.is_player_visited(player_id) {
            self.visited_players.push(player_id);
        }
    }

    pub(crate) fn is_team_visited(&self, team_id: TeamId) -> bool {
        self.visited_teams.contains(&team_id)
    }

    /// Return the number of teams that the iterator visited.
    #[must_use]
    pub fn visited_team_count(&self) -> usize {
        self.visited_teams.len()
    }

    pub(crate) fn visit_team(&mut self, team_id: TeamId) {
        if !self.is_team_visited(team_id) {
            self.visited_teams.push(team_id);
        }
    }

    pub(crate) fn is_unit_visited(&self, unit_id: EntityId) -> bool {
        self.visited_units.contains(&unit_id)
    }

    /// Return the number of units that the iterator visited.
    #[must_use]
    pub fn visited_unit_count(&self) -> usize {
        self.visited_units.len()
    }

    pub(crate) fn visit_unit(&mut self, unit_id: EntityId) {
        if !self.is_unit_visited(unit_id) {
            self.visited_units.push(unit_id);
        }
    }

    pub(crate) fn is_squad_visited(&self, squad_id: EntityId) -> bool {
        self.visited_squads.contains(&squad_id)
    }

    /// Return the number of squads that the iterator visited.
    #[must_use]
    pub fn visited_squad_count(&self) -> usize {
        self.visited_squads.len()
    }

    pub(crate) fn visit_squad(&mut self, squad_id: EntityId) {
        if !self.is_squad_visited(squad_id) {
            self.visited_squads.push(squad_id);
        }
    }

    pub(crate) fn visited_units(&self) -> &[EntityId] {
        &self.visited_units
    }

    pub(crate) fn visited_squads(&self) -> &[EntityId] {
        &self.visited_squads
    }

    pub(crate) fn is_object_visited(&self, object_id: EntityId) -> bool {
        self.visited_objects.contains(&object_id)
    }

    /// Return the number of objects that the iterator visited.
    #[must_use]
    pub fn visited_object_count(&self) -> usize {
        self.visited_objects.len()
    }

    pub(crate) fn visit_object(&mut self, object_id: EntityId) {
        if !self.is_object_visited(object_id) {
            self.visited_objects.push(object_id);
        }
    }

    pub(crate) fn is_vector_visited(&self, vector: Vec3) -> bool {
        self.visited_vectors.contains(&vector)
    }

    /// Return the number of vectors that the iterator visited.
    #[must_use]
    pub fn visited_vector_count(&self) -> usize {
        self.visited_vectors.len()
    }

    pub(crate) fn visit_vector(&mut self, vector: Vec3) {
        if !self.is_vector_visited(vector) {
            self.visited_vectors.push(vector);
        }
    }

    pub(crate) fn visited_objects(&self) -> &[EntityId] {
        &self.visited_objects
    }

    pub(crate) fn visited_vectors(&self) -> &[Vec3] {
        &self.visited_vectors
    }

    pub(crate) fn visited_players(&self) -> &[PlayerId] {
        &self.visited_players
    }

    pub(crate) fn visited_teams(&self) -> &[TeamId] {
        &self.visited_teams
    }
}

/// A 3D vector for locations and directions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

impl Vec3 {
    /// Make a vector from its three components.
    #[must_use]
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Return the zero vector.
    #[must_use]
    pub fn zero() -> Self {
        Self::default()
    }
}

/// RGBA color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    /// Red channel, 0 to 255.
    pub r: u8,
    /// Green channel, 0 to 255.
    pub g: u8,
    /// Blue channel, 0 to 255.
    pub b: u8,
    /// Alpha channel, 0 to 255.
    pub a: u8,
}

impl Color {
    /// Make a color from its four channels.
    #[must_use]
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// Number of resource slots stored by retail `BCost`.
pub const COST_RESOURCE_SLOTS: usize = 4;

/// Resource cost indexed by the scenario-layered `GameData/Resources` table.
///
/// The first three field names predate database-backed resource resolution.
/// `population` therefore remains the compatibility name for resource slot 2;
/// population-cap accounting is stored separately on [`crate::player::Player`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cost {
    /// Amount of resource slot 0 (supplies).
    pub supplies: f32,
    /// Amount of resource slot 1 (power).
    pub power: f32,
    /// Amount of resource slot 2. The name `population` is a legacy name.
    pub population: f32,
    /// Amount of resource slot 3.
    pub resource_3: f32,
}

impl Cost {
    /// Construct a cost from all four retail resource slots.
    #[must_use]
    pub const fn from_amounts(amounts: [f32; COST_RESOURCE_SLOTS]) -> Self {
        Self {
            supplies: amounts[0],
            power: amounts[1],
            population: amounts[2],
            resource_3: amounts[3],
        }
    }

    /// Return all resource amounts in runtime table order.
    #[must_use]
    pub const fn amounts(&self) -> [f32; COST_RESOURCE_SLOTS] {
        [self.supplies, self.power, self.population, self.resource_3]
    }

    /// Read one runtime resource slot, returning zero for an invalid ID.
    #[must_use]
    pub const fn get(&self, resource_id: usize) -> f32 {
        match resource_id {
            0 => self.supplies,
            1 => self.power,
            2 => self.population,
            3 => self.resource_3,
            _ => 0.0,
        }
    }

    /// Replace one runtime resource slot.
    pub fn set(&mut self, resource_id: usize, amount: f32) {
        match resource_id {
            0 => self.supplies = amount,
            1 => self.power = amount,
            2 => self.population = amount,
            3 => self.resource_3 = amount,
            _ => {}
        }
    }

    /// Add to one runtime resource slot.
    pub fn add(&mut self, resource_id: usize, amount: f32) {
        self.set(resource_id, self.get(resource_id) + amount);
    }
}

/// Player identifier.
pub type PlayerId = i32;

/// Team identifier.
pub type TeamId = i32;

/// Proto object ID (references game data definitions).
pub type ProtoObjectId = i32;

/// Proto squad ID.
pub type ProtoSquadId = i32;

/// Technology ID.
pub type TechId = i32;

/// Objective ID.
pub type ObjectiveId = i32;

/// Result payload written by retail trigger-issued building commands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuildingCommandState {
    done: bool,
    trained_squads: Vec<EntityId>,
}

impl BuildingCommandState {
    /// Return whether every production item associated with this command has finished.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Squads produced while executing the command, in completion order.
    #[must_use]
    pub fn trained_squads(&self) -> &[EntityId] {
        &self.trained_squads
    }

    pub(crate) fn begin(&mut self) {
        self.done = false;
        self.trained_squads.clear();
    }

    pub(crate) fn finish(&mut self) {
        self.done = true;
    }

    /// Record one produced squad using retail completion ordering.
    pub fn record_trained_squad(&mut self, squad_id: EntityId) {
        self.trained_squads.push(squad_id);
    }
}

/// Type-safe value that can be stored in a trigger variable.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerValue {
    // Primitives
    /// Boolean value.
    Bool(bool),
    /// Integer value.
    Int(i32),
    /// List of integers.
    IntegerList(Vec<i32>),
    /// Floating-point value.
    Float(f32),
    /// Text value.
    String(String),

    // Game entities
    /// Any entity.
    Entity(EntityId),
    /// List of entities.
    EntityList(Vec<EntityId>),
    /// Unit entity.
    Unit(EntityId),
    /// List of unit entities.
    UnitList(Vec<EntityId>),
    /// Squad entity.
    Squad(EntityId),
    /// List of squad entities.
    SquadList(Vec<EntityId>),
    /// Object entity.
    Object(EntityId),
    /// List of object entities.
    ObjectList(Vec<EntityId>),

    // Spatial
    /// World location.
    Location(Vec3),
    /// List of world locations.
    LocationList(Vec<Vec3>),
    /// Direction or offset vector.
    Vector(Vec3),
    /// List of vectors.
    VectorList(Vec<Vec3>),

    // Players/Teams
    /// Player ID.
    Player(PlayerId),
    /// List of player IDs.
    PlayerList(Vec<PlayerId>),
    /// Team ID.
    Team(TeamId),
    /// List of team IDs.
    TeamList(Vec<TeamId>),

    // Game data references
    /// Proto object ID from game data.
    ProtoObject(ProtoObjectId),
    /// List of proto object IDs.
    ProtoObjectList(Vec<ProtoObjectId>),
    /// Proto squad ID from game data.
    ProtoSquad(ProtoSquadId),
    /// List of proto squad IDs.
    ProtoSquadList(Vec<ProtoSquadId>),
    /// Technology ID.
    Tech(TechId),
    /// List of technology IDs.
    TechList(Vec<TechId>),
    /// Authored object-type name. Retail resolves this name to a runtime ID
    /// while loading a script; retaining the name keeps the same identity
    /// stable across scenario-layered databases.
    ObjectType(String),
    /// List of authored object-type names.
    ObjectTypeList(Vec<String>),
    /// Scenario design-object line identity.
    DesignLine(i32),
    /// Ordered design-line identities; duplicates are significant in retail.
    DesignLineList(Vec<i32>),

    // Resources
    /// Resource cost.
    Cost(Cost),
    /// Time in milliseconds.
    Time(u32), // milliseconds

    // Miscellaneous
    /// RGBA color.
    Color(Color),
    /// Objective ID.
    Objective(ObjectiveId),
    /// Trigger ID in the same script.
    Trigger(super::TriggerId),
    /// Iterator state over a list variable.
    Iterator(TriggerIterator),
    /// Set of entity filters.
    EntityFilterSet(EntityFilterSet),
    /// Result state of a building command.
    BuildingCommandState(BuildingCommandState),
    /// AI squad-analysis values.
    AISquadAnalysis(super::AISquadAnalysis),
    /// Selector for one AI squad-analysis value.
    AISquadAnalysisComponent(super::AISquadAnalysisComponent),

    // Placeholder for complex/uncommon types
    // These can be expanded as needed
    /// Raw data for a variable type that has no typed form.
    Other {
        /// Variable type of the data.
        var_type: super::VarType,
        /// Raw bytes of the value.
        data: Vec<u8>,
    },
}

impl Default for TriggerValue {
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl TriggerValue {
    /// Get as bool, returns None if wrong type.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as int, returns None if wrong type.
    #[must_use]
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Self::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as float, returns None if wrong type.
    #[must_use]
    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as entity, returns None if wrong type.
    #[must_use]
    pub fn as_entity(&self) -> Option<EntityId> {
        match self {
            Self::Entity(v) | Self::Unit(v) | Self::Squad(v) | Self::Object(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as location, returns None if wrong type.
    #[must_use]
    pub fn as_location(&self) -> Option<Vec3> {
        match self {
            Self::Location(v) | Self::Vector(v) => Some(*v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::{ReviveActionProfile, UnitRevivalProfile};

    #[test]
    fn alive_filter_excludes_hibernating_units_and_their_squads() {
        let mut world = World::new();
        world.init_players(1);
        let squad_id = world.create_squad(1);
        let unit_id = world.create_unit(1);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        world
            .get_unit_mut(unit_id)
            .unwrap()
            .configure_revival(UnitRevivalProfile::Revive(ReviveActionProfile {
                revive_delay: 1.0,
                hibernate_delay: 1.0,
                revive_rate: 1.0,
            }));
        assert!(world.damage_unit(unit_id, 100.0));

        let mut alive = EntityFilterSet::default();
        alive.push(EntityFilterPredicate::IsAlive { invert: false });
        assert!(!alive.matches_entity(unit_id, &world));
        assert!(!alive.matches_entity(squad_id, &world));

        let mut not_alive = EntityFilterSet::default();
        not_alive.push(EntityFilterPredicate::IsAlive { invert: true });
        assert!(not_alive.matches_entity(unit_id, &world));
        assert!(not_alive.matches_entity(squad_id, &world));
    }
}
