//! TriggerEngine - deterministic execution of trigger scripts.

use std::collections::BTreeMap;

use super::{INVALID_TRIGGER_SCRIPT_ID, Trigger, TriggerScript, TriggerScriptId};

/// Result of evaluating conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ConditionResult {
    True,
    False,
    /// Async condition waiting for input.
    Waiting,
}

/// The trigger engine manages and executes trigger scripts.
///
/// This is designed for deterministic execution - given the same
/// game state and scripts, it will always produce the same results
/// in the same order.
#[derive(Debug, Default)]
pub struct TriggerEngine {
    /// Active trigger scripts (BTreeMap for deterministic iteration).
    scripts: BTreeMap<TriggerScriptId, TriggerScript>,

    /// Next script ID to assign.
    next_script_id: TriggerScriptId,

    /// Performance tracking: number of evaluations this frame.
    evaluate_count: u32,

    /// Performance threshold for warnings.
    #[allow(dead_code)]
    performance_warning_threshold: u32,

    /// Infinite loop detection threshold.
    infinite_loop_threshold: u32,
}

impl TriggerEngine {
    /// Create a new trigger engine.
    pub fn new() -> Self {
        Self {
            scripts: BTreeMap::new(),
            next_script_id: 1,
            evaluate_count: 0,
            performance_warning_threshold: 1000,
            infinite_loop_threshold: 50000,
        }
    }

    /// Add a trigger script to the engine.
    pub fn add_script(&mut self, mut script: TriggerScript) -> TriggerScriptId {
        let id = if script.id == INVALID_TRIGGER_SCRIPT_ID {
            let id = self.next_script_id;
            self.next_script_id += 1;
            script.id = id;
            id
        } else {
            script.id
        };

        self.scripts.insert(id, script);
        id
    }

    /// Remove a trigger script.
    pub fn remove_script(&mut self, id: TriggerScriptId) -> Option<TriggerScript> {
        self.scripts.remove(&id)
    }

    /// Get a script by ID.
    pub fn get_script(&self, id: TriggerScriptId) -> Option<&TriggerScript> {
        self.scripts.get(&id)
    }

    /// Get a mutable script by ID.
    pub fn get_script_mut(&mut self, id: TriggerScriptId) -> Option<&mut TriggerScript> {
        self.scripts.get_mut(&id)
    }

    /// Activate a script.
    pub fn activate_script(&mut self, id: TriggerScriptId, current_time: u32) {
        if let Some(script) = self.scripts.get_mut(&id) {
            script.activate(current_time);
        }
    }

    /// Update all active trigger scripts.
    ///
    /// This is called once per simulation tick. It evaluates all active
    /// triggers and fires their effects.
    ///
    /// Returns the number of triggers that fired.
    pub fn update(&mut self, current_time: u32) -> u32 {
        self.evaluate_count = 0;
        let mut triggers_fired = 0;

        // Reset per-frame evaluation counts for all triggers
        for script in self.scripts.values_mut() {
            for trigger in &mut script.triggers {
                trigger.reset_evaluate_count();
            }
        }

        // Collect script IDs to iterate (avoid borrow issues)
        let script_ids: Vec<_> = self.scripts.keys().copied().collect();

        for script_id in script_ids {
            let script = match self.scripts.get_mut(&script_id) {
                Some(s) if s.is_active && !s.is_paused => s,
                _ => continue,
            };

            // Collect trigger indices that need evaluation
            let triggers_to_eval: Vec<_> = script
                .triggers
                .iter()
                .enumerate()
                .filter(|(_, t)| t.time_to_evaluate(current_time) && t.has_evaluations_remaining())
                .map(|(i, _)| i)
                .collect();

            for trigger_idx in triggers_to_eval {
                // Check for infinite loop
                if self.evaluate_count >= self.infinite_loop_threshold {
                    // Log warning and break
                    break;
                }

                let trigger = &mut script.triggers[trigger_idx];
                self.evaluate_count += 1;

                // Evaluate conditions
                let result = Self::evaluate_trigger_conditions(trigger, &script.variables);

                // Fire appropriate effects
                match result {
                    ConditionResult::True => {
                        // Effects would be executed here
                        triggers_fired += 1;
                    }
                    ConditionResult::False => {
                        // Fire on-false effects if any
                        if !trigger.effects_on_false.is_empty() {
                            triggers_fired += 1;
                        }
                    }
                    ConditionResult::Waiting => {
                        // Async condition, don't update timing
                        continue;
                    }
                }

                trigger.update_next_evaluate_time(current_time);
            }
        }

        triggers_fired
    }

    /// Evaluate a trigger's conditions.
    fn evaluate_trigger_conditions(
        trigger: &Trigger,
        _variables: &[super::script::TriggerVar],
    ) -> ConditionResult {
        if trigger.conditions.is_empty() {
            return ConditionResult::True;
        }

        // TODO: Actually evaluate conditions against game state
        // For now, return True as a placeholder
        ConditionResult::True
    }
}
