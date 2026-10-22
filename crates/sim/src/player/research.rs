//! Player-global technology research assignments.

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use std::collections::BTreeMap;

/// Retail technology status values from `BTechTree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TechStatus {
    /// The player can never research the technology.
    Unobtainable = 0,
    /// The player cannot research the technology yet.
    #[default]
    Obtainable = 1,
    /// The player can research the technology now.
    Available = 2,
    /// A research of the technology is in progress.
    Researching = 3,
    /// The technology is researched and active.
    Active = 4,
    /// The technology is disabled.
    Disabled = 5,
    /// A co-op ally researches the technology.
    CoopResearching = 6,
}

#[derive(Debug, Clone, Copy)]
struct ResearchAssignment {
    building_id: EntityId,
    current_points: f32,
}

/// Per-player status needed while paid technology work is outstanding.
#[derive(Debug, Clone, Default)]
pub struct PlayerResearchState {
    assignments: BTreeMap<i32, ResearchAssignment>,
}

impl PlayerResearchState {
    /// Returns `true` if a building researches `technology_id` now.
    #[must_use]
    pub fn is_researching(&self, technology_id: i32) -> bool {
        self.assignments.contains_key(&technology_id)
    }

    /// Returns the building that researches `technology_id`, if any.
    #[must_use]
    pub fn research_building(&self, technology_id: i32) -> Option<EntityId> {
        self.assignments
            .get(&technology_id)
            .map(|assignment| assignment.building_id)
    }

    /// Returns the research points collected for `technology_id`, if a research is in progress.
    #[must_use]
    pub fn research_points(&self, technology_id: i32) -> Option<f32> {
        self.assignments
            .get(&technology_id)
            .map(|assignment| assignment.current_points)
    }

    pub(crate) fn start(&mut self, technology_id: i32, building_id: EntityId) -> bool {
        self.assignments
            .insert(
                technology_id,
                ResearchAssignment {
                    building_id,
                    current_points: 0.0,
                },
            )
            .is_none()
    }

    pub(crate) fn set_points(&mut self, technology_id: i32, building_id: EntityId, points: f32) {
        if let Some(assignment) = self.assignments.get_mut(&technology_id)
            && assignment.building_id == building_id
        {
            assignment.current_points = points;
        }
    }

    pub(crate) fn stop(&mut self, technology_id: i32, building_id: EntityId) -> bool {
        let matches = self
            .assignments
            .get(&technology_id)
            .is_some_and(|assignment| assignment.building_id == building_id);
        if matches {
            self.assignments.remove(&technology_id);
        }
        matches
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.assignments.len()).unwrap_or(u32::MAX));
        for (&technology_id, assignment) in &self.assignments {
            checksum.hash_i32(technology_id);
            checksum.hash_u32(assignment.building_id.as_u32());
            checksum.hash_f32(assignment.current_points);
        }
    }
}
