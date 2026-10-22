//! Move and attack routing that retains squad-mode ability context.

use super::{CommandExecutor, InvalidWorkAbility};
use crate::commands::{WorkCommand, work_command_flags};
use crate::entities::SquadMode;
use crate::world::{SquadAttackContext, SquadMoveContext, World};

impl CommandExecutor<'_> {
    /// Execute a move order.
    pub(super) fn execute_move(&self, world: &mut World, command: &WorkCommand) {
        let entity_position = (!command.unit_id.is_invalid())
            .then(|| world.squad_move_entity_target(command.unit_id))
            .flatten()
            .map(|(_, position)| position);
        if let Some(target) = entity_position.or(command.terrain_point) {
            self.move_owned_recipients(world, command, target);
        }
    }

    /// Execute an attack order.
    pub(super) fn execute_attack(&self, world: &mut World, command: &WorkCommand) {
        let (Ok(player_id), Ok(squad_mode), Ok(ability_id)) = (
            u8::try_from(command.base.player_id),
            command_squad_mode(command),
            self.work_ability_id(command.ability_id),
        ) else {
            return;
        };
        if command.unit_id.is_invalid() {
            return;
        }
        for &recipient_id in &command.base.recipients {
            if self.ability_is_recovering(world, recipient_id, ability_id) {
                continue;
            }
            let squad_mode = squad_mode.or_else(|| {
                ability_id.and_then(|id| self.ability_squad_mode(world, recipient_id, id))
            });
            let _accepted = if let Some(gameplay) = self.gameplay {
                world.issue_attack_order_with_mode_automation(
                    player_id,
                    recipient_id,
                    command.unit_id,
                    SquadAttackContext {
                        range: command.range,
                        squad_mode,
                        ability_id,
                    },
                    gameplay,
                )
            } else {
                world.issue_attack_order_with_context(
                    player_id,
                    recipient_id,
                    command.unit_id,
                    command.range,
                    squad_mode,
                    ability_id,
                )
            };
        }
    }

    fn move_owned_recipients(&self, world: &mut World, command: &WorkCommand, target: glam::Vec3) {
        let (Ok(player_id), Ok(requested_mode), Ok(ability_id)) = (
            u8::try_from(command.base.player_id),
            command_squad_mode(command),
            self.work_ability_id(command.ability_id),
        ) else {
            return;
        };
        let attack_move = command.base.has_flag(work_command_flags::ATTACK_MOVE);
        let queue = command.base.has_flag(crate::Command::ALTERNATE_FLAG);
        for &recipient_id in &command.base.recipients {
            if self.ability_is_recovering(world, recipient_id, ability_id) {
                continue;
            }
            let accepted = if let Some(gameplay) = self.gameplay {
                world.issue_squad_move_order_to_position_with_context(
                    player_id,
                    recipient_id,
                    target,
                    SquadMoveContext {
                        attack_move,
                        queue,
                        requested_mode,
                        requested_ability_id: ability_id,
                    },
                    gameplay,
                )
            } else {
                world.issue_squad_move_order_to_position(
                    player_id,
                    recipient_id,
                    target,
                    attack_move,
                    queue,
                )
            };
            if !accepted && !attack_move && !queue {
                let _accepted = world.issue_move_order(player_id, recipient_id, target);
            }
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
