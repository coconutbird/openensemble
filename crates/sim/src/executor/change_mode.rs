//! Work and direct-building command routing for squad-mode transitions.

use super::CommandExecutor;
use crate::commands::{BuildingCommand, WorkCommand};
use crate::entities::SquadMode;
use crate::player::PlayerId;
use crate::world::World;

impl CommandExecutor<'_> {
    pub(super) fn execute_change_mode(&self, world: &mut World, command: &WorkCommand) {
        let (Some(gameplay), Ok(player_id), Ok(ability_id)) = (
            self.gameplay,
            u8::try_from(command.base.player_id),
            self.work_ability_id(command.ability_id),
        ) else {
            return;
        };
        let requested_mode = if command.squad_mode == -1 {
            None
        } else {
            let Some(mode) = SquadMode::from_i32(command.squad_mode) else {
                return;
            };
            Some(mode)
        };
        for &squad_id in &command.base.recipients {
            let _accepted = world.issue_change_mode_order(
                player_id,
                squad_id,
                requested_mode,
                ability_id,
                gameplay,
            );
        }
    }

    pub(super) fn execute_building_change_mode(
        &self,
        world: &mut World,
        command: &BuildingCommand,
        player_id: PlayerId,
    ) {
        let (Some(gameplay), Some(mode)) = (self.gameplay, SquadMode::from_i32(command.target_id))
        else {
            return;
        };
        for &unit_id in &command.base.recipients {
            let Some(squad_id) = world.get_unit(unit_id).and_then(|unit| unit.squad_id) else {
                continue;
            };
            let _accepted =
                world.issue_change_mode_order(player_id, squad_id, Some(mode), None, gameplay);
        }
    }
}
