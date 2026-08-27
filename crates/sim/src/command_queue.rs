//! Command queue for buffering commands until execution time.
//!
//! Commands are received from the network with a target execution time.
//! The queue holds them until the simulation reaches that time.

use crate::commands::{GameCommand, PowerCommand, WorkCommand};
use std::collections::BTreeMap;

/// A command with its execution time.
#[derive(Debug, Clone)]
pub enum QueuedCommand {
    Work(WorkCommand),
    Power(PowerCommand),
    Game(GameCommand),
}

impl QueuedCommand {
    /// Get the player ID from the base command.
    #[must_use]
    pub fn player_id(&self) -> i32 {
        match self {
            QueuedCommand::Work(cmd) => cmd.base.player_id,
            QueuedCommand::Power(cmd) => cmd.base.player_id,
            QueuedCommand::Game(cmd) => cmd.base.player_id,
        }
    }
}

/// Entry in the command queue.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    /// The command to execute.
    pub command: QueuedCommand,
    /// Execution time in game milliseconds.
    pub exec_time: u32,
    /// Sequence number for ordering commands at the same time.
    pub sequence: u64,
    /// Source client ID.
    pub source_client: u64,
}

/// Command queue with time-ordered execution.
///
/// Uses `BTreeMap` for deterministic ordering by (`exec_time`, sequence).
#[derive(Debug, Default)]
pub struct CommandQueue {
    /// Commands ordered by (`exec_time`, sequence).
    queue: BTreeMap<(u32, u64), CommandEntry>,
    /// Next sequence number.
    next_sequence: u64,
    /// Current game time.
    current_time: u32,
}

impl CommandQueue {
    /// Create a new command queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a command for execution at the specified time.
    pub fn enqueue(&mut self, command: QueuedCommand, exec_time: u32, source_client: u64) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let entry = CommandEntry {
            command,
            exec_time,
            sequence,
            source_client,
        };

        self.queue.insert((exec_time, sequence), entry);
    }

    /// Enqueue a work command.
    pub fn enqueue_work(&mut self, cmd: WorkCommand, exec_time: u32, source_client: u64) {
        self.enqueue(QueuedCommand::Work(cmd), exec_time, source_client);
    }

    /// Enqueue a power command.
    pub fn enqueue_power(&mut self, cmd: PowerCommand, exec_time: u32, source_client: u64) {
        self.enqueue(QueuedCommand::Power(cmd), exec_time, source_client);
    }

    /// Enqueue a game command.
    pub fn enqueue_game(&mut self, cmd: GameCommand, exec_time: u32, source_client: u64) {
        self.enqueue(QueuedCommand::Game(cmd), exec_time, source_client);
    }

    /// Get all commands ready to execute at or before the given time.
    pub fn drain_ready(&mut self, up_to_time: u32) -> Vec<CommandEntry> {
        let mut ready = Vec::new();

        // Collect keys to remove
        let keys_to_remove: Vec<_> = self
            .queue
            .range(..(up_to_time + 1, u64::MAX))
            .map(|(k, _)| *k)
            .collect();

        // Remove and collect entries
        for key in keys_to_remove {
            if let Some(entry) = self.queue.remove(&key) {
                ready.push(entry);
            }
        }

        ready
    }

    /// Peek at the next command without removing it.
    #[must_use]
    pub fn peek(&self) -> Option<&CommandEntry> {
        self.queue.values().next()
    }

    /// Get the execution time of the next command.
    #[must_use]
    pub fn next_exec_time(&self) -> Option<u32> {
        self.queue.keys().next().map(|(time, _)| *time)
    }

    /// Get the number of queued commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Clear all queued commands.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Set the current game time.
    pub fn set_time(&mut self, time: u32) {
        self.current_time = time;
    }

    /// Get the current game time.
    #[must_use]
    pub fn current_time(&self) -> u32 {
        self.current_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_ordering() {
        let mut queue = CommandQueue::new();

        // Add commands out of order
        queue.enqueue_game(GameCommand::default(), 300, 1);
        queue.enqueue_game(GameCommand::default(), 100, 1);
        queue.enqueue_game(GameCommand::default(), 200, 1);

        // Drain should return in time order
        let ready = queue.drain_ready(300);
        assert_eq!(ready.len(), 3);
        assert_eq!(ready[0].exec_time, 100);
        assert_eq!(ready[1].exec_time, 200);
        assert_eq!(ready[2].exec_time, 300);
    }
}
