//! Work and direct-building command routing for squad-mode transitions.

use super::CommandExecutor;
use crate::EntityId;
use crate::commands::{BuildingCommand, WorkCommand};
use crate::entities::SquadMode;
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

    /// Queue the `ChangeMode` order on the squad of each gated recipient unit.
    ///
    /// `BBuildingCommand_processUnit` @ `0x1401987D0` case 6 queues the order with
    /// `BSquad_queueOrder` and does not check the squad owner again, so the order goes out
    /// as the squad owner's order.
    pub(super) fn execute_building_change_mode(
        &self,
        world: &mut World,
        command: &BuildingCommand,
        recipients: &[EntityId],
    ) {
        let (Some(gameplay), Some(mode)) = (self.gameplay, SquadMode::from_i32(command.target_id))
        else {
            return;
        };
        for &unit_id in recipients {
            let Some((squad_id, owner_id)) = world
                .get_unit(unit_id)
                .and_then(|unit| Some((unit.squad_id?, unit.base.player_id)))
            else {
                continue;
            };
            let _accepted =
                world.issue_change_mode_order(owner_id, squad_id, Some(mode), None, gameplay);
        }
    }
}
