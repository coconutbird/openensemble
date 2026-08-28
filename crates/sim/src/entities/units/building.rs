//! Deterministic production state owned by class-1 building units.

use crate::entity_id::EntityId;
use crate::player::{PlayerId, Resources};
use crate::sync::SyncChecksum;

/// One paid technology item in a building's research queue.
#[derive(Debug, Clone)]
pub struct ResearchTask {
    pub(crate) player_id: PlayerId,
    pub(crate) technology_id: i32,
    pub(crate) technology_name: String,
    pub(crate) current_points: f32,
    pub(crate) total_points: f32,
    pub(crate) cost: Resources,
}

impl ResearchTask {
    #[must_use]
    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    #[must_use]
    pub fn technology_id(&self) -> i32 {
        self.technology_id
    }

    #[must_use]
    pub fn technology_name(&self) -> &str {
        &self.technology_name
    }

    #[must_use]
    pub fn current_points(&self) -> f32 {
        self.current_points
    }

    #[must_use]
    pub fn total_points(&self) -> f32 {
        self.total_points
    }

    #[must_use]
    pub fn fraction_complete(&self) -> f32 {
        if self.total_points > 0.0 {
            (self.current_points / self.total_points).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.technology_id);
        checksum.hash_u32(u32::try_from(self.technology_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.technology_name.as_bytes());
        checksum.hash_f32(self.current_points);
        checksum.hash_f32(self.total_points);
        for amount in self.cost.amounts {
            checksum.hash_f32(amount);
        }
    }
}

/// Retail-like single-worker research queue attached to a building unit.
#[derive(Debug, Clone, Default)]
pub struct BuildingProduction {
    pub(crate) current_research: Option<ResearchTask>,
    pub(crate) research_queue: Vec<ResearchTask>,
}

impl BuildingProduction {
    #[must_use]
    pub fn current_research(&self) -> Option<&ResearchTask> {
        self.current_research.as_ref()
    }

    pub fn queued_research(&self) -> impl Iterator<Item = &ResearchTask> {
        self.research_queue.iter()
    }

    #[must_use]
    pub fn queued_research_count(&self) -> usize {
        self.research_queue.len()
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.current_research.is_none() && self.research_queue.is_empty()
    }

    pub(crate) fn enqueue_research(&mut self, task: ResearchTask) {
        self.research_queue.push(task);
    }

    pub(crate) fn promote_research(&mut self) -> bool {
        if self.current_research.is_some() || self.research_queue.is_empty() {
            return false;
        }
        self.current_research = Some(self.research_queue.remove(0));
        true
    }

    pub(crate) fn cancel_research(
        &mut self,
        player_id: PlayerId,
        technology_id: i32,
    ) -> Option<ResearchTask> {
        if let Some(index) = self
            .research_queue
            .iter()
            .rposition(|task| task.player_id == player_id && task.technology_id == technology_id)
        {
            return Some(self.research_queue.remove(index));
        }
        self.current_research
            .as_ref()
            .is_some_and(|task| task.player_id == player_id && task.technology_id == technology_id)
            .then(|| self.current_research.take())
            .flatten()
    }

    pub(crate) fn research_task(
        &self,
        player_id: PlayerId,
        technology_id: i32,
    ) -> Option<(&ResearchTask, bool)> {
        if let Some(task) = self
            .current_research
            .as_ref()
            .filter(|task| task.player_id == player_id && task.technology_id == technology_id)
        {
            return Some((task, false));
        }
        self.research_queue
            .iter()
            .find(|task| task.player_id == player_id && task.technology_id == technology_id)
            .map(|task| (task, true))
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        if let Some(task) = &self.current_research {
            checksum.hash_u32(1);
            task.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::try_from(self.research_queue.len()).unwrap_or(u32::MAX));
        for task in &self.research_queue {
            task.hash_state(checksum);
        }
    }

    pub(crate) fn tasks(&self) -> impl Iterator<Item = &ResearchTask> {
        self.current_research
            .iter()
            .chain(self.research_queue.iter())
    }
}

/// Snapshot exposed to renderer/UI consumers without duplicating sim logic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResearchProgress {
    pub building_id: EntityId,
    pub current_points: f32,
    pub total_points: f32,
    pub queued: bool,
}

impl ResearchProgress {
    #[must_use]
    pub fn fraction_complete(self) -> f32 {
        if self.total_points > 0.0 {
            (self.current_points / self.total_points).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}
