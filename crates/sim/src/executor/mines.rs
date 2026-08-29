//! Translation of wire Mines commands into authoritative squad work state.

use super::CommandExecutor;
use crate::commands::WorkCommand;
use crate::world::World;

impl CommandExecutor<'_> {
    pub(super) fn execute_mines(&self, world: &mut World, command: &WorkCommand) {
        let (Some(database), Ok(player_id), Ok(ability_id)) = (
            self.database,
            u8::try_from(command.base.player_id),
            u8::try_from(command.ability_id),
        ) else {
            return;
        };
        if database.abilities.get(usize::from(ability_id)).is_none() {
            return;
        }
        let target_entity = (!command.unit_id.is_invalid()).then_some(command.unit_id);
        if target_entity.is_none() && command.terrain_point.is_none() {
            return;
        }
        let explicit_range = if command.range == 0.0 {
            None
        } else if command.range.is_finite() {
            Some(command.range)
        } else {
            return;
        };
        for &squad_id in &command.base.recipients {
            let _accepted = world.issue_mines_order(
                player_id,
                squad_id,
                target_entity,
                command.terrain_point,
                explicit_range,
                ability_id,
            );
        }
    }
}
