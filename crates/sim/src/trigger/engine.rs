//! Deterministic scheduling and execution of retail trigger scripts.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use super::effects::{ControlAction, EffectOutcome, execute_effect};
use super::library::LibraryScript;
use super::{
    INVALID_TRIGGER_SCRIPT_ID, TriggerExternals, TriggerId, TriggerScript, TriggerScriptId,
    TriggerScriptLibrary,
};
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::world::World;
use pipeline::database::hw1::Database;

/// Result of evaluating one condition or condition tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionResult {
    /// The condition passed.
    True,
    /// The condition failed.
    False,
    /// An asynchronous condition is waiting for external consensus.
    Waiting,
    /// The retail condition DBID is not implemented yet.
    Unsupported(u16),
}

/// Observable work performed by one trigger-manager update.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TriggerUpdate {
    /// Condition trees evaluated during the update.
    pub evaluations: u32,
    /// Triggers that fired either their true or conditional-false branch.
    pub triggers_fired: u32,
    /// Effects that changed authoritative simulation state.
    pub effects_applied: u32,
    /// Supported presentation/debug effects intentionally left to a UI adapter.
    pub presentation_effects: u32,
    /// Supported effects skipped because an authored binding or target was invalid.
    pub effects_skipped: u32,
    /// Effect DBIDs skipped because an authored binding or target was invalid.
    pub skipped_effect_types: Vec<u16>,
    /// Condition DBIDs encountered without an implementation.
    pub unsupported_condition_types: Vec<u16>,
    /// Effect DBIDs encountered without an implementation.
    pub unsupported_effect_types: Vec<u16>,
    /// Scripts that `LaunchScript` started, in launch order.
    pub launched_scripts: Vec<TriggerScriptId>,
    /// File names that `LaunchScript` asked for but the script library does not have.
    pub missing_script_files: Vec<String>,
    /// Whether the deterministic safety limit stopped a same-tick trigger loop.
    pub infinite_loop_guard_reached: bool,
}

impl TriggerUpdate {
    fn record_condition_result(&mut self, result: ConditionResult) {
        if let ConditionResult::Unsupported(raw_type) = result {
            unique_add(&mut self.unsupported_condition_types, raw_type);
        }
    }

    fn record_effect(&mut self, raw_type: u16, outcome: EffectOutcome) {
        match outcome {
            EffectOutcome::Applied => self.effects_applied += 1,
            EffectOutcome::Presentation => self.presentation_effects += 1,
            EffectOutcome::Skipped => {
                self.effects_skipped += 1;
                unique_add(&mut self.skipped_effect_types, raw_type);
            }
            EffectOutcome::Unsupported(raw_type) => {
                unique_add(&mut self.unsupported_effect_types, raw_type);
            }
        }
    }
}

/// The deterministic owner of active trigger scripts.
#[derive(Debug)]
pub struct TriggerEngine {
    pub(crate) scripts: BTreeMap<TriggerScriptId, TriggerScript>,
    pub(crate) next_script_id: TriggerScriptId,
    library: TriggerScriptLibrary,
    evaluate_count: u32,
    performance_warning_threshold: u32,
    infinite_loop_threshold: u32,
}

#[derive(Clone, Copy)]
struct EvaluationContext<'database> {
    current_time: u32,
    database: Option<&'database Database>,
    gameplay: Option<&'database GameplayCatalog>,
}

impl Default for TriggerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerEngine {
    /// Create an empty engine with retail-inspired diagnostic limits.
    ///
    /// Script IDs start at 0, like the retail `nextTriggerScriptID` after a game reset.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scripts: BTreeMap::new(),
            next_script_id: 0,
            library: TriggerScriptLibrary::default(),
            evaluate_count: 0,
            performance_warning_threshold: 1_000,
            infinite_loop_threshold: 50_000,
        }
    }

    /// Add a script and return its stable runtime ID.
    ///
    /// A script with the invalid ID gets the next ID in creation order
    /// (`BTriggerManager_createScriptFromNode`).
    pub fn add_script(&mut self, mut script: TriggerScript) -> TriggerScriptId {
        let id = if script.id == INVALID_TRIGGER_SCRIPT_ID {
            let id = self.next_script_id;
            self.next_script_id = self.next_script_id.wrapping_add(1);
            script.id = id;
            id
        } else {
            self.next_script_id = self.next_script_id.max(script.id.wrapping_add(1));
            script.id
        };
        self.scripts.insert(id, script);
        id
    }

    /// Read the parsed trigger script files that [`launch_script`](Self::launch_script) uses.
    #[must_use]
    pub fn script_library(&self) -> &TriggerScriptLibrary {
        &self.library
    }

    /// Mutably access the script library. Fill it before the first tick.
    pub fn script_library_mut(&mut self) -> &mut TriggerScriptLibrary {
        &mut self.library
    }

    /// Create a script from a library file, set its externals and activate it.
    ///
    /// This follows `BTriggerManager_createScriptFromFile`, the `setExternal*` calls and
    /// `BTriggerManager_activateScript`:
    ///
    /// - A file that is not in the library gives `None` and uses no script ID.
    /// - A file that did not parse uses one script ID and gives `None`.
    /// - Otherwise the script gets the next ID and the file name, and its start-active triggers
    ///   become active. A script launched during an update first runs in the next update.
    pub fn launch_script(
        &mut self,
        file_name: &str,
        externals: TriggerExternals,
        current_time: u32,
    ) -> Option<TriggerScriptId> {
        let mut script = match self.library.get(file_name) {
            LibraryScript::Missing => return None,
            LibraryScript::Invalid => {
                self.next_script_id = self.next_script_id.wrapping_add(1);
                return None;
            }
            LibraryScript::Parsed(script) => script.clone(),
        };
        script.id = INVALID_TRIGGER_SCRIPT_ID;
        file_name.clone_into(&mut script.name);
        script.externals = externals;
        let id = self.add_script(script);
        self.activate_script(id, current_time);
        Some(id)
    }

    /// Remove a script.
    pub fn remove_script(&mut self, id: TriggerScriptId) -> Option<TriggerScript> {
        self.scripts.remove(&id)
    }

    /// Get a script by runtime ID.
    #[must_use]
    pub fn get_script(&self, id: TriggerScriptId) -> Option<&TriggerScript> {
        self.scripts.get(&id)
    }

    /// Mutably get a script by runtime ID.
    pub fn get_script_mut(&mut self, id: TriggerScriptId) -> Option<&mut TriggerScript> {
        self.scripts.get_mut(&id)
    }

    /// Iterate scripts in deterministic runtime-ID order.
    pub fn scripts(&self) -> impl Iterator<Item = (&TriggerScriptId, &TriggerScript)> {
        self.scripts.iter()
    }

    /// Return the number of resident scripts.
    #[must_use]
    pub fn script_count(&self) -> usize {
        self.scripts.len()
    }

    /// Return the number of currently active triggers across all scripts.
    #[must_use]
    pub fn active_trigger_count(&self) -> usize {
        self.scripts
            .values()
            .map(|script| script.active_triggers().len())
            .sum()
    }

    /// Activate a script and all of its authored start-active triggers.
    pub fn activate_script(&mut self, id: TriggerScriptId, current_time: u32) {
        if let Some(script) = self.scripts.get_mut(&id) {
            script.activate(current_time);
        }
    }

    /// Record a destroyed unit or squad for every active script.
    ///
    /// This follows `BTriggerManager_invalidateEntityID`. Each script removes the ID from its
    /// list variables at the start of its next update.
    pub fn invalidate_entity(&mut self, entity_id: EntityId) {
        for script in self.scripts.values_mut() {
            script.invalidate_entity(entity_id, false);
        }
    }

    /// Configure the diagnostic evaluation threshold.
    pub fn set_performance_warning_threshold(&mut self, threshold: u32) {
        self.performance_warning_threshold = threshold;
    }

    /// Configure the hard same-update loop guard.
    pub fn set_infinite_loop_threshold(&mut self, threshold: u32) {
        self.infinite_loop_threshold = threshold.max(1);
    }

    /// Return whether the most recent update crossed the diagnostic threshold.
    #[must_use]
    pub fn performance_warning_reached(&self) -> bool {
        self.evaluate_count >= self.performance_warning_threshold
    }

    /// Update scripts in runtime-ID order against authoritative world state.
    pub fn update(&mut self, world: &mut World) -> TriggerUpdate {
        self.update_with_context(world, None, None)
    }

    /// Update scripts with the active layered gameplay database.
    pub fn update_with_database(
        &mut self,
        world: &mut World,
        database: &Database,
    ) -> TriggerUpdate {
        self.update_with_context(world, Some(database), None)
    }

    /// Update scripts with the active layered database and tactic catalog.
    pub fn update_with_gameplay(
        &mut self,
        world: &mut World,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> TriggerUpdate {
        self.update_with_context(world, Some(database), Some(gameplay))
    }

    /// Run one `BTriggerManager_update`.
    ///
    /// The script list is fixed at the start, so a script that an effect launches first runs
    /// in the next update. A script marked for cleanup is deleted right after its update.
    fn update_with_context(
        &mut self,
        world: &mut World,
        database: Option<&Database>,
        gameplay: Option<&GameplayCatalog>,
    ) -> TriggerUpdate {
        self.evaluate_count = 0;
        let current_time = world.game_time_ms;
        let context = EvaluationContext {
            current_time,
            database,
            gameplay,
        };
        let mut update = TriggerUpdate::default();
        let script_ids = self.scripts.keys().copied().collect::<Vec<_>>();

        for script_id in script_ids {
            let Some(mut script) = self.scripts.remove(&script_id) else {
                continue;
            };
            if script.is_active && !script.is_paused {
                self.update_script(&mut script, world, context, &mut update);
            }
            if !script.marked_for_cleanup {
                self.scripts.insert(script_id, script);
            }
            if update.infinite_loop_guard_reached {
                break;
            }
        }

        update
    }

    /// Run one `BTriggerScript_update`.
    fn update_script(
        &mut self,
        script: &mut TriggerScript,
        world: &mut World,
        context: EvaluationContext<'_>,
        update: &mut TriggerUpdate,
    ) {
        script.flush_invalid_entities();
        for trigger in &mut script.triggers {
            trigger.reset_evaluate_count();
        }
        let mut queue = script
            .active_triggers()
            .iter()
            .copied()
            .collect::<VecDeque<_>>();

        while let Some(trigger_id) = queue.pop_front() {
            if self.evaluate_count >= self.infinite_loop_threshold {
                update.infinite_loop_guard_reached = true;
                break;
            }
            self.evaluate_queued_trigger(script, trigger_id, world, context, &mut queue, update);
        }

        if script.active_triggers().is_empty() {
            script.marked_for_cleanup = true;
        }
    }

    fn evaluate_queued_trigger(
        &mut self,
        script: &mut TriggerScript,
        trigger_id: TriggerId,
        world: &mut World,
        context: EvaluationContext<'_>,
        queue: &mut VecDeque<TriggerId>,
        update: &mut TriggerUpdate,
    ) {
        let Some(trigger) = script.get_trigger(trigger_id) else {
            return;
        };
        if !trigger.time_to_evaluate(context.current_time) || !trigger.has_evaluations_remaining() {
            return;
        }

        let conditions = Arc::clone(&trigger.conditions);
        let condition_mode = trigger.condition_mode;
        let activated_time = trigger.activated_time;
        let is_conditional = trigger.is_conditional;
        if let Some(trigger) = script.get_trigger_mut(trigger_id) {
            trigger.update_next_evaluate_time(context.current_time);
        }
        self.evaluate_count += 1;
        update.evaluations += 1;

        let result = super::conditions::evaluate_conditions(
            &conditions,
            condition_mode,
            activated_time,
            script,
            world,
            context.database,
        );
        update.record_condition_result(result);
        let fire_true = result == ConditionResult::True;
        if !(fire_true || is_conditional && result == ConditionResult::False) {
            return;
        }

        let effects = script
            .get_trigger(trigger_id)
            .map_or_else(Arc::default, |trigger| {
                if fire_true {
                    Arc::clone(&trigger.effects_on_true)
                } else {
                    Arc::clone(&trigger.effects_on_false)
                }
            });
        deactivate_trigger(script, trigger_id, queue);
        update.triggers_fired += 1;
        for effect in effects.iter() {
            let (mut outcome, control) =
                execute_effect(effect, script, world, context.database, context.gameplay);
            if let Some(control) = control
                && !self.apply_control(control, script, context.current_time, queue, update)
            {
                outcome = EffectOutcome::Skipped;
            }
            update.record_effect(effect.raw_type, outcome);
            self.record_destroyed_entities(world, script);
        }
    }

    /// Apply a schedule change of an effect. Return `false` if the change did nothing.
    fn apply_control(
        &mut self,
        control: ControlAction,
        script: &mut TriggerScript,
        current_time: u32,
        queue: &mut VecDeque<TriggerId>,
        update: &mut TriggerUpdate,
    ) -> bool {
        match control {
            ControlAction::Activate(trigger_id) => {
                if script.get_trigger(trigger_id).is_none() {
                    return false;
                }
                script.activate_trigger(trigger_id, current_time);
                if !queue.contains(&trigger_id) {
                    queue.push_back(trigger_id);
                }
            }
            ControlAction::Deactivate(trigger_id) => {
                deactivate_trigger(script, trigger_id, queue);
            }
            ControlAction::Shutdown => {
                script.shut_down();
                queue.clear();
            }
            ControlAction::Launch(launch) => {
                let Some(script_id) =
                    self.launch_script(&launch.file_name, launch.externals, current_time)
                else {
                    update.missing_script_files.push(launch.file_name);
                    return false;
                };
                update.launched_scripts.push(script_id);
            }
        }
        true
    }

    /// Move the units and squads that an effect destroyed into the scripts.
    ///
    /// The running script removes them at once. The other active scripts remove them at the
    /// start of their next update (`BTriggerScript_invalidateEntityID`).
    fn record_destroyed_entities(&mut self, world: &mut World, updating: &mut TriggerScript) {
        for entity_id in world.take_destroyed_trigger_entities() {
            updating.invalidate_entity(entity_id, true);
            self.invalidate_entity(entity_id);
        }
    }
}

fn deactivate_trigger(
    script: &mut TriggerScript,
    trigger_id: TriggerId,
    queue: &mut VecDeque<TriggerId>,
) {
    script.deactivate_trigger(trigger_id);
    queue.retain(|queued_id| *queued_id != trigger_id);
}

fn unique_add(values: &mut Vec<u16>, value: u16) {
    if let Err(index) = values.binary_search(&value) {
        values.insert(index, value);
    }
}

#[cfg(test)]
mod tests;
