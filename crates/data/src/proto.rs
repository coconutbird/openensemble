//! Proto object definitions.
//!
//! Proto objects define the base properties of game entities (units, buildings, techs).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Proto object ID.
pub type ProtoId = u32;

/// A proto object (base definition for units/buildings).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtoObject {
    /// Unique ID.
    pub id: ProtoId,
    /// Internal name.
    pub name: String,
    /// Display name key.
    pub display_name: String,
    /// Object type (unit, building, etc.).
    pub object_type: ObjectType,
    /// Base hitpoints.
    pub hitpoints: f32,
    /// Base shield points.
    pub shield_points: f32,
    /// Movement speed.
    pub movement_speed: f32,
    /// Build time in seconds.
    pub build_time: f32,
    /// Resource costs.
    pub cost: ResourceCost,
    /// Population cost.
    pub population_cost: i32,
    /// Population capacity (for buildings).
    pub population_capacity: i32,
    /// Unit flags.
    pub flags: ProtoFlags,
}

/// Object type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ObjectType {
    #[default]
    Unit,
    Building,
    Projectile,
    Effect,
    Resource,
}

/// Resource cost for building/training.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceCost {
    pub supplies: f32,
    pub power: f32,
    pub population: i32,
}

/// Proto object flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtoFlags {
    pub can_attack: bool,
    pub can_move: bool,
    pub flying: bool,
    pub infantry: bool,
    pub vehicle: bool,
    pub aircraft: bool,
    pub building: bool,
    pub hero: bool,
}

/// Proto squad definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtoSquad {
    /// Unique ID.
    pub id: ProtoId,
    /// Internal name.
    pub name: String,
    /// Proto objects that make up this squad.
    pub units: Vec<SquadUnit>,
    /// Formation type.
    pub formation: String,
}

/// A unit in a squad.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SquadUnit {
    /// Proto object ID.
    pub proto_id: ProtoId,
    /// Number of units.
    pub count: u32,
}

/// Proto tech definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtoTech {
    /// Unique ID.
    pub id: ProtoId,
    /// Internal name.
    pub name: String,
    /// Display name key.
    pub display_name: String,
    /// Research time in seconds.
    pub research_time: f32,
    /// Resource costs.
    pub cost: ResourceCost,
    /// Effects applied when researched.
    pub effects: Vec<TechEffect>,
}

/// A tech research effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TechEffect {
    /// Modify a stat by a percentage.
    ModifyPercent { stat: String, amount: f32 },
    /// Modify a stat by an absolute value.
    ModifyAbsolute { stat: String, amount: f32 },
    /// Enable an ability.
    EnableAbility { ability: String },
    /// Unlock a unit/building.
    Unlock { proto_name: String },
}

/// Database of all proto objects.
#[derive(Debug, Default)]
pub struct ProtoDatabase {
    /// Proto objects by ID.
    pub objects: HashMap<ProtoId, ProtoObject>,
    /// Proto objects by name.
    pub objects_by_name: HashMap<String, ProtoId>,
    /// Proto squads by ID.
    pub squads: HashMap<ProtoId, ProtoSquad>,
    /// Proto squads by name.
    pub squads_by_name: HashMap<String, ProtoId>,
    /// Proto techs by ID.
    pub techs: HashMap<ProtoId, ProtoTech>,
    /// Proto techs by name.
    pub techs_by_name: HashMap<String, ProtoId>,
}

impl ProtoDatabase {
    /// Create a new empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a proto object.
    pub fn add_object(&mut self, obj: ProtoObject) {
        let id = obj.id;
        let name = obj.name.clone();
        self.objects.insert(id, obj);
        self.objects_by_name.insert(name, id);
    }

    /// Get a proto object by ID.
    pub fn get_object(&self, id: ProtoId) -> Option<&ProtoObject> {
        self.objects.get(&id)
    }

    /// Get a proto object by name.
    pub fn get_object_by_name(&self, name: &str) -> Option<&ProtoObject> {
        self.objects_by_name
            .get(name)
            .and_then(|&id| self.objects.get(&id))
    }
}

