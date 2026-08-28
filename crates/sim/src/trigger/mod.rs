//! Trigger system for scripted game events.
//!
//! This is a hybrid system that supports:
//! - Loading vanilla `.triggerscript` files (XMB format)
//! - Custom scripts via an embedded scripting language (future)
//!
//! Both compile to the same internal representation and execute
//! through the same deterministic engine.

mod ai_analysis;
mod binding;
mod checksum;
mod condition;
mod conditions;
#[path = "trigger.rs"]
mod definition;
mod effect;
mod effects;
mod engine;
mod loader;
mod script;
mod value;
mod var_type;

pub use ai_analysis::{AISquadAnalysis, AISquadAnalysisComponent};
pub use binding::VarBinding;
pub use condition::{Condition, ConditionType};
pub use definition::{ConditionMode, Trigger};
pub use effect::{Effect, EffectType};
pub use engine::{ConditionResult, TriggerEngine, TriggerUpdate};
pub use loader::{LoadError, LoadResult, TriggerLoadContext, VanillaLoader};
pub use script::{ScriptType, TriggerScript, TriggerVar};
pub use value::{
    BuildingCommandState, Cost as TriggerCost, EntityFilterSet, TriggerIterator, TriggerValue,
    Vec3 as TriggerVec3,
};
pub use var_type::VarType;

/// Unique identifier for a trigger within a script.
pub type TriggerId = u32;

/// Unique identifier for a trigger script.
pub type TriggerScriptId = u32;

/// Unique identifier for a variable within a script.
pub type VarId = u32;

/// Invalid trigger ID constant.
pub const INVALID_TRIGGER_ID: TriggerId = 0xFFFF_FFFF;

/// Invalid trigger script ID constant.
pub const INVALID_TRIGGER_SCRIPT_ID: TriggerScriptId = 0xFFFF_FFFF;
