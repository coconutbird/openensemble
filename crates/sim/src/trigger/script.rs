//! `TriggerScript` - a collection of triggers and shared variables.

use std::collections::BTreeMap;

use super::{Trigger, TriggerExternals, TriggerId, TriggerScriptId, TriggerValue, VarId, VarType};
use crate::entity_id::{EntityClass, EntityId};

/// Type of trigger script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ScriptType {
    /// No valid script type. This is the default.
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

    /// Whether the authored variable is null. Retail sets this flag only when it loads the
    /// script (`BTriggerScript_loadTriggerVars`), so a write does not change it.
    is_null: bool,

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
            is_null: false,
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

    /// Mark this typed variable as null.
    ///
    /// Use this only while you build a script. The flag does not change after load.
    #[must_use]
    pub fn null(mut self) -> Self {
        self.is_null = true;
        self
    }

    /// Mark as output parameter.
    #[must_use]
    pub fn as_output(mut self) -> Self {
        self.is_output = true;
        self
    }

    /// Return whether the authored variable is null.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        self.is_null
    }
}

/// A trigger script containing variables and triggers.
#[derive(Debug, Clone)]
pub struct TriggerScript {
    /// Unique ID for this script instance.
    pub id: TriggerScriptId,

    /// Human-readable name. A script that the engine launches from a file has the file name.
    pub name: String,

    /// Type of script.
    pub script_type: ScriptType,

    /// Variables shared across triggers.
    pub variables: BTreeMap<VarId, TriggerVar>,

    /// Map from editor ID to runtime ID for variables.
    pub var_editor_to_id: BTreeMap<VarId, VarId>,

    /// Triggers in this script.
    pub triggers: Vec<Trigger>,

    /// Map from trigger ID to index in triggers vec.
    pub trigger_id_to_index: BTreeMap<TriggerId, usize>,

    /// Whether this script is active.
    pub is_active: bool,

    /// Whether this script is paused.
    pub is_paused: bool,

    /// Whether this script should be cleaned up.
    pub marked_for_cleanup: bool,

    /// Active triggers in activation order (`BTriggerScript_activateTrigger`).
    active_triggers: Vec<TriggerId>,

    /// Inputs that the launcher of this script gives (`BTriggerScriptExternals`).
    pub externals: TriggerExternals,

    /// Destroyed entity IDs to remove from list variables at the start of the next update.
    pending_invalid_entities: Vec<EntityId>,
}

impl Default for TriggerScript {
    fn default() -> Self {
        Self {
            id: super::INVALID_TRIGGER_SCRIPT_ID,
            name: String::new(),
            script_type: ScriptType::Invalid,
            variables: BTreeMap::new(),
            var_editor_to_id: BTreeMap::new(),
            triggers: Vec::new(),
            trigger_id_to_index: BTreeMap::new(),
            is_active: false,
            is_paused: false,
            marked_for_cleanup: false,
            active_triggers: Vec::new(),
            externals: TriggerExternals::default(),
            pending_invalid_entities: Vec::new(),
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

    /// Add a variable to the script.
    pub fn add_variable(&mut self, var: TriggerVar) {
        let editor_id = var.editor_id;
        let id = var.id;
        self.variables.insert(id, var);
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
        self.variables.get(&id)
    }

    /// Get a mutable variable by ID.
    pub fn get_variable_mut(&mut self, id: VarId) -> Option<&mut TriggerVar> {
        self.variables.get_mut(&id)
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

    /// Activate this script and its start-active triggers in trigger order.
    ///
    /// This follows `BTriggerManager_activateScript`.
    pub fn activate(&mut self, current_time: u32) {
        self.is_active = true;
        self.marked_for_cleanup = false;
        let start_active = self
            .triggers
            .iter()
            .filter(|trigger| trigger.start_active)
            .map(|trigger| trigger.id)
            .collect::<Vec<_>>();
        for trigger_id in start_active {
            self.activate_trigger(trigger_id, current_time);
        }
    }

    /// Activate a trigger.
    ///
    /// This follows `BTriggerScript_activateTrigger`. The activation time changes also when the
    /// trigger is already active. A newly active trigger goes to the end of the active list.
    pub fn activate_trigger(&mut self, trigger_id: TriggerId, current_time: u32) {
        let Some(trigger) = self.get_trigger_mut(trigger_id) else {
            return;
        };
        trigger.activated_time = current_time;
        if !self.active_triggers.contains(&trigger_id) {
            self.active_triggers.push(trigger_id);
        }
    }

    /// Deactivate a trigger and remove it from the active list.
    pub fn deactivate_trigger(&mut self, trigger_id: TriggerId) {
        self.active_triggers
            .retain(|active_id| *active_id != trigger_id);
    }

    /// Return whether a trigger is active.
    #[must_use]
    pub fn is_trigger_active(&self, trigger_id: TriggerId) -> bool {
        self.active_triggers.contains(&trigger_id)
    }

    /// Return the active triggers in activation order.
    #[must_use]
    pub fn active_triggers(&self) -> &[TriggerId] {
        &self.active_triggers
    }

    /// Stop the script after its current update (`Shutdown`, retail effect case `0x38`).
    pub(super) fn shut_down(&mut self) {
        self.marked_for_cleanup = true;
        self.active_triggers.clear();
    }

    /// Record a destroyed entity, or remove it now if `updating` is true.
    ///
    /// This follows `BTriggerScript_invalidateEntityID`. An inactive script ignores the ID.
    pub(super) fn invalidate_entity(&mut self, entity_id: EntityId, updating: bool) {
        if !self.is_active {
            return;
        }
        if updating {
            self.remove_entity_from_lists(entity_id);
        } else {
            self.pending_invalid_entities.push(entity_id);
        }
    }

    /// Remove the recorded destroyed entities from the list variables.
    pub(super) fn flush_invalid_entities(&mut self) {
        let pending = std::mem::take(&mut self.pending_invalid_entities);
        for entity_id in pending {
            self.remove_entity_from_lists(entity_id);
        }
    }

    /// Return the destroyed entity IDs that the next update removes.
    #[must_use]
    pub fn pending_invalid_entities(&self) -> &[EntityId] {
        &self.pending_invalid_entities
    }

    /// Remove the first copy of a unit ID from each non-null unit list, or of a squad ID from
    /// each non-null squad list. Retail keeps the per-type lists only for `UnitList` and
    /// `SquadList` variables.
    fn remove_entity_from_lists(&mut self, entity_id: EntityId) {
        let list_type = match entity_id.class() {
            Some(EntityClass::Unit) => VarType::UnitList,
            Some(EntityClass::Squad) => VarType::SquadList,
            _ => return,
        };
        for variable in self.variables.values_mut() {
            if variable.var_type != list_type || variable.is_null() {
                continue;
            }
            if let TriggerValue::UnitList(values) | TriggerValue::SquadList(values) =
                &mut variable.value
                && let Some(index) = values.iter().position(|value| *value == entity_id)
            {
                values.remove(index);
            }
        }
    }
}
