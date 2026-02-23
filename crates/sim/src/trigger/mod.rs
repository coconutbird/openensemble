//! Trigger system for scripted game events.
//!
//! This is a hybrid system that supports:
//! - Loading vanilla `.triggerscript` files (XMB format)
//! - Custom scripts via an embedded scripting language (future)
//!
//! Both compile to the same internal representation and execute
//! through the same deterministic engine.

mod condition;
mod effect;
mod engine;
mod loader;
mod script;
#[allow(clippy::module_inception)]
mod trigger;
mod value;
mod var_type;

pub use condition::{Condition, ConditionType};
pub use effect::{Effect, EffectType};
pub use engine::TriggerEngine;
pub use loader::{LoadError, LoadResult, VanillaLoader};
pub use script::{ScriptType, TriggerScript, TriggerVar};
pub use trigger::Trigger;
pub use value::TriggerValue;
pub use var_type::VarType;

/// Unique identifier for a trigger within a script.
pub type TriggerId = u32;

/// Unique identifier for a trigger script.
pub type TriggerScriptId = u32;

/// Unique identifier for a variable within a script.
pub type VarId = u32;

/// Invalid trigger ID constant.
pub const INVALID_TRIGGER_ID: TriggerId = 0xFFFFFFFF;

/// Invalid trigger script ID constant.
pub const INVALID_TRIGGER_SCRIPT_ID: TriggerScriptId = 0xFFFFFFFF;
