//! Retail right-click Work commands: recipient type Army and `id` -1.
//!
//! Retail runs these commands through `BCommand_processArmies`
//! (`0x1401947B0`) and `BWorkCommand_processArmy` (`0x140196300`):
//!
//! 1. Recipient filter (command source byte `+161` is 0 for every wire
//!    command): when the target unit has capture work in progress
//!    (`unit+759 & 0x40`), drop the squads that already capture it (a unit
//!    with entity reference type 15 on the target). Drop squads with
//!    `mFlagIsRepairing` (`squad+741` bit 7), the squads with a Repair work
//!    action. Keep only squads of the command player or Gaia.
//! 2. The target entity position, or else the last waypoint, must be inside
//!    the simulation bounds (`BWorld_isInsideSimBounds`, `gWorld+1192`; the
//!    maximum edges are exclusive). Otherwise no squad gets an order.
//! 3. Each platoon picks its order type with `BSquad_getDefaultOrderType`
//!    (`0x140399E30`): the tactic action for the target, else Move to the
//!    waypoint. A Move order drops the target. Flag bit 0 (`cFlagAlternate`)
//!    appends the order to the queue instead of replacing the queue.
//!
//! Armies and platoon formations are not modelled: each squad picks its own
//! action with [`default_work_action`] and gets the order directly. The squad
//! order queue holds only move waypoints, so a queued targeted action starts
//! at once.

use super::CommandExecutor;
use super::mode_orders::WorkOrderContext;
use crate::commands::WorkCommand;
use crate::world::World;
use crate::{DefaultWorkAction, EntityId, default_work_action};
use glam::Vec3;

impl CommandExecutor<'_> {
    /// Execute a retail right-click Work command.
    pub(super) fn execute_user_work(&self, world: &mut World, command: &WorkCommand) {
        let Some(context) = self.work_order_context(command) else {
            return;
        };
        let target_id = (!command.unit_id.is_invalid()).then_some(command.unit_id);
        let waypoint = command.base.waypoints.last().copied();
        let Some(bounds_position) = target_id
            .and_then(|target_id| world.squad_move_entity_target(target_id))
            .map(|(_, position)| position)
            .or(waypoint)
        else {
            return;
        };
        if !inside_simulation_bounds(world, bounds_position) {
            return;
        }
        for squad_id in user_work_recipients(world, context.player_id, command, target_id) {
            let action = target_id
                .zip(self.gameplay)
                .and_then(|(target_id, gameplay)| {
                    default_work_action(world, gameplay, squad_id, target_id)
                });
            self.issue_user_work(world, command, context, squad_id, action, waypoint);
        }
    }

    fn issue_user_work(
        &self,
        world: &mut World,
        command: &WorkCommand,
        context: WorkOrderContext,
        squad_id: EntityId,
        action: Option<DefaultWorkAction>,
        waypoint: Option<Vec3>,
    ) {
        let player_id = context.player_id;
        match action {
            None => {
                if let Some(waypoint) = waypoint {
                    self.move_recipient(world, context, squad_id, waypoint);
                }
            }
            Some(DefaultWorkAction::Attack) => {
                self.attack_recipient(world, command, context, squad_id);
            }
            Some(DefaultWorkAction::Gather) => {
                self.gather_recipient(world, command, player_id, squad_id);
            }
            Some(DefaultWorkAction::Capture) => {
                self.capture_recipient(world, command, player_id, squad_id);
            }
            Some(DefaultWorkAction::Garrison) => {
                Self::garrison_recipient(world, command, player_id, squad_id);
            }
            Some(DefaultWorkAction::Join) => {
                self.join_recipient(world, command, player_id, squad_id);
            }
            Some(DefaultWorkAction::Hitch) => {
                Self::hitch_recipient(world, command, player_id, squad_id);
            }
            Some(DefaultWorkAction::RepairOther) => {
                self.repair_other_recipient(world, command, player_id, squad_id);
            }
        }
    }
}

/// Apply the `BCommand_processArmies` recipient filter.
fn user_work_recipients(
    world: &World,
    player_id: u8,
    command: &WorkCommand,
    target_id: Option<EntityId>,
) -> Vec<EntityId> {
    let capturing_unit_ids = target_id
        .and_then(|target_id| world.get_unit(target_id))
        .filter(|target| target.capture.target.is_being_captured())
        .map_or(&[][..], |target| target.capture.target.active_unit_ids());
    let mut recipients = Vec::with_capacity(command.base.recipients.len());
    for &squad_id in &command.base.recipients {
        let Some(squad) = world.get_squad(squad_id) else {
            continue;
        };
        if !World::command_reaches_squad_owner(player_id, squad.base.player_id)
            || squad.is_self_repairing()
            || squad
                .unit_ids
                .iter()
                .any(|unit_id| capturing_unit_ids.binary_search(unit_id).is_ok())
            || recipients.contains(&squad_id)
        {
            continue;
        }
        recipients.push(squad_id);
    }
    recipients
}

/// `BWorld_isInsideSimBounds` (`0x140194F60`): minimum edges inclusive,
/// maximum edges exclusive. A world without bounds accepts every point.
fn inside_simulation_bounds(world: &World, position: Vec3) -> bool {
    world.effective_playable_bounds().is_none_or(|bounds| {
        bounds.min_x() <= position.x
            && bounds.min_z() <= position.z
            && position.x < bounds.max_x()
            && position.z < bounds.max_z()
    })
}

#[cfg(test)]
mod tests;
