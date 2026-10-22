//! Move and attack routing that retains squad-mode ability context.

use super::{CommandExecutor, InvalidWorkAbility};
use crate::commands::{WorkCommand, work_command_flags};
use crate::entities::SquadMode;
use crate::world::{SquadAttackContext, SquadMoveContext, World};

/// Command-wide inputs of the move and attack orders of one Work command.
#[derive(Debug, Clone, Copy)]
pub(super) struct WorkOrderContext {
    pub(super) player_id: u8,
    squad_mode: Option<SquadMode>,
    ability_id: Option<u8>,
    attack_move: bool,
    queue: bool,
}

impl CommandExecutor<'_> {
    /// Execute a move order.
    pub(super) fn execute_move(&self, world: &mut World, command: &WorkCommand) {
        let entity_position = (!command.unit_id.is_invalid())
            .then(|| world.squad_move_entity_target(command.unit_id))
            .flatten()
            .map(|(_, position)| position);
        let Some(target) = entity_position.or(command.terrain_point) else {
            return;
        };
        let Some(context) = self.work_order_context(command) else {
            return;
        };
        for &recipient_id in &command.base.recipients {
            self.move_recipient(world, context, recipient_id, target);
        }
    }

    /// Execute an attack order.
    pub(super) fn execute_attack(&self, world: &mut World, command: &WorkCommand) {
        let Some(context) = self.work_order_context(command) else {
            return;
        };
        if command.unit_id.is_invalid() {
            return;
        }
        for &recipient_id in &command.base.recipients {
            self.attack_recipient(world, command, context, recipient_id);
        }
    }

    /// Read the player, squad mode, ability and flags of a Work command.
    ///
    /// Returns `None` when the player, squad mode or ability is not valid.
    pub(super) fn work_order_context(&self, command: &WorkCommand) -> Option<WorkOrderContext> {
        let (Ok(player_id), Ok(squad_mode), Ok(ability_id)) = (
            u8::try_from(command.base.player_id),
            command_squad_mode(command),
            self.work_ability_id(command.ability_id),
        ) else {
            return None;
        };
        Some(WorkOrderContext {
            player_id,
            squad_mode,
            ability_id,
            attack_move: command.base.has_flag(work_command_flags::ATTACK_MOVE),
            queue: command.base.has_flag(crate::Command::ALTERNATE_FLAG),
        })
    }

    /// Order one recipient to attack the command target unit.
    pub(super) fn attack_recipient(
        &self,
        world: &mut World,
        command: &WorkCommand,
        context: WorkOrderContext,
        recipient_id: crate::EntityId,
    ) {
        if self.ability_is_recovering(world, recipient_id, context.ability_id) {
            return;
        }
        let squad_mode = context.squad_mode.or_else(|| {
            context
                .ability_id
                .and_then(|id| self.ability_squad_mode(world, recipient_id, id))
        });
        let _accepted = if let Some(gameplay) = self.gameplay {
            world.issue_attack_order_with_mode_automation(
                context.player_id,
                recipient_id,
                command.unit_id,
                SquadAttackContext {
                    range: command.range,
                    squad_mode,
                    ability_id: context.ability_id,
                },
                gameplay,
            )
        } else {
            world.issue_attack_order_with_context(
                context.player_id,
                recipient_id,
                command.unit_id,
                command.range,
                squad_mode,
                context.ability_id,
            )
        };
    }

    /// Order one recipient to move to a position.
    pub(super) fn move_recipient(
        &self,
        world: &mut World,
        context: WorkOrderContext,
        recipient_id: crate::EntityId,
        target: glam::Vec3,
    ) {
        if self.ability_is_recovering(world, recipient_id, context.ability_id) {
            return;
        }
        let accepted = if let Some(gameplay) = self.gameplay {
            world.issue_squad_move_order_to_position_with_context(
                context.player_id,
                recipient_id,
                target,
                SquadMoveContext {
                    attack_move: context.attack_move,
                    queue: context.queue,
                    requested_mode: context.squad_mode,
                    requested_ability_id: context.ability_id,
                },
                gameplay,
            )
        } else {
            world.issue_squad_move_order_to_position(
                context.player_id,
                recipient_id,
                target,
                context.attack_move,
                context.queue,
            )
        };
        if !accepted && !context.attack_move && !context.queue {
            let _accepted = world.issue_move_order(context.player_id, recipient_id, target);
        }
    }

    fn ability_is_recovering(
        &self,
        world: &World,
        recipient_id: crate::EntityId,
        ability_id: Option<u8>,
    ) -> bool {
        ability_id.is_some_and(|id| self.ability_order_is_recovering(world, recipient_id, id))
    }
}

fn command_squad_mode(command: &WorkCommand) -> Result<Option<SquadMode>, InvalidWorkAbility> {
    if command.squad_mode == -1 {
        Ok(None)
    } else {
        SquadMode::from_i32(command.squad_mode)
            .map(Some)
            .ok_or(InvalidWorkAbility)
    }
}
