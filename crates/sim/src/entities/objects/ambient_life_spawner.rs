//! Synchronized state for a persistent class-zero `AmbientLifeSpawner` action.

use super::Object;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
enum AmbientLifeSpawnerPhase {
    #[default]
    Disconnected = 0,
    Starting = 1,
    Working = 2,
    Done = 3,
}

/// Mutable state owned by one ambient-life spawner object.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ObjectAmbientLifeSpawner {
    phase: AmbientLifeSpawnerPhase,
    opportunity_timer_ms: u32,
}

impl ObjectAmbientLifeSpawner {
    pub(crate) fn connect(&mut self, check_frequency_ms: u32) {
        if self.phase == AmbientLifeSpawnerPhase::Disconnected {
            self.phase = AmbientLifeSpawnerPhase::Starting;
            self.opportunity_timer_ms = check_frequency_ms;
        }
    }

    pub(crate) fn start(&mut self) -> bool {
        if self.phase != AmbientLifeSpawnerPhase::Starting {
            return false;
        }
        self.phase = AmbientLifeSpawnerPhase::Working;
        true
    }

    pub(crate) fn opportunity_due(&mut self, elapsed_ms: u32, check_frequency_ms: u32) -> bool {
        if self.phase != AmbientLifeSpawnerPhase::Working {
            return false;
        }
        if self.opportunity_timer_ms <= elapsed_ms {
            self.opportunity_timer_ms = check_frequency_ms;
            true
        } else {
            self.opportunity_timer_ms -= elapsed_ms;
            false
        }
    }

    pub(crate) fn complete(&mut self) {
        self.phase = AmbientLifeSpawnerPhase::Done;
    }

    pub(crate) fn disconnect(&mut self) {
        *self = Self::default();
    }

    const fn is_connected(self) -> bool {
        !matches!(self.phase, AmbientLifeSpawnerPhase::Disconnected)
    }

    const fn is_complete(self) -> bool {
        matches!(self.phase, AmbientLifeSpawnerPhase::Done)
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.opportunity_timer_ms);
    }
}

impl Object {
    /// Whether an authored ambient-life spawner action is connected.
    #[must_use]
    pub const fn has_ambient_life_spawner(&self) -> bool {
        self.ambient_life_spawner.is_connected()
    }

    /// Whether this one-shot spawner has created its ambient squad.
    #[must_use]
    pub const fn ambient_life_spawn_complete(&self) -> bool {
        self.ambient_life_spawner.is_complete()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_update_only_enters_working_state() {
        let mut state = ObjectAmbientLifeSpawner::default();
        state.connect(1_000);

        assert!(state.start());
        assert_eq!(state.opportunity_timer_ms, 1_000);
        assert!(state.opportunity_due(1_000, 1_000));
    }

    #[test]
    fn missed_opportunity_resets_the_full_interval() {
        let mut state = ObjectAmbientLifeSpawner::default();
        state.connect(100);
        assert!(state.start());

        assert!(state.opportunity_due(100, 100));
        assert!(!state.opportunity_due(99, 100));
        assert!(state.opportunity_due(1, 100));
        state.complete();
        assert!(!state.opportunity_due(100, 100));
    }
}
