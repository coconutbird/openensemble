//! Checksummed squad order and per-member progress for retail Mines work.

use super::{Squad, SquadState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

#[derive(Debug, Clone, Copy)]
struct MineWorker {
    unit_id: EntityId,
    placed_any: bool,
    finished: bool,
}

/// Immutable target context copied from one Mines work command.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MineOrder {
    pub(crate) target_entity: Option<EntityId>,
    pub(crate) target_position: Option<Vec3>,
    pub(crate) explicit_range: Option<f32>,
    pub(crate) requested_ability_id: u8,
}

/// Active retail squad work action with one child opportunity per member.
#[derive(Debug, Clone, Default)]
pub(crate) struct SquadMines {
    order: Option<MineOrder>,
    workers: Vec<MineWorker>,
}

impl SquadMines {
    fn begin(&mut self, order: MineOrder, unit_ids: &[EntityId]) -> bool {
        if unit_ids.is_empty() {
            return false;
        }
        self.order = Some(order);
        self.workers.clear();
        self.workers
            .extend(unit_ids.iter().copied().map(|unit_id| MineWorker {
                unit_id,
                placed_any: false,
                finished: false,
            }));
        self.workers.sort_unstable_by_key(|worker| worker.unit_id);
        self.workers.dedup_by_key(|worker| worker.unit_id);
        true
    }

    pub(crate) const fn order(&self) -> Option<MineOrder> {
        self.order
    }

    pub(crate) fn pending_workers(&self) -> Vec<EntityId> {
        self.workers
            .iter()
            .filter_map(|worker| (!worker.finished).then_some(worker.unit_id))
            .collect()
    }

    pub(crate) fn mark_placed(&mut self, unit_id: EntityId) {
        if let Some(worker) = self
            .workers
            .iter_mut()
            .find(|worker| worker.unit_id == unit_id)
        {
            worker.placed_any = true;
        }
    }

    pub(crate) fn finish_worker(&mut self, unit_id: EntityId) {
        if let Some(worker) = self
            .workers
            .iter_mut()
            .find(|worker| worker.unit_id == unit_id)
        {
            worker.finished = true;
        }
    }

    pub(crate) fn all_finished(&self) -> bool {
        self.order.is_some() && self.workers.iter().all(|worker| worker.finished)
    }

    pub(super) fn cancel(&mut self) {
        self.order = None;
        self.workers.clear();
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        if let Some(order) = self.order {
            checksum.hash_u32(1);
            checksum.hash_u32(order.target_entity.map_or(u32::MAX, EntityId::as_u32));
            if let Some(position) = order.target_position {
                checksum.hash_u32(1);
                checksum.hash_vec3(position.x, position.y, position.z);
            } else {
                checksum.hash_u32(0);
            }
            checksum.hash_u32(u32::from(order.explicit_range.is_some()));
            checksum.hash_f32(order.explicit_range.unwrap_or_default());
            checksum.hash_u32(u32::from(order.requested_ability_id));
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::try_from(self.workers.len()).unwrap_or(u32::MAX));
        for worker in &self.workers {
            checksum.hash_u32(worker.unit_id.as_u32());
            checksum.hash_u32(u32::from(worker.placed_any));
            checksum.hash_u32(u32::from(worker.finished));
        }
    }
}

impl Squad {
    /// Return whether this squad is executing a Mines work order.
    #[must_use]
    pub fn is_placing_mines(&self) -> bool {
        self.mines.order.is_some()
    }

    pub(crate) fn begin_mines_order(&mut self, order: MineOrder) -> bool {
        if !self.is_alive()
            || self.garrison.is_garrisoned()
            || order.target_entity.is_none() && order.target_position.is_none()
            || order
                .target_position
                .is_some_and(|position| !position.is_finite())
        {
            return false;
        }
        let unit_ids = self.unit_ids.clone();
        self.remove_all_orders();
        if !self.mines.begin(order, &unit_ids) {
            return false;
        }
        self.state = SquadState::Working;
        self.base.velocity = Vec3::ZERO;
        self.cancel_idle_action();
        true
    }

    pub(crate) fn cancel_mines_order(&mut self) {
        self.mines.cancel();
        if self.state == SquadState::Working {
            self.state = SquadState::Idle;
            self.cancel_idle_action();
        }
    }

    pub(crate) fn complete_mines_order(&mut self) {
        self.mines.cancel();
        if !self.begin_next_queued_move() && self.state == SquadState::Working {
            self.state = SquadState::Idle;
            self.cancel_idle_action();
        }
    }

    pub(crate) fn hash_mines_state(&self, checksum: &mut SyncChecksum) {
        self.mines.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn workers_are_sorted_checksummed_and_reset() {
        let mut squad = Squad::new(EntityId::new(EntityClass::Squad, 1), 1);
        let first = EntityId::new(EntityClass::Unit, 3);
        let second = EntityId::new(EntityClass::Unit, 2);
        squad.unit_ids = vec![first, second];
        assert!(squad.begin_mines_order(MineOrder {
            target_entity: None,
            target_position: Some(Vec3::ZERO),
            explicit_range: None,
            requested_ability_id: 4,
        }));
        assert_eq!(squad.mines.pending_workers(), vec![second, first]);
        squad.mines.mark_placed(second);
        squad.mines.finish_worker(second);
        assert!(
            squad
                .mines
                .workers
                .iter()
                .find(|worker| worker.unit_id == second)
                .unwrap()
                .placed_any
        );
        assert!(!squad.mines.all_finished());
        squad.cancel_mines_order();
        assert!(!squad.is_placing_mines());
        assert_eq!(squad.state, SquadState::Idle);
    }
}
