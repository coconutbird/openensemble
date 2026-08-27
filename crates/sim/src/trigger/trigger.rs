//! Trigger - a condition/effect pair that executes when conditions are met.

use super::{Condition, Effect, TriggerId};

/// How a trigger combines its conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConditionMode {
    /// Every condition must be true.
    #[default]
    All,
    /// At least one condition must be true.
    Any,
}

/// A trigger that evaluates conditions and fires effects.
///
/// Triggers are the core building block of the scripting system.
/// When active, they periodically evaluate their conditions and
/// fire the appropriate effects based on the result.
#[derive(Debug, Clone)]
pub struct Trigger {
    /// Unique ID within the parent script.
    pub id: TriggerId,

    /// Editor-assigned ID (for debugging/tooling).
    pub editor_id: TriggerId,

    /// Human-readable name (debug only).
    pub name: String,

    /// Group ID for organizational purposes.
    pub group_id: i32,

    /// Conditions to evaluate (AND'd together by default).
    pub conditions: Vec<Condition>,

    /// Effects to fire when conditions evaluate to true.
    pub effects_on_true: Vec<Effect>,

    /// Effects to fire when conditions evaluate to false.
    pub effects_on_false: Vec<Effect>,

    /// How this trigger combines its conditions.
    pub condition_mode: ConditionMode,

    /// Whether this trigger starts active.
    pub start_active: bool,

    /// Whether this is a "conditional" trigger (can be called as a condition).
    pub is_conditional: bool,

    /// Time when this trigger was activated (game time in ms).
    pub activated_time: u32,

    /// Next time to evaluate conditions (game time in ms).
    pub next_evaluate_time: u32,

    /// How often to evaluate (ms between evaluations).
    pub evaluate_frequency: u32,

    /// How many times evaluated this frame.
    pub evaluate_count: u32,

    /// Max evaluations per frame (0 = unlimited).
    pub evaluate_limit: u32,

    /// Whether currently active.
    pub is_active: bool,
}

impl Default for Trigger {
    fn default() -> Self {
        Self {
            id: super::INVALID_TRIGGER_ID,
            editor_id: super::INVALID_TRIGGER_ID,
            name: String::new(),
            group_id: -1,
            conditions: Vec::new(),
            effects_on_true: Vec::new(),
            effects_on_false: Vec::new(),
            condition_mode: ConditionMode::All,
            start_active: false,
            is_conditional: false,
            activated_time: 0,
            next_evaluate_time: 0,
            evaluate_frequency: 0,
            evaluate_count: 0,
            evaluate_limit: 0,
            is_active: false,
        }
    }
}

impl Trigger {
    /// Create a new trigger with the given ID.
    #[must_use]
    pub fn new(id: TriggerId) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }

    /// Set the trigger name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add a condition.
    #[must_use]
    pub fn with_condition(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Add an effect for when conditions are true.
    #[must_use]
    pub fn with_effect_on_true(mut self, effect: Effect) -> Self {
        self.effects_on_true.push(effect);
        self
    }

    /// Add an effect for when conditions are false.
    #[must_use]
    pub fn with_effect_on_false(mut self, effect: Effect) -> Self {
        self.effects_on_false.push(effect);
        self
    }

    /// Set to start active.
    #[must_use]
    pub fn starts_active(mut self) -> Self {
        self.start_active = true;
        self.is_active = true;
        self
    }

    /// Set evaluation frequency.
    #[must_use]
    pub fn with_frequency(mut self, frequency_ms: u32) -> Self {
        self.evaluate_frequency = frequency_ms;
        self
    }

    /// Check if it's time to evaluate this trigger.
    #[must_use]
    pub fn time_to_evaluate(&self, current_time: u32) -> bool {
        if !self.is_active {
            return false;
        }
        current_time >= self.next_evaluate_time
    }

    /// Check if there are evaluations remaining this frame.
    #[must_use]
    pub fn has_evaluations_remaining(&self) -> bool {
        self.evaluate_limit == 0 || self.evaluate_count < self.evaluate_limit
    }

    /// Called when the trigger is activated.
    pub fn on_activated(&mut self, current_time: u32) {
        self.is_active = true;
        self.activated_time = current_time;
        self.next_evaluate_time = current_time;
    }

    /// Called when the trigger is deactivated.
    pub fn on_deactivated(&mut self) {
        self.is_active = false;
    }

    /// Reset the per-frame evaluation count.
    pub fn reset_evaluate_count(&mut self) {
        self.evaluate_count = 0;
    }

    /// Update the next evaluation time after an evaluation.
    pub fn update_next_evaluate_time(&mut self, current_time: u32) {
        self.evaluate_count += 1;
        self.next_evaluate_time = current_time + self.evaluate_frequency;
    }
}
