//! Authoritative scenario scoring configuration.

use super::World;
use crate::sync::SyncChecksum;

const TRIGGER_SCENARIO_ID: i32 = -1;

/// Campaign scoring parameters authored by retail trigger effect 984.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenarioScoreInfo {
    scenario_id: i32,
    combat_bonus_range: [f32; 2],
    mission_par_time_range_ms: [u32; 2],
    grade_score_thresholds: [i32; 3],
}

impl ScenarioScoreInfo {
    /// Scenario identifier supplied by the retail trigger effect.
    #[must_use]
    pub const fn scenario_id(self) -> i32 {
        self.scenario_id
    }

    /// Minimum combat-bonus multiplier.
    #[must_use]
    pub const fn combat_bonus_min_multiplier(self) -> f32 {
        self.combat_bonus_range[0]
    }

    /// Maximum combat-bonus multiplier.
    #[must_use]
    pub const fn combat_bonus_max_multiplier(self) -> f32 {
        self.combat_bonus_range[1]
    }

    /// Completion time at or below which the full time bonus applies.
    #[must_use]
    pub const fn mission_min_par_time_ms(self) -> u32 {
        self.mission_par_time_range_ms[0]
    }

    /// Completion time above which no time bonus applies.
    #[must_use]
    pub const fn mission_max_par_time_ms(self) -> u32 {
        self.mission_par_time_range_ms[1]
    }

    /// Score thresholds for Gold, Silver, and Bronze, in that order.
    #[must_use]
    pub const fn grade_score_thresholds(self) -> [i32; 3] {
        self.grade_score_thresholds
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.scenario_id);
        for multiplier in self.combat_bonus_range {
            checksum.hash_f32(multiplier);
        }
        for par_time in self.mission_par_time_range_ms {
            checksum.hash_u32(par_time);
        }
        for threshold in self.grade_score_thresholds {
            checksum.hash_i32(threshold);
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ScenarioScoreState {
    current: Option<ScenarioScoreInfo>,
}

impl ScenarioScoreState {
    fn set(
        &mut self,
        combat_bonus_range: [f32; 2],
        mission_par_time_range_ms: [u32; 2],
        grade_score_thresholds: [i32; 3],
    ) {
        self.current = Some(ScenarioScoreInfo {
            scenario_id: TRIGGER_SCENARIO_ID,
            combat_bonus_range,
            mission_par_time_range_ms,
            grade_score_thresholds,
        });
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        if let Some(info) = self.current {
            checksum.hash_u32(1);
            info.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
    }
}

impl World {
    /// Return scenario-authored scoring data after effect 984 has executed.
    #[must_use]
    pub const fn scenario_score_info(&self) -> Option<ScenarioScoreInfo> {
        self.scenario_score.current
    }

    pub(crate) fn set_scenario_score_info(
        &mut self,
        combat_bonus_range: [f32; 2],
        mission_par_time_range_ms: [u32; 2],
        grade_score_thresholds: [i32; 3],
    ) {
        self.scenario_score.set(
            combat_bonus_range,
            mission_par_time_range_ms,
            grade_score_thresholds,
        );
    }
}
