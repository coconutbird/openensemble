//! Scenario-authored objective state used by gameplay triggers and UI clients.

use super::World;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;
use crate::trigger::ObjectiveId;

const DEFAULT_TRACKER_DURATION_MS: u32 = 8_000;
const DEFAULT_MIN_TRACKER_INCREMENT: u32 = 1;

/// Authoritative mutable state for one scenario objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectiveState {
    id: ObjectiveId,
    player_mask: u8,
    required: bool,
    score: u32,
    tracker_duration_ms: u32,
    min_tracker_increment: u32,
    current_count: i32,
    final_count: i32,
    completed: bool,
    displayed: bool,
}

impl ObjectiveState {
    pub(crate) const fn new(id: ObjectiveId) -> Self {
        Self {
            id,
            player_mask: 0,
            required: false,
            score: 0,
            tracker_duration_ms: DEFAULT_TRACKER_DURATION_MS,
            min_tracker_increment: DEFAULT_MIN_TRACKER_INCREMENT,
            current_count: -1,
            final_count: -1,
            completed: false,
            displayed: false,
        }
    }

    /// Retail objective identifier referenced by trigger variables.
    #[must_use]
    pub const fn id(self) -> ObjectiveId {
        self.id
    }

    /// Whether the objective is required for scenario completion.
    #[must_use]
    pub const fn required(self) -> bool {
        self.required
    }

    /// Whether this objective is assigned to the given retail player slot.
    #[must_use]
    pub fn assigned_to_player(self, player_id: PlayerId) -> bool {
        player_bit(player_id).is_some_and(|bit| self.player_mask & bit != 0)
    }

    /// Campaign score awarded by this objective.
    #[must_use]
    pub const fn score(self) -> u32 {
        self.score
    }

    /// Minimum duration for objective tracker presentation.
    #[must_use]
    pub const fn tracker_duration_ms(self) -> u32 {
        self.tracker_duration_ms
    }

    /// Minimum counter delta that refreshes the objective tracker.
    #[must_use]
    pub const fn min_tracker_increment(self) -> u32 {
        self.min_tracker_increment
    }

    /// Current progress, or `-1` for an objective without initialized progress.
    #[must_use]
    pub const fn current_count(self) -> i32 {
        self.current_count
    }

    /// Authored destination count, or `-1` for a non-counter objective.
    #[must_use]
    pub const fn final_count(self) -> i32 {
        self.final_count
    }

    /// Whether gameplay has completed the objective.
    #[must_use]
    pub const fn completed(self) -> bool {
        self.completed
    }

    /// Whether gameplay has discovered/displayed the objective.
    #[must_use]
    pub const fn displayed(self) -> bool {
        self.displayed
    }

    pub(crate) fn set_required(&mut self, required: bool) {
        self.required = required;
    }

    pub(crate) fn assign_player(&mut self, player_id: PlayerId) {
        if let Some(bit) = player_bit(player_id) {
            self.player_mask |= bit;
        }
    }

    pub(crate) fn set_score(&mut self, score: u32) {
        self.score = score;
    }

    pub(crate) fn set_tracker_duration_ms(&mut self, duration_ms: u32) {
        self.tracker_duration_ms = duration_ms;
    }

    pub(crate) fn set_min_tracker_increment(&mut self, increment: u32) {
        self.min_tracker_increment = increment;
    }

    pub(crate) fn set_final_count(&mut self, count: i32) {
        self.final_count = count;
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.id);
        checksum.hash_u32(u32::from(self.player_mask));
        checksum.hash_u32(u32::from(self.required));
        checksum.hash_u32(self.score);
        checksum.hash_u32(self.tracker_duration_ms);
        checksum.hash_u32(self.min_tracker_increment);
        checksum.hash_i32(self.current_count);
        checksum.hash_i32(self.final_count);
        checksum.hash_u32(u32::from(self.completed));
        checksum.hash_u32(u32::from(self.displayed));
    }
}

impl World {
    /// Iterate objectives in their authored scenario order.
    pub fn objectives(&self) -> impl Iterator<Item = &ObjectiveState> {
        self.objectives.iter()
    }

    /// Return an objective by its sparse retail identifier.
    #[must_use]
    pub fn objective(&self, objective_id: ObjectiveId) -> Option<&ObjectiveState> {
        self.objectives
            .iter()
            .find(|objective| objective.id == objective_id)
    }

    /// Match retail's counter query, including its zero fallback for invalid IDs.
    #[must_use]
    pub fn objective_current_count(&self, objective_id: ObjectiveId) -> i32 {
        self.objective(objective_id)
            .map_or(0, |objective| objective.current_count)
    }

    /// Match retail's final-counter query, including its zero fallback for invalid IDs.
    #[must_use]
    pub fn objective_final_count(&self, objective_id: ObjectiveId) -> i32 {
        self.objective(objective_id)
            .map_or(0, |objective| objective.final_count)
    }

    pub(crate) fn configure_objectives(&mut self, objectives: Vec<ObjectiveState>) {
        self.objectives = objectives;
    }

    pub(crate) fn set_objective_current_count(
        &mut self,
        objective_id: ObjectiveId,
        count: i32,
    ) -> bool {
        let Some(objective) = self
            .objectives
            .iter_mut()
            .find(|objective| objective.id == objective_id)
        else {
            return false;
        };
        objective.current_count = count;
        true
    }

    pub(super) fn hash_objectives(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.objectives.len()).unwrap_or(u32::MAX));
        for objective in &self.objectives {
            objective.hash_state(checksum);
        }
    }
}

fn player_bit(player_id: PlayerId) -> Option<u8> {
    (1..=6)
        .contains(&player_id)
        .then(|| 1_u8 << (player_id - 1))
}

#[cfg(test)]
#[path = "objectives/tests.rs"]
mod tests;
