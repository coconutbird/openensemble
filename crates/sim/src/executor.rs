//! Command executor - processes commands and applies them to the world.
//!
//! This is where commands from the network get wired up to actual game logic.

use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{
    BuildingCommand, BuildingCommandType, GameCommand, GameCommandType, PowerCommand, WorkCommand,
};
use crate::entities::{RecoveryType, SquadMode};
use crate::gameplay::resolve_database_ability;
use crate::order::OrderType;
use crate::spawn::{MAX_SPAWN_BATCH, spawn_object_at, spawn_squads_at};
use crate::world::World;
use pipeline::database::hw1::Database;

#[cfg(test)]
use glam::Vec3;

/// Command executor that processes commands against the world.
#[derive(Debug, Default)]
pub struct CommandExecutor<'database> {
    database: Option<&'database Database>,
}

impl<'database> CommandExecutor<'database> {
    /// Create a new command executor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an executor that can resolve database-backed create commands.
    #[must_use]
    pub const fn with_database(database: &'database Database) -> Self {
        Self {
            database: Some(database),
        }
    }

    /// Execute a list of commands against the world.
    pub fn execute_all(&self, world: &mut World, commands: &[CommandEntry]) {
        for entry in commands {
            self.execute(world, entry);
        }
    }

    /// Execute a single command against the world.
    pub fn execute(&self, world: &mut World, entry: &CommandEntry) {
        match &entry.command {
            QueuedCommand::Work(cmd) => self.execute_work(world, cmd),
            QueuedCommand::Power(cmd) => Self::execute_power(world, cmd),
            QueuedCommand::Building(cmd) => self.execute_building(world, cmd),
            QueuedCommand::Game(cmd) => self.execute_game(world, cmd),
        }
    }

    /// Execute a work command.
    fn execute_work(&self, world: &mut World, cmd: &WorkCommand) {
        let order_type = OrderType::from_i32(cmd.base.id);

        match order_type {
            Some(OrderType::Move) => Self::execute_move(world, cmd),
            Some(OrderType::Attack) => self.execute_attack(world, cmd),
            _ => {}
        }
    }

    /// Execute a move order.
    fn execute_move(world: &mut World, cmd: &WorkCommand) {
        if let Some(target) = cmd.terrain_point {
            Self::move_owned_recipients(world, cmd, target);
        }
    }

    /// Execute an attack order.
    fn execute_attack(&self, world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        if cmd.unit_id.is_invalid() {
            return;
        }
        let squad_mode = if cmd.squad_mode == -1 {
            None
        } else {
            let Some(mode) = SquadMode::from_i32(cmd.squad_mode) else {
                return;
            };
            Some(mode)
        };
        let ability_id = if cmd.ability_id == -1 {
            None
        } else {
            let Ok(id) = u8::try_from(cmd.ability_id) else {
                return;
            };
            if self
                .database
                .is_some_and(|database| usize::from(id) >= database.abilities.len())
            {
                return;
            }
            Some(id)
        };
        for &recipient_id in &cmd.base.recipients {
            if ability_id.is_some_and(|ability_id| {
                self.ability_order_is_recovering(world, recipient_id, ability_id)
            }) {
                continue;
            }
            let _accepted = world.issue_attack_order_with_context(
                player_id,
                recipient_id,
                cmd.unit_id,
                cmd.range,
                squad_mode,
                ability_id,
            );
        }
    }

    fn ability_order_is_recovering(
        &self,
        world: &World,
        recipient_id: crate::EntityId,
        requested_id: u8,
    ) -> bool {
        let (Some(database), Some(squad)) = (self.database, world.get_squad(recipient_id)) else {
            return false;
        };
        let Some(proto_object_name) = squad
            .unit_ids
            .iter()
            .find_map(|unit_id| world.get_unit(*unit_id))
            .map(|unit| unit.proto_object_name.as_str())
        else {
            return false;
        };
        let Some((_, ability)) =
            resolve_database_ability(database, proto_object_name, requested_id)
        else {
            return false;
        };
        ability
            .recover_type
            .as_deref()
            .and_then(RecoveryType::from_authored)
            .is_some_and(|recovery_type| squad.recovery.blocks(recovery_type))
    }

    fn move_owned_recipients(world: &mut World, cmd: &WorkCommand, target: glam::Vec3) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        for &recipient_id in &cmd.base.recipients {
            if world
                .get_squad(recipient_id)
                .is_some_and(|squad| squad.base.player_id == player_id)
            {
                if let Some(squad) = world.get_squad_mut(recipient_id) {
                    squad.move_to(target);
                }
            } else if world
                .get_unit(recipient_id)
                .is_some_and(|unit| unit.base.player_id == player_id)
                && let Some(unit) = world.get_unit_mut(recipient_id)
            {
                unit.move_to(target);
            }
        }
    }

    /// Execute a power command.
    fn execute_power(_world: &mut World, _cmd: &PowerCommand) {
        // TODO: Implement power commands
    }

    /// Execute supported building production commands.
    fn execute_building(&self, world: &mut World, cmd: &BuildingCommand) {
        let (Some(database), Ok(player_id)) = (self.database, u8::try_from(cmd.base.player_id))
        else {
            return;
        };
        if cmd.building_type != BuildingCommandType::Research || cmd.count == 0 {
            return;
        }
        for &building_id in &cmd.base.recipients {
            if cmd.count > 0 {
                let _result = world.queue_research(player_id, building_id, database, cmd.target_id);
            } else {
                let _result =
                    world.cancel_research(player_id, building_id, database, cmd.target_id);
            }
        }
    }

    /// Execute a game command.
    fn execute_game(&self, world: &mut World, cmd: &GameCommand) {
        let Some(database) = self.database else {
            return;
        };
        match cmd.game_type {
            GameCommandType::CreateSquad => Self::execute_create_squad(world, database, cmd),
            GameCommandType::CreateObject => Self::execute_create_object(world, database, cmd),
            _ => {}
        }
    }

    fn execute_create_squad(world: &mut World, database: &Database, cmd: &GameCommand) {
        let (Ok(player_id), Ok(count)) =
            (u8::try_from(cmd.base.player_id), u32::try_from(cmd.data2))
        else {
            return;
        };
        let _spawned = spawn_squads_at(
            world,
            database,
            player_id,
            cmd.data,
            count,
            cmd.position,
            glam::Vec3::Z,
        );
    }

    fn execute_create_object(world: &mut World, database: &Database, cmd: &GameCommand) {
        let (Ok(player_id), Ok(count)) =
            (u8::try_from(cmd.base.player_id), u32::try_from(cmd.data2))
        else {
            return;
        };
        if count > MAX_SPAWN_BATCH {
            return;
        }
        for _ in 0..count {
            if spawn_object_at(
                world,
                database,
                player_id,
                cmd.data,
                cmd.position,
                glam::Vec3::Z,
            )
            .is_err()
            {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::command_queue::QueuedCommand;
    use crate::entities::SquadState;
    use crate::order::OrderType;

    #[test]
    fn test_move_command_sets_squad_target() {
        let mut world = World::new();
        world.init_players(1);

        // Create a squad at origin
        let squad_id = world.create_squad(1);

        // Create a move command
        let work_cmd = WorkCommand {
            base: Command {
                id: OrderType::Move as i32,
                player_id: 1,
                recipients: vec![squad_id],
                ..Default::default()
            },
            terrain_point: Some(Vec3::new(100.0, 0.0, 50.0)),
            ..Default::default()
        };

        let entry = CommandEntry {
            command: QueuedCommand::Work(work_cmd),
            exec_time: 50,
            sequence: 0,
            source_client: 1,
        };

        // Execute the command
        let executor = CommandExecutor::new();
        executor.execute(&mut world, &entry);

        // Verify squad is now moving to target
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.state, SquadState::Moving);
        assert_eq!(squad.move_target, Some(Vec3::new(100.0, 0.0, 50.0)));
    }

    #[test]
    fn test_squad_moves_toward_target() {
        let mut world = World::new();
        world.init_players(1);

        // Create a squad at origin
        let squad_id = world.create_squad_at(1, Vec3::ZERO);

        // Issue move command to (100, 0, 0)
        {
            let squad = world.get_squad_mut(squad_id).unwrap();
            squad.move_to(Vec3::new(100.0, 0.0, 0.0));
        }

        // Simulate one tick (50ms = 0.05s)
        world.update_entities(0.05);

        // Squad should have moved toward target
        let squad = world.get_squad(squad_id).unwrap();
        assert!(squad.position().x > 0.0);
        assert!(squad.position().x < 100.0);
        assert_eq!(squad.state, SquadState::Moving);
    }

    #[test]
    fn test_move_command_moves_standalone_unit() {
        let mut world = World::new();
        let unit_id = world.create_unit(1);
        let target = Vec3::new(25.0, 0.0, 10.0);
        let work_cmd = WorkCommand {
            base: Command {
                id: OrderType::Move as i32,
                player_id: 1,
                recipients: vec![unit_id],
                ..Default::default()
            },
            terrain_point: Some(target),
            ..Default::default()
        };
        let entry = CommandEntry {
            command: QueuedCommand::Work(work_cmd),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::new().execute(&mut world, &entry);

        assert_eq!(world.get_unit(unit_id).unwrap().move_target, Some(target));
    }

    #[test]
    fn attack_command_preserves_mode_and_ability_context() {
        let mut world = World::new();
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        let attacker = world.create_squad(1);
        let target = world.create_unit(2);
        let work_cmd = WorkCommand {
            base: Command {
                id: OrderType::Attack as i32,
                player_id: 1,
                recipients: vec![attacker],
                ..Default::default()
            },
            unit_id: target,
            squad_mode: SquadMode::Cover as i32,
            ability_id: 3,
            ..Default::default()
        };
        let entry = CommandEntry {
            command: QueuedCommand::Work(work_cmd),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::new().execute(&mut world, &entry);

        let squad = world.get_squad(attacker).unwrap();
        assert_eq!(squad.mode, SquadMode::Cover);
        assert_eq!(squad.attack_ability_id, Some(3));
        assert_eq!(squad.attack_target, Some(target));
    }
}
