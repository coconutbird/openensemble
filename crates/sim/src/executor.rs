//! Command executor - processes commands and applies them to the world.
//!
//! This is where commands from the network get wired up to actual game logic.

use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{GameCommand, PowerCommand, WorkCommand};
use crate::order::OrderType;
use crate::world::World;

#[cfg(test)]
use glam::Vec3;

/// Command executor that processes commands against the world.
#[derive(Debug, Default)]
pub struct CommandExecutor;

impl CommandExecutor {
    /// Create a new command executor.
    pub fn new() -> Self {
        Self
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
            QueuedCommand::Power(cmd) => self.execute_power(world, cmd),
            QueuedCommand::Game(cmd) => self.execute_game(world, cmd),
        }
    }

    /// Execute a work command.
    fn execute_work(&self, world: &mut World, cmd: &WorkCommand) {
        let order_type = OrderType::from_i32(cmd.base.id);

        match order_type {
            Some(OrderType::Move) => self.execute_move(world, cmd),
            Some(OrderType::Attack) => self.execute_attack(world, cmd),
            Some(OrderType::None) | None => {
                // No-op or unknown order type
            }
            Some(_) => {
                // Other order types - not yet implemented
                // TODO: Implement remaining order types as needed
            }
        }
    }

    /// Execute a move order.
    fn execute_move(&self, world: &mut World, cmd: &WorkCommand) {
        if let Some(target) = cmd.terrain_point {
            // Apply move order to all recipients
            for &recipient_id in &cmd.base.recipients {
                if let Some(squad) = world.get_squad_mut(recipient_id) {
                    squad.move_to(target);
                }
            }
        }
    }

    /// Execute an attack order.
    fn execute_attack(&self, world: &mut World, cmd: &WorkCommand) {
        // For now, attack just moves to position (combat not implemented)
        if let Some(target) = cmd.terrain_point {
            for &recipient_id in &cmd.base.recipients {
                if let Some(squad) = world.get_squad_mut(recipient_id) {
                    squad.move_to(target);
                }
            }
        }
    }

    /// Execute a power command.
    fn execute_power(&self, _world: &mut World, _cmd: &PowerCommand) {
        // TODO: Implement power commands
    }

    /// Execute a game command.
    fn execute_game(&self, _world: &mut World, _cmd: &GameCommand) {
        // TODO: Implement game commands (resign, flare, etc.)
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
        let mut work_cmd = WorkCommand::default();
        work_cmd.base = Command {
            id: OrderType::Move as i32,
            player_id: 1,
            recipients: vec![squad_id],
            ..Default::default()
        };
        work_cmd.terrain_point = Some(Vec3::new(100.0, 0.0, 50.0));

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
}
