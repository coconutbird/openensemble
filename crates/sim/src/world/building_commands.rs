//! Building commands `UnloadUnits`, Kill, `CancelKill`, `DestroyBase`, `CancelDestroyBase`,
//! `TrainLock` and `TrainUnlock` (`BBuildingCommand_processUnit` @ `0x1401987D0` cases 4, 8, 9,
//! 14, 15, 18 and 19).
//!
//! - `UnloadUnits`: `BSquad_queueUnload` @ `0x14039AD80` on the parent squad of the recipient,
//!   with user priority, every garrisoned squad, no rally point, and only the squads of the
//!   command player.
//! - Kill and `DestroyBase`: event 73 (`CommandRecycle`) and `BUnit_doSelfDestruct(false)`
//!   @ `0x1404D0640` when the recipient prototype has a Kill or `DestroyBase` command (or no
//!   prototype). `BUnitActionBuilding_doSelfDestruct` @ `0x1404F9780` clears the queue of a
//!   built building and enters state 9 (Fading) with destruct time 0. On its next update
//!   (`BUnitActionBuilding_update`) the building refunds its cost (`getCost` with count -1)
//!   to the creating player, times `RecyleRefundRate` when built, times hitpoints over
//!   maximum hitpoints when damaged, and dies.
//! - `CancelKill` and `CancelDestroyBase`: event 74 (`CommandRecycleCancel`) and
//!   `BUnit_doSelfDestruct(true)`: a Fading building goes back to its work.
//! - `TrainLock` and `TrainUnlock`: `BUnit_setTrainLock` @ `0x1404D12E0` adds or removes the
//!   `TrainLock` entity reference and changes the parent squad to Lockdown or Normal mode.
//!   `BUnit_getTrainLock` @ `0x1404D1520` then keeps trained squads inside the building.
//!
//! Not modelled: the construction-paused branch of `doSelfDestruct` (the sim has no paused
//! construction), the creating player of a finished building (the owner gets the refund),
//! the `unit+193` bit that blocks `TrainUnlock`, and the team score of a recycled building
//! (`0x140387430`).

use super::{GeneralEvent, GeneralEventType, World};
use crate::entities::SquadMode;
use crate::entities::units::ProductionTask;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, Resources};
use crate::prototypes::object_by_name_trim_query as object_by_name;
use pipeline::database::hw1::Database;

impl World {
    /// Case 4 (UnloadUnits): unload the command player's squads from the parent squad of
    /// `unit_id`. Returns the count of squads that start to unload.
    pub fn unload_building_squads(&mut self, player_id: PlayerId, unit_id: EntityId) -> usize {
        let Some(container_squad_id) = self.get_unit(unit_id).and_then(|unit| unit.squad_id) else {
            return 0;
        };
        let passengers = self
            .squads
            .get(container_squad_id)
            .map_or_else(Vec::new, |squad| {
                squad.garrison.contained_squad_ids().to_vec()
            });
        let mut unloaded = 0;
        for passenger_id in passengers {
            let owned = self
                .squads
                .get(passenger_id)
                .is_some_and(|squad| squad.base.player_id == player_id);
            if owned
                && self
                    .issue_ungarrison_order(player_id, passenger_id, None)
                    .is_ok()
            {
                // An unloaded trained squad leaves the birth queue of its trainer.
                if let Some(building) = self.get_building_mut(unit_id) {
                    let _removed = building.production.remove_trained_squad(passenger_id);
                }
                unloaded += 1;
            }
        }
        unloaded
    }

    /// Cases 8 and 18 (Kill, `DestroyBase`): recycle the building on its next production
    /// update. Returns `false` when the prototype has no Kill or `DestroyBase` command, the
    /// recipient is not a building, or it already recycles.
    pub fn start_building_self_destruct(
        &mut self,
        player_id: PlayerId,
        unit_id: EntityId,
        database: Option<&Database>,
    ) -> bool {
        let Some(unit) = self.get_unit(unit_id) else {
            return false;
        };
        let has_command = database
            .and_then(|database| object_by_name(database, &unit.proto_object_name))
            .is_none_or(|prototype| {
                prototype.commands.iter().any(|command| {
                    command.command_type.as_deref().is_some_and(|kind| {
                        kind.eq_ignore_ascii_case("Kill")
                            || kind.eq_ignore_ascii_case("DestroyBase")
                    })
                })
            });
        if !has_command {
            return false;
        }
        let _fired = self.fire_general_event(
            &GeneralEvent::new(GeneralEventType::CommandRecycle, i32::from(player_id))
                .with_entities(Some(unit_id), None),
        );
        let Some((built, fading)) = self
            .get_building(unit_id)
            .map(|building| (building.built, building.production.self_destructing))
        else {
            return false;
        };
        if fading {
            return false;
        }
        if built {
            self.clear_building_production_queue(unit_id);
        }
        if let Some(building) = self.get_building_mut(unit_id) {
            building.production.self_destructing = true;
        }
        true
    }

    /// Cases 9 and 19 (`CancelKill`, `CancelDestroyBase`): stop a pending recycle. Returns
    /// whether the building recycled.
    pub fn cancel_building_self_destruct(
        &mut self,
        player_id: PlayerId,
        unit_id: EntityId,
    ) -> bool {
        let _fired = self.fire_general_event(
            &GeneralEvent::new(GeneralEventType::CommandRecycleCancel, i32::from(player_id))
                .with_entities(Some(unit_id), None),
        );
        self.get_building_mut(unit_id).is_some_and(|building| {
            std::mem::replace(&mut building.production.self_destructing, false)
        })
    }

    /// The Fading update: refund the recycle value and kill the building.
    pub(super) fn recycle_building(&mut self, unit_id: EntityId, database: &Database) {
        let Some(building) = self.get_building_mut(unit_id) else {
            return;
        };
        building.production.self_destructing = false;
        let built = building.built;
        let hitpoints = building.hitpoints;
        let maximum_hitpoints = building.max_hitpoints;
        let prototype_name = building.proto_object_name.clone();
        let creator_id = match building.production.current_item.as_ref() {
            Some(ProductionTask::Construction(task)) if !built => task.purchasing_player_id,
            _ => building.base.player_id,
        };
        let cost = object_by_name(database, &prototype_name).and_then(|prototype| {
            self.object_cost_for_prototype(database, creator_id, prototype, -1)
                .ok()
        });
        if let Some(mut cost) = cost {
            if built {
                let rate = database
                    .game_data
                    .as_ref()
                    .and_then(|game_data| game_data.recyle_refund_rate)
                    .unwrap_or(1.0);
                scale(&mut cost, rate);
            }
            if maximum_hitpoints > hitpoints {
                scale(&mut cost, hitpoints / maximum_hitpoints);
            }
            self.refund_cost(creator_id, &cost);
        }
        let _killed = self.kill_unit(unit_id, false);
    }

    /// Cases 14 and 15 (`TrainLock`, `TrainUnlock`): `BUnit_setTrainLock`. Returns whether
    /// the lock changed.
    pub fn set_building_train_lock(
        &mut self,
        player_id: PlayerId,
        unit_id: EntityId,
        locked: bool,
    ) -> bool {
        let Some(unit) = self.get_unit(unit_id) else {
            return false;
        };
        let squad = unit.squad_id.and_then(|squad_id| self.squads.get(squad_id));
        if squad.is_some_and(crate::entities::Squad::is_cryo_frozen) {
            return false;
        }
        if let Some(parking_lot_id) = unit
            .associated_parking_lot()
            .filter(|parking_lot_id| *parking_lot_id != unit_id)
        {
            return self.set_building_train_lock(player_id, parking_lot_id, locked);
        }
        if squad.is_some_and(crate::entities::Squad::is_changing_mode) {
            return false;
        }
        let squad_id = unit.squad_id;
        let Some(building) = self.get_building_mut(unit_id) else {
            return false;
        };
        let was_locked = building.production.train_lock_player.is_some();
        if locked == was_locked {
            return false;
        }
        building.production.train_lock_player = locked.then_some(player_id);
        if let Some(squad_id) = squad_id {
            let mode = if locked {
                SquadMode::Lockdown
            } else {
                SquadMode::Normal
            };
            let _started = self.start_explicit_change_mode(squad_id, mode);
        }
        true
    }

    /// `BUnit_getTrainLock`: whether trained squads stay inside the building.
    #[must_use]
    pub fn building_train_locked(&self, unit_id: EntityId) -> bool {
        let Some(unit) = self.get_unit(unit_id) else {
            return false;
        };
        let squad_mode = unit
            .squad_id
            .and_then(|squad_id| self.squads.get(squad_id))
            .map(|squad| squad.mode);
        if squad_mode == Some(SquadMode::Lockdown) && unit.is_building() {
            return true;
        }
        if let Some(parking_lot_id) = unit
            .associated_parking_lot()
            .filter(|parking_lot_id| *parking_lot_id != unit_id)
        {
            return self.building_train_locked(parking_lot_id);
        }
        squad_mode.is_none_or(|mode| mode == SquadMode::Lockdown)
            && unit.production.train_lock_player.is_some()
    }
}

/// `BCost` scaling (`0x1401D6B60`).
fn scale(cost: &mut Resources, factor: f32) {
    for amount in &mut cost.amounts {
        *amount *= factor;
    }
}
