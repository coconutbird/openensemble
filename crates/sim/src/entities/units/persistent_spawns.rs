//! Runtime progress for persistent `SpawnSquad` unit actions.

use super::Unit;
use crate::sync::SyncChecksum;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SpawnPhase {
    #[default]
    Fresh,
    Working,
    Done,
}

/// One connected retail spawn action's synchronized mutable state.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpawnSquadActionState {
    phase: SpawnPhase,
    current_points: f32,
    work_rate_variance: f32,
    completed_count: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UnitPersistentSpawns {
    actions: BTreeMap<String, SpawnSquadActionState>,
}

impl UnitPersistentSpawns {
    pub(crate) fn action_mut(&mut self, action_name: &str) -> &mut SpawnSquadActionState {
        self.actions
            .entry(action_name.to_ascii_lowercase())
            .or_default()
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.actions.len()).unwrap_or(u32::MAX));
        for (name, state) in &self.actions {
            checksum.hash_u32(u32::try_from(name.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(name.as_bytes());
            state.hash_state(checksum);
        }
    }
}

impl SpawnSquadActionState {
    pub(crate) fn advance(
        &mut self,
        elapsed: f32,
        build_points: f32,
        work_rate: f32,
        count: u32,
        auto_join: bool,
        live_auto_joins: u32,
    ) -> bool {
        if self.phase == SpawnPhase::Done {
            if !auto_join || count == 0 || live_auto_joins >= count {
                return false;
            }
            self.phase = SpawnPhase::Fresh;
            self.current_points = 0.0;
            self.work_rate_variance = 0.0;
            self.completed_count = live_auto_joins;
        }
        if self.phase == SpawnPhase::Fresh {
            if auto_join {
                self.completed_count = live_auto_joins;
            }
            if count > 0 && self.completed_count >= count {
                self.phase = SpawnPhase::Done;
            } else {
                self.phase = SpawnPhase::Working;
            }
            return false;
        }
        if !elapsed.is_finite() || elapsed <= 0.0 {
            return false;
        }
        self.current_points += elapsed;
        let threshold = if work_rate > 0.0 {
            work_rate + self.work_rate_variance
        } else {
            build_points
        };
        self.current_points >= threshold
    }

    pub(crate) fn finish_attempt(
        &mut self,
        spawned: bool,
        build_points: f32,
        count: u32,
        next_variance: f32,
    ) {
        self.work_rate_variance = next_variance;
        if spawned {
            self.completed_count = self.completed_count.saturating_add(1);
            self.current_points = 0.0;
            if count > 0 && self.completed_count >= count {
                self.phase = SpawnPhase::Done;
            }
        } else {
            self.current_points = build_points;
        }
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(match self.phase {
            SpawnPhase::Fresh => 0,
            SpawnPhase::Working => 1,
            SpawnPhase::Done => 2,
        });
        checksum.hash_f32(self.current_points);
        checksum.hash_f32(self.work_rate_variance);
        checksum.hash_u32(self.completed_count);
    }
}

impl Unit {
    pub(crate) fn hash_persistent_spawn_state(&self, checksum: &mut SyncChecksum) {
        self.persistent_spawns.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_auto_join_actions_reopen_after_a_follower_disappears() {
        let mut state = SpawnSquadActionState::default();
        assert!(!state.advance(0.05, 0.0, 0.0, 2, true, 0));
        assert!(state.advance(0.05, 0.0, 0.0, 2, true, 0));
        state.finish_attempt(true, 0.0, 2, 0.0);
        assert!(state.advance(0.05, 0.0, 0.0, 2, true, 1));
        state.finish_attempt(true, 0.0, 2, 0.0);
        assert!(!state.advance(0.05, 0.0, 0.0, 2, true, 2));

        assert!(!state.advance(0.05, 0.0, 0.0, 2, true, 1));
        assert!(state.advance(0.05, 0.0, 0.0, 2, true, 1));
    }

    #[test]
    fn failed_attempt_uses_build_points_for_the_retail_retry_state() {
        let mut state = SpawnSquadActionState::default();
        assert!(!state.advance(0.1, 20.0, 10.0, 0, false, 0));
        assert!(!state.advance(9.8, 20.0, 10.0, 0, false, 0));
        assert!(state.advance(0.2, 20.0, 10.0, 0, false, 0));
        state.finish_attempt(false, 20.0, 0, 1.5);
        assert!(state.advance(0.01, 20.0, 10.0, 0, false, 0));
    }
}
