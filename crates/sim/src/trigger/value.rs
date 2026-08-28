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

    fn matches_unit(&self, entity_id: EntityId, world: &World) -> bool {
        let Some(unit) = world.get_unit(entity_id) else {
            return false;
        };
        self.filters.iter().all(|predicate| {
            let (matches, invert) = match predicate {
                EntityFilterPredicate::IsAlive { invert } => (unit.is_alive(), *invert),
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
                EntityFilterPredicate::IsAlive { invert } => (squad.is_alive(), *invert),
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
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    #[must_use]
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub fn zero() -> Self {
        Self::default()
    }
}

/// RGBA color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
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
    pub supplies: f32,
    pub power: f32,
    pub population: f32,
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
    Bool(bool),
    Int(i32),
    IntegerList(Vec<i32>),
    Float(f32),
    String(String),

    // Game entities
    Entity(EntityId),
    EntityList(Vec<EntityId>),
    Unit(EntityId),
    UnitList(Vec<EntityId>),
    Squad(EntityId),
    SquadList(Vec<EntityId>),
    Object(EntityId),
    ObjectList(Vec<EntityId>),

    // Spatial
    Location(Vec3),
    LocationList(Vec<Vec3>),
    Vector(Vec3),
    VectorList(Vec<Vec3>),

    // Players/Teams
    Player(PlayerId),
    PlayerList(Vec<PlayerId>),
    Team(TeamId),
    TeamList(Vec<TeamId>),

    // Game data references
    ProtoObject(ProtoObjectId),
    ProtoObjectList(Vec<ProtoObjectId>),
    ProtoSquad(ProtoSquadId),
    ProtoSquadList(Vec<ProtoSquadId>),
    Tech(TechId),
    TechList(Vec<TechId>),
    /// Authored object-type name. Retail resolves this name to a runtime ID
    /// while loading a script; retaining the name keeps the same identity
    /// stable across scenario-layered databases.
    ObjectType(String),
    ObjectTypeList(Vec<String>),

    // Resources
    Cost(Cost),
    Time(u32), // milliseconds

    // Miscellaneous
    Color(Color),
    Objective(ObjectiveId),
    Trigger(super::TriggerId),
    Iterator(TriggerIterator),
    EntityFilterSet(EntityFilterSet),
    BuildingCommandState(BuildingCommandState),
    AISquadAnalysis(super::AISquadAnalysis),
    AISquadAnalysisComponent(super::AISquadAnalysisComponent),

    // Placeholder for complex/uncommon types
    // These can be expanded as needed
    Other {
        var_type: super::VarType,
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
