//! Simulation loop for deterministic game updates.
//!
//! The simulation runs at a fixed tick rate and processes commands
//! at their scheduled execution times.

use crate::command_queue::{CommandEntry, CommandQueue};
use crate::executor::CommandExecutor;
use crate::random::Random;
use crate::scenario::LoadedScenario;
use crate::sync::SyncChecksum;
use crate::world::World;
use pipeline::database::hw1::Database;

/// Simulation tick rate (updates per second).
pub const TICK_RATE: u32 = 20;

/// Milliseconds per tick.
pub const MS_PER_TICK: u32 = 1_000 / TICK_RATE;

const MS_PER_TICK_F32: f32 = 50.0;
const SECONDS_PER_TICK: f32 = 0.05;

/// Simulation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimState {
    #[default]
    Stopped,
    Running,
    Paused,
}

/// Command handler trait for processing commands.
pub trait CommandHandler {
    /// Process a work command.
    fn handle_work(&mut self, cmd: &crate::commands::WorkCommand);
    /// Process a power command.
    fn handle_power(&mut self, cmd: &crate::commands::PowerCommand);
    /// Process a building command.
    fn handle_building(&mut self, cmd: &crate::commands::BuildingCommand);
    /// Process a game command.
    fn handle_game(&mut self, cmd: &crate::commands::GameCommand);
}

/// Simulation update result.
#[derive(Debug, Default)]
pub struct SimUpdateResult {
    /// Number of commands processed.
    pub commands_processed: usize,
    /// Current game time after update.
    pub game_time: u32,
    /// Current tick number.
    pub tick: u64,
    /// Sync checksum after update.
    pub checksum: u32,
}

/// Deterministic simulation loop.
#[derive(Debug)]
pub struct Simulation {
    /// Current state.
    pub state: SimState,
    /// Current game time in milliseconds.
    pub game_time_ms: u32,
    /// Current tick number.
    pub tick: u64,
    /// Command queue.
    pub command_queue: CommandQueue,
    /// Deterministic RNG.
    pub rng: Random,
    /// Sync checksum.
    pub checksum: SyncChecksum,
    /// Game speed multiplier.
    pub speed: f32,
    /// Accumulated time for sub-tick updates.
    accumulated_ms: f32,
}

impl Default for Simulation {
    fn default() -> Self {
        Self {
            state: SimState::Stopped,
            game_time_ms: 0,
            tick: 0,
            command_queue: CommandQueue::new(),
            rng: Random::new(),
            checksum: SyncChecksum::new(),
            speed: 1.0,
            accumulated_ms: 0.0,
        }
    }
}

impl Simulation {
    /// Create a new simulation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a simulation with a specific random seed.
    #[must_use]
    pub fn with_seed(seed: u64) -> Self {
        let mut sim = Self::new();
        sim.rng.set_seed64(seed);
        sim
    }

    /// Start the simulation.
    pub fn start(&mut self) {
        self.state = SimState::Running;
    }

    /// Pause the simulation.
    pub fn pause(&mut self) {
        self.state = SimState::Paused;
    }

    /// Resume the simulation.
    pub fn resume(&mut self) {
        if self.state == SimState::Paused {
            self.state = SimState::Running;
        }
    }

    /// Stop the simulation.
    pub fn stop(&mut self) {
        self.state = SimState::Stopped;
    }

    /// Reset the simulation.
    pub fn reset(&mut self) {
        self.state = SimState::Stopped;
        self.game_time_ms = 0;
        self.tick = 0;
        self.command_queue.clear();
        self.checksum.reset();
        self.accumulated_ms = 0.0;
    }

    /// Update the simulation by the given delta time (in seconds).
    ///
    /// Returns the commands that were processed this frame.
    pub fn update(&mut self, dt_seconds: f32) -> Vec<CommandEntry> {
        if self.state != SimState::Running {
            return Vec::new();
        }

        let dt_ms = dt_seconds * 1_000.0 * self.speed;
        self.accumulated_ms += dt_ms;

        let mut all_commands = Vec::new();

        // Process fixed timestep ticks
        while self.accumulated_ms >= MS_PER_TICK_F32 {
            self.accumulated_ms -= MS_PER_TICK_F32;
            let commands = self.tick_once();
            all_commands.extend(commands);
        }

        all_commands
    }

    /// Process a single simulation tick.
    pub fn tick_once(&mut self) -> Vec<CommandEntry> {
        self.game_time_ms += MS_PER_TICK;
        self.tick += 1;
        self.command_queue.set_time(self.game_time_ms);

        // Get commands ready for this tick
        let commands = self.command_queue.drain_ready(self.game_time_ms);

        // Update checksum with tick info
        self.checksum.hash_u32(self.game_time_ms);
        let wrapped_tick = u32::try_from(self.tick & u64::from(u32::MAX)).unwrap_or_default();
        self.checksum.set_update(wrapped_tick);

        commands
    }

    /// Process a single tick with world and command execution.
    ///
    /// This is the high-level API that:
    /// 1. Drains commands ready for this tick
    /// 2. Executes them against the world
    /// 3. Updates all entities
    ///
    /// Returns the commands that were processed.
    pub fn tick_with_world(&mut self, world: &mut World) -> Vec<CommandEntry> {
        // Get commands for this tick
        let commands = self.tick_once();

        // Execute commands
        let executor = CommandExecutor::new();
        executor.execute_all(world, &commands);

        // Update entities (movement, etc.)
        world.update_entities(SECONDS_PER_TICK);

        // Sync world time
        world.game_time_ms = self.game_time_ms;

        commands
    }

    /// Process one tick with world state and the active game database.
    ///
    /// This variant also executes database-backed game commands such as
    /// `CreateSquad` and `CreateObject`.
    pub fn tick_with_world_and_database(
        &mut self,
        world: &mut World,
        database: &Database,
    ) -> Vec<CommandEntry> {
        let commands = self.tick_once();
        CommandExecutor::with_database(database).execute_all(world, &commands);
        let _completed_research = world.update_research(SECONDS_PER_TICK, database);
        world.update_entities(SECONDS_PER_TICK);
        world.game_time_ms = self.game_time_ms;
        commands
    }

    /// Process one authoritative scenario tick with its gameplay catalog.
    pub fn tick_with_scenario(
        &mut self,
        scenario: &mut LoadedScenario,
        database: &Database,
    ) -> Vec<CommandEntry> {
        let commands = self.tick_once();
        CommandExecutor::with_database(database).execute_all(&mut scenario.world, &commands);
        let _completed_research = scenario.world.update_research(SECONDS_PER_TICK, database);
        scenario
            .world
            .update_entities_with_gameplay(SECONDS_PER_TICK, &scenario.gameplay);
        scenario.world.game_time_ms = self.game_time_ms;
        commands
    }

    /// Update the simulation with world integration.
    ///
    /// This is the high-level API that processes multiple ticks
    /// based on real-time delta, executing commands and updating entities.
    pub fn update_with_world(&mut self, dt_seconds: f32, world: &mut World) -> Vec<CommandEntry> {
        if self.state != SimState::Running {
            return Vec::new();
        }

        let dt_ms = dt_seconds * 1_000.0 * self.speed;
        self.accumulated_ms += dt_ms;

        let mut all_commands = Vec::new();

        // Process fixed timestep ticks
        while self.accumulated_ms >= MS_PER_TICK_F32 {
            self.accumulated_ms -= MS_PER_TICK_F32;
            let commands = self.tick_with_world(world);
            all_commands.extend(commands);
        }

        all_commands
    }

    /// Advance fixed ticks while executing commands with the active database.
    pub fn update_with_world_and_database(
        &mut self,
        dt_seconds: f32,
        world: &mut World,
        database: &Database,
    ) -> Vec<CommandEntry> {
        if self.state != SimState::Running {
            return Vec::new();
        }

        self.accumulated_ms += dt_seconds * 1_000.0 * self.speed;
        let mut all_commands = Vec::new();
        while self.accumulated_ms >= MS_PER_TICK_F32 {
            self.accumulated_ms -= MS_PER_TICK_F32;
            all_commands.extend(self.tick_with_world_and_database(world, database));
        }
        all_commands
    }

    /// Advance fixed ticks against an authoritative loaded scenario.
    pub fn update_with_scenario(
        &mut self,
        dt_seconds: f32,
        scenario: &mut LoadedScenario,
        database: &Database,
    ) -> Vec<CommandEntry> {
        if self.state != SimState::Running {
            return Vec::new();
        }

        self.accumulated_ms += dt_seconds * 1_000.0 * self.speed;
        let mut all_commands = Vec::new();
        while self.accumulated_ms >= MS_PER_TICK_F32 {
            self.accumulated_ms -= MS_PER_TICK_F32;
            all_commands.extend(self.tick_with_scenario(scenario, database));
        }
        all_commands
    }
}
