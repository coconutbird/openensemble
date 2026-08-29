//! Authoritative squad movement-command state.

use super::{Squad, SquadState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
struct SquadMoveOrder {
    target: Vec3,
    attack_move: bool,
}

impl SquadMoveOrder {
    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_vec3(self.target.x, self.target.y, self.target.z);
        checksum.hash_u32(u32::from(self.attack_move));
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SquadOrders {
    active_move: Option<SquadMoveOrder>,
    queued_moves: VecDeque<SquadMoveOrder>,
    auto_attack_origin: Option<Vec3>,
}

impl SquadOrders {
    fn clear(&mut self) {
        self.active_move = None;
        self.queued_moves.clear();
        self.auto_attack_origin = None;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.active_move.is_some()));
        if let Some(order) = self.active_move {
            order.hash_state(checksum);
        }
        checksum.hash_u32(u32::try_from(self.queued_moves.len()).unwrap_or(u32::MAX));
        for order in &self.queued_moves {
            order.hash_state(checksum);
        }
        checksum.hash_u32(u32::from(self.auto_attack_origin.is_some()));
        if let Some(origin) = self.auto_attack_origin {
            checksum.hash_vec3(origin.x, origin.y, origin.z);
        }
    }
}

impl Squad {
    pub(crate) fn issue_scripted_move(
        &mut self,
        target: Vec3,
        attack_move: bool,
        queue: bool,
    ) -> bool {
        self.issue_scripted_path(&[target], attack_move, queue)
    }

    pub(crate) fn issue_scripted_path(
        &mut self,
        targets: &[Vec3],
        attack_move: bool,
        queue: bool,
    ) -> bool {
        if targets.is_empty()
            || targets.iter().any(|target| !target.is_finite())
            || !self.is_alive()
            || !self.base.is_mobile()
            || self.garrison.is_garrisoned()
        {
            return false;
        }
        let mut orders = targets.iter().copied().map(|target| SquadMoveOrder {
            target,
            attack_move,
        });
        self.garrison.cancel_pending();
        if queue
            && (self.orders.active_move.is_some()
                || !self.orders.queued_moves.is_empty()
                || self.state != SquadState::Idle)
        {
            self.orders.queued_moves.extend(orders);
            self.cancel_idle_action();
            return true;
        }
        self.orders.clear();
        let Some(first) = orders.next() else {
            return false;
        };
        self.orders.queued_moves.extend(orders);
        self.begin_move_order(first);
        true
    }

    pub(crate) fn start_direct_move(&mut self, target: Vec3) {
        self.orders.clear();
        self.clear_attack_state();
        self.start_moving_to(target);
    }

    pub(crate) fn cancel_scripted_move_orders(&mut self) {
        self.orders.clear();
    }

    /// Whether the active scripted movement order may acquire enemy targets.
    #[must_use]
    pub fn is_executing_attack_move(&self) -> bool {
        self.orders
            .active_move
            .is_some_and(|order| order.attack_move)
    }

    pub(crate) fn is_auto_attack_engagement(&self) -> bool {
        self.orders.auto_attack_origin.is_some()
    }

    pub(crate) fn auto_attack_origin(&self) -> Option<Vec3> {
        self.orders.auto_attack_origin
    }

    pub(crate) fn begin_attack_move_engagement(&mut self, target: EntityId) -> bool {
        if !self.is_executing_attack_move()
            || self.state != SquadState::Moving
            || target.is_invalid()
        {
            return false;
        }
        self.orders.auto_attack_origin = Some(self.base.position);
        self.clear_attack_state();
        self.attack_target = Some(target);
        self.cancel_idle_action();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        self.state = SquadState::Attacking;
        true
    }

    /// Cancel combat, resuming a suspended attack-move or a queued move.
    pub fn clear_attack_order(&mut self) {
        self.cancel_idle_action();
        let resume_attack_move = self.orders.auto_attack_origin.take().is_some();
        self.clear_attack_state();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if resume_attack_move && self.resume_active_move() {
            return;
        }
        if self.orders.active_move.is_none() && self.begin_next_queued_move() {
            return;
        }
        if self.state == SquadState::Attacking {
            self.state = SquadState::Idle;
        }
    }

    pub(crate) fn finish_current_movement(&mut self) {
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if self.state == SquadState::Attacking {
            return;
        }
        self.orders.active_move = None;
        self.orders.auto_attack_origin = None;
        if !self.begin_next_queued_move() && self.state == SquadState::Moving {
            self.state = SquadState::Idle;
        }
        self.cancel_idle_action();
    }

    pub(crate) fn hash_order_state(&self, checksum: &mut SyncChecksum) {
        self.orders.hash_state(checksum);
    }

    fn begin_move_order(&mut self, order: SquadMoveOrder) {
        self.orders.active_move = Some(order);
        self.orders.auto_attack_origin = None;
        self.clear_attack_state();
        self.start_moving_to(order.target);
    }

    pub(super) fn begin_next_queued_move(&mut self) -> bool {
        let Some(order) = self.orders.queued_moves.pop_front() else {
            return false;
        };
        self.begin_move_order(order);
        true
    }

    fn resume_active_move(&mut self) -> bool {
        let Some(order) = self.orders.active_move else {
            return self.begin_next_queued_move();
        };
        self.start_moving_to(order.target);
        true
    }

    fn start_moving_to(&mut self, target: Vec3) {
        self.mines.cancel();
        self.detonate.cancel();
        self.cancel_idle_action();
        self.move_target = Some(target);
        self.state = SquadState::Moving;
    }

    fn clear_attack_state(&mut self) {
        self.attack_target = None;
        self.clear_experience_bank();
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.ability_used_unit_ids.clear();
    }
}

#[cfg(test)]
mod tests;
