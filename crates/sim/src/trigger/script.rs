//! `TriggerScript` - a collection of triggers and shared variables.

use std::collections::HashMap;

use super::{Trigger, TriggerId, TriggerScriptId, TriggerValue, VarId, VarType};

/// Type of trigger script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ScriptType {
    #[default]
    Invalid = 0,
    /// Scenario scripts (campaign, skirmish).
    Scenario = 1,
    /// Power scripts (leader abilities).
    Power = 2,
    /// Unit ability scripts.
    Ability = 3,
    /// Standalone trigger scripts.
    TriggerScript = 4,
}

/// A variable in a trigger script.
#[derive(Debug, Clone)]
pub struct TriggerVar {
    /// Variable ID (index in the script's variable list).
    pub id: VarId,

    /// Editor-assigned ID.
    pub editor_id: VarId,

    /// Human-readable name (debug only).
    pub name: String,

    /// The type of this variable.
    pub var_type: VarType,

    /// The current value.
    pub value: TriggerValue,

    /// Whether this variable is an input parameter.
    pub is_input: bool,

    /// Whether this variable is an output parameter.
    pub is_output: bool,
}

impl TriggerVar {
    /// Create a new variable.
    #[must_use]
    pub fn new(id: VarId, var_type: VarType) -> Self {
        Self {
            id,
            editor_id: id,
            name: String::new(),
            var_type,
            value: TriggerValue::default(),
            is_input: false,
            is_output: false,
        }
    }

    /// Set the variable name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the initial value.
    #[must_use]
    pub fn with_value(mut self, value: TriggerValue) -> Self {
        self.value = value;
        self
    }

    /// Mark as input parameter.
    #[must_use]
    pub fn as_input(mut self) -> Self {
        self.is_input = true;
        self
    }

    /// Mark as output parameter.
    #[must_use]
    pub fn as_output(mut self) -> Self {
        self.is_output = true;
        self
    }
}

/// A trigger script containing variables and triggers.
#[derive(Debug, Clone)]
pub struct TriggerScript {
    /// Unique ID for this script instance.
    pub id: TriggerScriptId,

    /// Human-readable name.
    pub name: String,

    /// Type of script.
    pub script_type: ScriptType,

    /// Variables shared across triggers.
    pub variables: Vec<TriggerVar>,

    /// Map from editor ID to runtime ID for variables.
    pub var_editor_to_id: HashMap<VarId, VarId>,

    /// Triggers in this script.
    pub triggers: Vec<Trigger>,

    /// Map from trigger ID to index in triggers vec.
    pub trigger_id_to_index: HashMap<TriggerId, usize>,

    /// Whether this script is active.
    pub is_active: bool,

    /// Whether this script is paused.
    pub is_paused: bool,

    /// Whether this script should be cleaned up.
    pub marked_for_cleanup: bool,
}

impl Default for TriggerScript {
    fn default() -> Self {
        Self {
            id: super::INVALID_TRIGGER_SCRIPT_ID,
            name: String::new(),
            script_type: ScriptType::Invalid,
            variables: Vec::new(),
            var_editor_to_id: HashMap::new(),
            triggers: Vec::new(),
            trigger_id_to_index: HashMap::new(),
            is_active: false,
            is_paused: false,
            marked_for_cleanup: false,
        }
    }
}

impl TriggerScript {
    /// Create a new trigger script.
    #[must_use]
    pub fn new(id: TriggerScriptId) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }

    /// Set the script name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the script type.
    #[must_use]
    pub fn with_type(mut self, script_type: ScriptType) -> Self {
        self.script_type = script_type;
        self
    }

    /// Add a variable to the script.
    pub fn add_variable(&mut self, var: TriggerVar) {
        let editor_id = var.editor_id;
        let id = var.id;
        self.variables.push(var);
        self.var_editor_to_id.insert(editor_id, id);
    }

    /// Add a trigger to the script.
    pub fn add_trigger(&mut self, trigger: Trigger) {
        let id = trigger.id;
        let index = self.triggers.len();
        self.triggers.push(trigger);
        self.trigger_id_to_index.insert(id, index);
    }

    /// Get a variable by ID.
    #[must_use]
    pub fn get_variable(&self, id: VarId) -> Option<&TriggerVar> {
        self.variables.get(id as usize)
    }

    /// Get a mutable variable by ID.
    pub fn get_variable_mut(&mut self, id: VarId) -> Option<&mut TriggerVar> {
        self.variables.get_mut(id as usize)
    }

    /// Get a variable by editor ID.
    #[must_use]
    pub fn get_variable_by_editor_id(&self, editor_id: VarId) -> Option<&TriggerVar> {
        self.var_editor_to_id
            .get(&editor_id)
            .and_then(|id| self.get_variable(*id))
    }

    /// Get a trigger by ID.
    #[must_use]
    pub fn get_trigger(&self, id: TriggerId) -> Option<&Trigger> {
        self.trigger_id_to_index
            .get(&id)
            .and_then(|idx| self.triggers.get(*idx))
    }

    /// Get a mutable trigger by ID.
    pub fn get_trigger_mut(&mut self, id: TriggerId) -> Option<&mut Trigger> {
        if let Some(&idx) = self.trigger_id_to_index.get(&id) {
            self.triggers.get_mut(idx)
        } else {
            None
        }
    }

    /// Activate this script.
    pub fn activate(&mut self, current_time: u32) {
        self.is_active = true;

        // Activate all triggers that should start active
        for trigger in &mut self.triggers {
            if trigger.start_active {
                trigger.on_activated(current_time);
            }
        }
    }

    /// Deactivate this script.
    pub fn deactivate(&mut self) {
        self.is_active = false;
        for trigger in &mut self.triggers {
            trigger.on_deactivated();
        }
    }

    /// Activate a specific trigger.
    pub fn activate_trigger(&mut self, trigger_id: TriggerId, current_time: u32) {
        if let Some(trigger) = self.get_trigger_mut(trigger_id) {
            trigger.on_activated(current_time);
        }
    }

    /// Deactivate a specific trigger.
    pub fn deactivate_trigger(&mut self, trigger_id: TriggerId) {
        if let Some(trigger) = self.get_trigger_mut(trigger_id) {
            trigger.on_deactivated();
        }
    }
}
