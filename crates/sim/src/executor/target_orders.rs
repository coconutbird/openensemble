//! Work orders against one target entity: gather, capture, repair other,
//! garrison, join and hitch.

use super::CommandExecutor;
use crate::EntityId;
use crate::commands::WorkCommand;
use crate::world::World;

impl CommandExecutor<'_> {
    pub(super) fn execute_gather(&self, world: &mut World, cmd: &WorkCommand) {
        for_each_target_recipient(world, cmd, |world, player_id, recipient_id| {
            self.gather_recipient(world, cmd, player_id, recipient_id);
        });
    }

    pub(super) fn execute_capture(&self, world: &mut World, cmd: &WorkCommand) {
        for_each_target_recipient(world, cmd, |world, player_id, recipient_id| {
            self.capture_recipient(world, cmd, player_id, recipient_id);
        });
    }

    pub(super) fn execute_repair_other(&self, world: &mut World, cmd: &WorkCommand) {
        for_each_target_recipient(world, cmd, |world, player_id, recipient_id| {
            self.repair_other_recipient(world, cmd, player_id, recipient_id);
        });
    }

    pub(super) fn execute_garrison(world: &mut World, cmd: &WorkCommand) {
        for_each_target_recipient(world, cmd, |world, player_id, recipient_id| {
            Self::garrison_recipient(world, cmd, player_id, recipient_id);
        });
    }

    pub(super) fn execute_join(&self, world: &mut World, cmd: &WorkCommand) {
        for_each_target_recipient(world, cmd, |world, player_id, recipient_id| {
            self.join_recipient(world, cmd, player_id, recipient_id);
        });
    }

    pub(super) fn execute_hitch(world: &mut World, cmd: &WorkCommand) {
        for_each_target_recipient(world, cmd, |world, player_id, recipient_id| {
            Self::hitch_recipient(world, cmd, player_id, recipient_id);
        });
    }

    /// Order one recipient to gather the command target.
    pub(super) fn gather_recipient(
        &self,
        world: &mut World,
        cmd: &WorkCommand,
        player_id: u8,
        recipient_id: EntityId,
    ) {
        if let Some(gameplay) = self.gameplay {
            let _accepted =
                world.issue_gather_order(player_id, recipient_id, cmd.unit_id, gameplay);
        }
    }

    /// Order one recipient to capture the command target.
    pub(super) fn capture_recipient(
        &self,
        world: &mut World,
        cmd: &WorkCommand,
        player_id: u8,
        recipient_id: EntityId,
    ) {
        if let (Some(database), Some(gameplay)) = (self.database, self.gameplay) {
            let _accepted =
                world.issue_capture_order(player_id, recipient_id, cmd.unit_id, database, gameplay);
        }
    }

    /// Order one recipient to repair the command target.
    pub(super) fn repair_other_recipient(
        &self,
        world: &mut World,
        cmd: &WorkCommand,
        player_id: u8,
        recipient_id: EntityId,
    ) {
        let (Some(database), Some(gameplay), Ok(ability_id)) = (
            self.database,
            self.gameplay,
            self.work_ability_id(cmd.ability_id),
        ) else {
            return;
        };
        let _accepted = world.issue_repair_other_order(
            player_id,
            recipient_id,
            cmd.unit_id,
            ability_id,
            database,
            gameplay,
        );
    }

    /// Order one recipient to enter the command target.
    pub(super) fn garrison_recipient(
        world: &mut World,
        cmd: &WorkCommand,
        player_id: u8,
        recipient_id: EntityId,
    ) {
        let _result = world.issue_garrison_order(player_id, recipient_id, cmd.unit_id, cmd.range);
    }

    /// Order one recipient to join the command target.
    pub(super) fn join_recipient(
        &self,
        world: &mut World,
        cmd: &WorkCommand,
        player_id: u8,
        recipient_id: EntityId,
    ) {
        if let Ok(ability_id) = self.work_ability_id(cmd.ability_id) {
            let _accepted =
                world.issue_join_order(player_id, recipient_id, cmd.unit_id, ability_id);
        }
    }

    /// Order one recipient to hitch to the command target.
    pub(super) fn hitch_recipient(
        world: &mut World,
        cmd: &WorkCommand,
        player_id: u8,
        recipient_id: EntityId,
    ) {
        let _result = world.issue_hitch_order(player_id, recipient_id, cmd.unit_id);
    }
}

/// Call `issue` for each recipient when the command has a player and a target.
fn for_each_target_recipient(
    world: &mut World,
    cmd: &WorkCommand,
    mut issue: impl FnMut(&mut World, u8, EntityId),
) {
    let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
        return;
    };
    if cmd.unit_id.is_invalid() {
        return;
    }
    for &recipient_id in &cmd.base.recipients {
        issue(world, player_id, recipient_id);
    }
}
