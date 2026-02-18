//! Trigger variable values - type-safe storage for trigger data.

use crate::EntityId;

/// A 3D vector for locations and directions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

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

/// Resource cost (supplies, power, population).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cost {
    pub supplies: f32,
    pub power: f32,
    pub population: f32,
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

/// Type-safe value that can be stored in a trigger variable.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerValue {
    // Primitives
    Bool(bool),
    Int(i32),
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
    ObjectType(i32),
    ObjectTypeList(Vec<i32>),

    // Resources
    Cost(Cost),
    Time(u32), // milliseconds

    // Miscellaneous
    Color(Color),
    Objective(ObjectiveId),
    Trigger(super::TriggerId),

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
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as int, returns None if wrong type.
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Self::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as float, returns None if wrong type.
    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as entity, returns None if wrong type.
    pub fn as_entity(&self) -> Option<EntityId> {
        match self {
            Self::Entity(v) | Self::Unit(v) | Self::Squad(v) | Self::Object(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as location, returns None if wrong type.
    pub fn as_location(&self) -> Option<Vec3> {
        match self {
            Self::Location(v) | Self::Vector(v) => Some(*v),
            _ => None,
        }
    }
}
