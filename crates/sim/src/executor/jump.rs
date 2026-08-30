//! Work-command routing for voluntary squad Jump orders.

use super::CommandExecutor;
use crate::commands::WorkCommand;
use crate::order::{JumpOrderRequest, JumpOrderType};
use crate::world::World;
use glam::Vec3;

impl CommandExecutor<'_> {
    pub(super) fn execute_jump(
        &self,
        world: &mut World,
        command: &WorkCommand,
        kind: JumpOrderType,
    ) {
        let (Some(gameplay), Ok(player_id), Ok(ability_id)) = (
            self.gameplay,
            u8::try_from(command.base.player_id),
            self.work_ability_id(command.ability_id),
        ) else {
            return;
        };
        let target_id = (!command.unit_id.is_invalid()).then_some(command.unit_id);
        let target_position = command.terrain_point.unwrap_or(Vec3::ZERO);
        let request = target_id.map_or_else(
            || JumpOrderRequest::location(kind, target_position, ability_id),
            |target_id| JumpOrderRequest::entity(kind, target_id, ability_id),
        );
        for &squad_id in &command.base.recipients {
            let _accepted = world.issue_jump_order(player_id, squad_id, request, gameplay);
        }
    }
}
