//! Translation of wire Detonate commands into authoritative squad work state.

use super::CommandExecutor;
use crate::commands::WorkCommand;
use crate::world::World;

impl CommandExecutor<'_> {
    pub(super) fn execute_detonate(&self, world: &mut World, command: &WorkCommand) {
        let Ok(player_id) = u8::try_from(command.base.player_id) else {
            return;
        };
        if command.unit_id.is_invalid() {
            return;
        }
        let ability_id = if command.ability_id == -1 {
            None
        } else {
            let (Some(database), Ok(ability_id)) =
                (self.database, u8::try_from(command.ability_id))
            else {
                return;
            };
            if database.abilities.get(usize::from(ability_id)).is_none() {
                return;
            }
            Some(ability_id)
        };
        for &squad_id in &command.base.recipients {
            if ability_id.is_some_and(|ability_id| {
                self.ability_order_is_recovering(world, squad_id, ability_id)
            }) {
                continue;
            }
            let _accepted =
                world.issue_detonate_order(player_id, squad_id, command.unit_id, ability_id);
        }
    }
}
