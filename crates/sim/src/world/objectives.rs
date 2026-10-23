//! Scenario-authored objective state used by gameplay triggers and UI clients.
//!
//! The state follows retail `BObjectiveManager` (`gWorld+600`): the objective list in scenario
//! order, the objective notifications of `BObjectiveManager_addObjectiveNotification`
//! (`0x1402B0610`) and the objective score of `BScoreManager_reportObjectiveCompleted`
//! (`0x1403875F0`).

use std::collections::{BTreeMap, VecDeque};

use super::World;
use crate::player::{PlayerId, PlayerType};
use crate::sync::SyncChecksum;
use crate::trigger::ObjectiveId;

const DEFAULT_TRACKER_DURATION_MS: u32 = 8_000;
const DEFAULT_MIN_TRACKER_INCREMENT: u32 = 1;
/// Notification time in seconds when the trigger gives none (`flt_1411F40BC`).
pub(crate) const DEFAULT_NOTIFICATION_SECONDS: f32 = 10.0;
/// Notifications that the world keeps for UI clients that read them late.
const MAX_RETAINED_NOTIFICATIONS: usize = 64;

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
    completed_at_ms: u32,
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
            completed_at_ms: 0,
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

    /// Game time in milliseconds when gameplay completed the objective, or 0 if it is not
    /// complete.
    #[must_use]
    pub const fn completed_at_ms(self) -> u32 {
        self.completed_at_ms
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
        checksum.hash_u32(self.completed_at_ms);
        checksum.hash_u32(u32::from(self.displayed));
    }
}

/// One objective notification for the objective tracker UI (retail `BObjectiveMessage`).
///
/// Retail adds a notification when an objective becomes displayed or becomes not complete
/// again. A completion adds none. The UI reads the notifications after its own cursor with
/// [`World::objective_notifications_after`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectiveNotification {
    sequence: u64,
    objective_id: ObjectiveId,
    duration_s: f32,
}

impl ObjectiveNotification {
    /// The sequence number of the notification. It increases with each notification.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// The objective that the notification is for.
    #[must_use]
    pub const fn objective_id(self) -> ObjectiveId {
        self.objective_id
    }

    /// The display time in seconds. Zero tells the UI to keep the notification.
    #[must_use]
    pub const fn duration_s(self) -> f32 {
        self.duration_s
    }
}

/// The objective list, the objective score and the notifications of the world.
#[derive(Debug, Default)]
pub(super) struct ObjectiveManager {
    list: Vec<ObjectiveState>,
    player_scores: BTreeMap<PlayerId, i32>,
    notifications: VecDeque<ObjectiveNotification>,
    latest_notification_sequence: u64,
}

impl ObjectiveManager {
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    fn index_of(&self, objective_id: ObjectiveId) -> Option<usize> {
        self.list
            .iter()
            .position(|objective| objective.id == objective_id)
    }

    /// `BObjectiveManager_addObjectiveNotification`. Retail resets the time of a message that
    /// it still shows for the same objective. The UI gets the same result when it replaces the
    /// shown message of the objective with the newer notification.
    fn notify(&mut self, objective_id: ObjectiveId, duration_s: f32) {
        self.latest_notification_sequence =
            self.latest_notification_sequence.wrapping_add(1).max(1);
        if self.notifications.len() == MAX_RETAINED_NOTIFICATIONS {
            self.notifications.pop_front();
        }
        self.notifications.push_back(ObjectiveNotification {
            sequence: self.latest_notification_sequence,
            objective_id,
            duration_s,
        });
    }
}

impl World {
    /// Iterate objectives in their authored scenario order.
    pub fn objectives(&self) -> impl Iterator<Item = &ObjectiveState> {
        self.objectives.list.iter()
    }

    /// Return an objective by its sparse retail identifier.
    #[must_use]
    pub fn objective(&self, objective_id: ObjectiveId) -> Option<&ObjectiveState> {
        self.objectives
            .list
            .iter()
            .find(|objective| objective.id == objective_id)
    }

    /// Return the objective at a position of the scenario order.
    ///
    /// `IsObjectiveComplete` (746) uses the objective ID of its input as this position, not as
    /// an ID (`BTriggerCondition_evaluate` case 746).
    #[must_use]
    pub fn objective_at_index(&self, index: i32) -> Option<&ObjectiveState> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.objectives.list.get(index))
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

    /// The objective score of a player: the sum of the scores of the objectives that are
    /// complete. Only human players get objective score.
    #[must_use]
    pub fn player_objective_score(&self, player_id: PlayerId) -> i32 {
        self.objectives
            .player_scores
            .get(&player_id)
            .copied()
            .unwrap_or(0)
    }

    /// The sequence number of the latest objective notification, or 0 if there is none.
    #[must_use]
    pub const fn latest_objective_notification_sequence(&self) -> u64 {
        self.objectives.latest_notification_sequence
    }

    /// Iterate the retained objective notifications that are newer than a UI cursor.
    pub fn objective_notifications_after(
        &self,
        sequence: u64,
    ) -> impl Iterator<Item = &ObjectiveNotification> {
        self.objectives
            .notifications
            .iter()
            .filter(move |notification| {
                notification.sequence != sequence
                    && notification.sequence.wrapping_sub(sequence) < (1_u64 << 63)
            })
    }

    pub(crate) fn configure_objectives(&mut self, objectives: Vec<ObjectiveState>) {
        self.objectives.list = objectives;
    }

    pub(crate) fn set_objective_current_count(
        &mut self,
        objective_id: ObjectiveId,
        count: i32,
    ) -> bool {
        let Some(index) = self.objectives.index_of(objective_id) else {
            return false;
        };
        self.objectives.list[index].current_count = count;
        true
    }

    /// `BObjectiveManager_setObjectiveCompleted` (`0x1402B04F0`).
    ///
    /// A change stores the completion time, adds the objective score to each human player
    /// (or removes it), and adds a notification if the objective is not complete again.
    /// Returns `false` for an unknown objective ID.
    pub(crate) fn set_objective_completed(
        &mut self,
        objective_id: ObjectiveId,
        completed: bool,
    ) -> bool {
        let Some(index) = self.objectives.index_of(objective_id) else {
            return false;
        };
        let game_time_ms = self.game_time_ms;
        let objective = &mut self.objectives.list[index];
        if objective.completed == completed {
            return true;
        }
        objective.completed = completed;
        objective.completed_at_ms = if completed { game_time_ms } else { 0 };
        let score = objective.score.cast_signed();
        self.report_objective_score(completed, score);
        if !completed {
            self.objectives
                .notify(objective_id, DEFAULT_NOTIFICATION_SECONDS);
        }
        true
    }

    /// `BObjectiveManager_setObjectiveDisplayed` (`0x1402B0430`). Each call sets the flag and,
    /// if the objective is displayed, adds a notification. Returns `false` for an unknown
    /// objective ID.
    pub(crate) fn set_objective_displayed(
        &mut self,
        objective_id: ObjectiveId,
        displayed: bool,
        duration_s: f32,
    ) -> bool {
        let Some(index) = self.objectives.index_of(objective_id) else {
            return false;
        };
        self.objectives.list[index].displayed = displayed;
        if displayed {
            self.objectives.notify(objective_id, duration_s);
        }
        true
    }

    /// `BScoreManager_reportObjectiveCompleted` (`0x1403875F0`). Retail visits the score
    /// players and changes the objective score of each human player. It does not halve the
    /// score in a co-op game. The sim has no skulls, so the skull bonus is always zero.
    fn report_objective_score(&mut self, completed: bool, score: i32) {
        for player in &self.players {
            if player.player_type != PlayerType::Human {
                continue;
            }
            let total = self.objectives.player_scores.entry(player.id).or_insert(0);
            *total = if completed {
                total.wrapping_add(score)
            } else {
                total.wrapping_sub(score)
            };
        }
    }

    pub(super) fn hash_objectives(&self, checksum: &mut SyncChecksum) {
        let objectives = &self.objectives;
        checksum.hash_u32(u32::try_from(objectives.list.len()).unwrap_or(u32::MAX));
        for objective in &objectives.list {
            objective.hash_state(checksum);
        }
        checksum.hash_u32(u32::try_from(objectives.player_scores.len()).unwrap_or(u32::MAX));
        for (player_id, score) in &objectives.player_scores {
            checksum.hash_u32(u32::from(*player_id));
            checksum.hash_i32(*score);
        }
        checksum.hash_bytes(&objectives.latest_notification_sequence.to_le_bytes());
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
