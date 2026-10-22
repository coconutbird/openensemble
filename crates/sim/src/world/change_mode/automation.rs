//! Automatic unlock, action resumption, and relock around squad orders.

use super::World;
use crate::EntityId;
use crate::entities::{SquadChangeModeResume, SquadMode, SquadModeOrder, SquadState};
use crate::gameplay::GameplayCatalog;

impl World {
    pub(in crate::world) fn squad_move_mode_context(
        &self,
        squad_id: EntityId,
        requested_mode: Option<SquadMode>,
        requested_ability_id: Option<u8>,
        gameplay: &GameplayCatalog,
    ) -> (Option<SquadModeOrder>, bool) {
        if let Some(order) =
            self.requested_mode_order(squad_id, requested_mode, requested_ability_id, gameplay)
        {
            let permits_locked = self
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.mode == SquadMode::Lockdown);
            return (Some(order), permits_locked);
        }
        let Some((mode, _, can_unlock)) = self.squad_mode_metadata(squad_id, gameplay) else {
            return (None, false);
        };
        if mode != SquadMode::Lockdown || !can_unlock {
            return (None, false);
        }
        let can_lock = self
            .squad_mode_metadata(squad_id, gameplay)
            .is_some_and(|(_, can_lock, _)| can_lock);
        let final_mode = if can_lock {
            SquadMode::Lockdown
        } else {
            SquadMode::Normal
        };
        (Some(SquadModeOrder::new(final_mode, None)), true)
    }

    pub(in crate::world) fn requested_mode_order(
        &self,
        squad_id: EntityId,
        requested_mode: Option<SquadMode>,
        requested_ability_id: Option<u8>,
        gameplay: &GameplayCatalog,
    ) -> Option<SquadModeOrder> {
        let prototype = self.squad_leader_prototype(squad_id)?;
        let ability = requested_ability_id
            .and_then(|requested| gameplay.resolve_order_ability(prototype, requested));
        let target_mode = requested_mode
            .or_else(|| ability.and_then(crate::gameplay::AbilityGameplay::squad_mode))?;
        (target_mode == SquadMode::Lockdown).then(|| {
            SquadModeOrder::new(
                target_mode,
                ability.map(crate::gameplay::AbilityGameplay::database_id),
            )
        })
    }

    pub(in crate::world) fn update_mode_order_transitions(&mut self, gameplay: &GameplayCatalog) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| (!squad.is_changing_mode()).then_some(id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_squad_mode_order_transition(squad_id, gameplay);
        }
    }

    pub(in crate::world) fn update_squad_mode_order_transition(
        &mut self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) {
        if self.start_pending_move_relock(squad_id) || self.start_move_unlock(squad_id) {
            return;
        }
        self.start_attack_mode_transition(squad_id, gameplay);
    }

    fn start_pending_move_relock(&mut self, squad_id: EntityId) -> bool {
        let Some((order, current_mode, has_queued, player_id)) =
            self.squads.get(squad_id).and_then(|squad| {
                Some((
                    squad.pending_move_relock()?,
                    squad.mode,
                    squad.has_queued_move_orders(),
                    squad.base.player_id,
                ))
            })
        else {
            return false;
        };
        if current_mode == order.target_mode() {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.clear_pending_move_relock();
                let _started = squad.begin_next_queued_move();
            }
            return false;
        }
        let resume = if has_queued {
            SquadChangeModeResume::NextMove
        } else {
            SquadChangeModeResume::None
        };
        if !self.start_order_change_mode(player_id, squad_id, order, resume) {
            return false;
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.clear_pending_move_relock();
        }
        true
    }

    fn start_move_unlock(&mut self, squad_id: EntityId) -> bool {
        let Some((order, mode, player_id)) = self.squads.get(squad_id).and_then(|squad| {
            Some((
                squad.active_move_mode_order()?,
                squad.mode,
                squad.base.player_id,
            ))
        }) else {
            return false;
        };
        mode == SquadMode::Lockdown
            && self.start_order_change_mode(
                player_id,
                squad_id,
                SquadModeOrder::new(SquadMode::Normal, order.ability_id()),
                SquadChangeModeResume::ActiveMove,
            )
    }

    fn start_attack_mode_transition(&mut self, squad_id: EntityId, gameplay: &GameplayCatalog) {
        let Some((mode, state, player_id, order, can_lock, can_unlock)) =
            self.squads.get(squad_id).and_then(|squad| {
                let (_, can_lock, can_unlock) = self.squad_mode_metadata(squad_id, gameplay)?;
                Some((
                    squad.mode,
                    squad.state,
                    squad.base.player_id,
                    squad.attack_mode_order(),
                    can_lock,
                    can_unlock,
                ))
            })
        else {
            return;
        };
        if state != SquadState::Attacking {
            return;
        }
        let Some(in_range) = self.squad_attack_is_in_range(squad_id, gameplay) else {
            return;
        };
        let target_inside_lockdown_minimum = mode == SquadMode::Normal
            && self.squad_attack_target_inside_minimum_for_mode(
                squad_id,
                gameplay,
                SquadMode::Lockdown,
            );
        let transition = match (mode, in_range) {
            (SquadMode::Lockdown, false) if order.is_some() || can_unlock => Some((
                SquadModeOrder::new(
                    SquadMode::Normal,
                    order.and_then(SquadModeOrder::ability_id),
                ),
                SquadChangeModeResume::Attack,
            )),
            (SquadMode::Normal, true)
                if !target_inside_lockdown_minimum && (order.is_some() || can_lock) =>
            {
                Some((
                    order.unwrap_or_else(|| SquadModeOrder::new(SquadMode::Lockdown, None)),
                    SquadChangeModeResume::Attack,
                ))
            }
            _ => None,
        };
        let Some((transition, resume)) = transition else {
            return;
        };
        if order.is_none()
            && can_lock
            && can_unlock
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.set_attack_mode_order(Some(SquadModeOrder::new(SquadMode::Lockdown, None)));
        }
        let _started = self.start_order_change_mode(player_id, squad_id, transition, resume);
    }

    fn start_order_change_mode(
        &mut self,
        player_id: u8,
        squad_id: EntityId,
        order: SquadModeOrder,
        resume: SquadChangeModeResume,
    ) -> bool {
        if !self.squads.get(squad_id).is_some_and(|squad| {
            squad.base.player_id == player_id
                && squad.mode != order.target_mode()
                && self.change_mode_context_valid(squad_id)
        }) {
            return false;
        }
        self.squads.get_mut(squad_id).is_some_and(|squad| {
            squad.begin_change_mode_with_resume(order.target_mode(), order.ability_id(), resume)
        })
    }

    fn squad_mode_metadata(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<(SquadMode, bool, bool)> {
        let squad = self.squads.get(squad_id)?;
        let prototype = self.squad_leader_prototype(squad_id)?;
        Some((
            squad.mode,
            gameplay.can_auto_lock(prototype),
            gameplay.can_auto_unlock(prototype),
        ))
    }

    fn squad_leader_prototype(&self, squad_id: EntityId) -> Option<&str> {
        self.squads
            .get(squad_id)?
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .find(|unit| unit.is_operational())
            .map(|unit| unit.proto_object_name.as_str())
    }
}
