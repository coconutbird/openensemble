//! Building commands: the cases of `BBuildingCommand_processUnit` @ `0x1401987D0`.
//!
//! Building commands have unit recipients, so only the units that pass the
//! `BCommand_processUnits` @ `0x140194560` gate get the command. `processUnit` has no case for
//! Reinforce (5), Ability (7), Tribute (10), Power (12), `RallyPoint` (16) and
//! `ReverseHotDrop` (20): its jump table sends them to the default case, which does nothing.
//! Retail sends these requests as other commands (game, power and Work commands).

use super::CommandExecutor;
use crate::EntityId;
use crate::commands::{BuildingCommand, BuildingCommandType};
use crate::entities::TrainingKind;
use crate::player::PlayerId;
use crate::world::World;

impl CommandExecutor<'_> {
    /// Execute one building command on each gated recipient.
    pub(super) fn execute_building(&self, world: &mut World, cmd: &BuildingCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        let recipients = command_unit_recipients(world, player_id, &cmd.base.recipients);
        match cmd.building_type {
            BuildingCommandType::ClearRallyPoint => {
                for &building_id in &recipients {
                    let _cleared = world.clear_unit_rally_point(building_id, player_id);
                }
            }
            BuildingCommandType::ChangeMode => {
                self.execute_building_change_mode(world, cmd, &recipients);
            }
            BuildingCommandType::UnloadUnits => {
                for &unit_id in &recipients {
                    let _unloaded = world.unload_building_squads(player_id, unit_id);
                }
            }
            BuildingCommandType::Kill | BuildingCommandType::DestroyBase => {
                for &unit_id in &recipients {
                    let _started =
                        world.start_building_self_destruct(player_id, unit_id, self.database);
                }
            }
            BuildingCommandType::CancelKill | BuildingCommandType::CancelDestroyBase => {
                for &unit_id in &recipients {
                    let _canceled = world.cancel_building_self_destruct(player_id, unit_id);
                }
            }
            BuildingCommandType::TrainLock | BuildingCommandType::TrainUnlock => {
                let locked = cmd.building_type == BuildingCommandType::TrainLock;
                for &unit_id in &recipients {
                    let _changed = world.set_building_train_lock(player_id, unit_id, locked);
                }
            }
            BuildingCommandType::Research
            | BuildingCommandType::TrainUnit
            | BuildingCommandType::TrainSquad
            | BuildingCommandType::Build
            | BuildingCommandType::BuildOther
            | BuildingCommandType::CustomCommand => {
                self.execute_building_production(world, cmd, player_id, &recipients);
            }
            BuildingCommandType::Undefined
            | BuildingCommandType::Reinforce
            | BuildingCommandType::Ability
            | BuildingCommandType::Tribute
            | BuildingCommandType::Power
            | BuildingCommandType::RallyPoint
            | BuildingCommandType::ReverseHotDrop => {}
        }
    }

    /// Cases 0, 1, 2, 3, 11 and 13: queue or cancel production work.
    fn execute_building_production(
        &self,
        world: &mut World,
        cmd: &BuildingCommand,
        player_id: PlayerId,
        recipients: &[EntityId],
    ) {
        if cmd.count == 0 {
            return;
        }
        for &building_id in recipients {
            if cmd.building_type == BuildingCommandType::CustomCommand {
                if cmd.count > 0 {
                    let _queued = world.queue_custom_command(player_id, building_id, cmd.target_id);
                } else {
                    let _canceled =
                        world.cancel_custom_command(player_id, building_id, cmd.target_id);
                }
                continue;
            }
            let Some(database) = self.database else {
                continue;
            };
            match cmd.building_type {
                BuildingCommandType::Research if cmd.count > 0 => {
                    let _result =
                        world.queue_research(player_id, building_id, database, cmd.target_id);
                }
                BuildingCommandType::Research => {
                    let _result =
                        world.cancel_research(player_id, building_id, database, cmd.target_id);
                }
                BuildingCommandType::TrainUnit | BuildingCommandType::TrainSquad => {
                    let kind = if cmd.building_type == BuildingCommandType::TrainUnit {
                        TrainingKind::Unit
                    } else {
                        TrainingKind::Squad
                    };
                    if cmd.count > 0 {
                        let _result = world.queue_training(
                            player_id,
                            building_id,
                            database,
                            kind,
                            cmd.target_id,
                            cmd.count.cast_unsigned(),
                        );
                    } else {
                        let _result = world.cancel_training(
                            player_id,
                            building_id,
                            database,
                            kind,
                            cmd.target_id,
                            cmd.count.unsigned_abs(),
                        );
                    }
                }
                BuildingCommandType::Build => {
                    if cmd.count > 0 {
                        let _result = world.start_build(
                            player_id,
                            building_id,
                            database,
                            cmd.target_id,
                            cmd.target_position,
                            cmd.socket_id,
                        );
                    } else {
                        let _result = world.cancel_build(player_id, building_id, cmd.target_id);
                    }
                }
                BuildingCommandType::BuildOther => {
                    if cmd.count > 0 {
                        let _result = world.queue_build_other(
                            player_id,
                            building_id,
                            database,
                            cmd.target_id,
                        );
                    } else {
                        let _result =
                            world.cancel_build_other(player_id, building_id, cmd.target_id);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Keep the recipient units that pass the `BCommand_processUnits` gate, in command order.
fn command_unit_recipients(world: &World, player_id: u8, recipients: &[EntityId]) -> Vec<EntityId> {
    recipients
        .iter()
        .copied()
        .filter(|&unit_id| {
            world
                .get_unit(unit_id)
                .is_some_and(|unit| world.command_reaches_owner(player_id, unit.base.player_id))
        })
        .collect()
}
