//! Renderer/platform handoff for simulation-owned rumble requests.

use std::collections::BTreeSet;

use sim::{PlayerId, RumbleRequest, World};

/// Starts and stops needed to synchronize one local gamepad with the sim.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RumbleProjection {
    /// Newly active requests for the platform rumble backend.
    pub started: Vec<RumbleRequest>,
    /// Request IDs no longer active in the simulation.
    pub stopped: Vec<i32>,
}

/// Renderer-local cursor over authoritative rumble request IDs.
#[derive(Debug, Default, Clone)]
pub struct SimulationRumbleAdapter {
    active_ids: BTreeSet<i32>,
}

impl SimulationRumbleAdapter {
    /// Return only the platform changes needed for this synchronization.
    pub fn synchronize(&mut self, world: &World, player_id: PlayerId) -> RumbleProjection {
        let requests = world.rumble_requests(player_id).collect::<Vec<_>>();
        let current_ids = requests.iter().map(|request| request.id()).collect();
        let started = requests
            .into_iter()
            .filter(|request| !self.active_ids.contains(&request.id()))
            .cloned()
            .collect();
        let stopped = self.active_ids.difference(&current_ids).copied().collect();
        self.active_ids = current_ids;
        RumbleProjection { started, stopped }
    }

    /// Forget platform state after a device or scenario reset.
    pub fn reset(&mut self) {
        self.active_ids.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_sim_requests_stop_renderer_local_ids_once() {
        let world = World::new();
        let mut adapter = SimulationRumbleAdapter {
            active_ids: BTreeSet::from([4, 9]),
        };
        let changes = adapter.synchronize(&world, 1);
        assert!(changes.started.is_empty());
        assert_eq!(changes.stopped, [4, 9]);
        assert_eq!(adapter.synchronize(&world, 1), RumbleProjection::default());
    }
}
