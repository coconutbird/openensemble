//! Headless server for OpenEnsemble.
//!
//! Runs the game simulation without rendering, suitable for:
//! - Dedicated server hosting
//! - AI training
//! - Automated testing
//! - Replay validation

use anyhow::{Context, Result};
use data::GameDatabase;
use pipeline::hw1::scenario::ScenarioData;
use sim::{Session, SessionState, Simulation, World, load_scenario_into_world};
use std::time::{Duration, Instant};

/// Headless server configuration.
#[derive(Debug, Clone)]
pub struct HeadlessConfig {
    /// Path to the game directory (defaults to OPENENSEMBLE_GAME_DIR or cwd).
    pub game_dir: Option<String>,
    /// Map/scenario to load (e.g., "skirmish/blood_gulch").
    pub map_name: Option<String>,
    /// Random seed for deterministic simulation.
    pub random_seed: u64,
    /// Game speed multiplier.
    pub game_speed: f32,
    /// Maximum ticks to run (0 = unlimited).
    pub max_ticks: u64,
    /// Target tick rate (ticks per second).
    pub tick_rate: u32,
    /// Skip loading the game database (for testing without game files).
    pub skip_database: bool,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            game_dir: None,
            map_name: None,
            random_seed: 42,
            game_speed: 1.0,
            max_ticks: 0,
            tick_rate: sim::TICK_RATE,
            skip_database: false,
        }
    }
}

/// Headless game server.
pub struct HeadlessServer {
    config: HeadlessConfig,
    database: Option<GameDatabase>,
    simulation: Simulation,
    session: Session,
    world: World,
    running: bool,
    tick_count: u64,
}

impl HeadlessServer {
    /// Create a new headless server with the given configuration.
    pub fn new(config: HeadlessConfig) -> Result<Self> {
        log::info!("Initializing headless server...");

        // Load game database (unless skipped)
        let database = if config.skip_database {
            log::info!("Skipping game database load (--no-database)");
            None
        } else {
            log::info!("Loading game database...");
            let mut src = data::load_game_assets();
            let db = GameDatabase::load(&mut src)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("Failed to load game database from game directory")?;

            log::info!(
                "Loaded {} objects, {} civs, {} leaders",
                db.objects.len(),
                db.civs.len(),
                db.leaders.len()
            );
            Some(db)
        };

        // Create simulation with seed
        let simulation = Simulation::with_seed(config.random_seed);

        // Create session
        let mut session = Session::new();
        session.random_seed = config.random_seed;
        session.game_speed = config.game_speed;

        // Create empty world (will be populated when loading a scenario)
        let world = World::with_seed(config.random_seed);

        Ok(Self {
            config,
            database,
            simulation,
            session,
            world,
            running: false,
            tick_count: 0,
        })
    }

    /// Load a scenario from a [`ScenarioData`] instance.
    pub fn load_scenario_data(&mut self, scenario: &ScenarioData) -> Result<()> {
        let empty_db = GameDatabase::new();
        let db = self.database.as_ref().unwrap_or(&empty_db);
        let loaded = load_scenario_into_world(scenario, db);
        self.world = loaded.world;

        let player_count = scenario.players.as_ref().map_or(0, |p| p.entries.len());
        let object_count = scenario.objects.as_ref().map_or(0, |o| o.entries.len());
        log::info!(
            "Scenario loaded: {} players, {} objects",
            player_count,
            object_count
        );

        Ok(())
    }

    /// Load a scenario from XML string (for testing).
    #[doc(hidden)]
    pub fn load_scenario_xml(&mut self, xml: &str) -> Result<()> {
        let scenario =
            ScenarioData::from_xml_str(xml).context("Failed to parse scenario XML")?;
        self.load_scenario_data(&scenario)
    }

    /// Start the simulation.
    pub fn start(&mut self) {
        log::info!("Starting simulation...");
        self.simulation.start();
        self.session.state = SessionState::Playing;
        self.running = true;
    }

    /// Stop the simulation.
    pub fn stop(&mut self) {
        log::info!("Stopping simulation at tick {}", self.tick_count);
        self.simulation.stop();
        self.running = false;
    }

    /// Get the game database (if loaded).
    pub fn database(&self) -> Option<&GameDatabase> {
        self.database.as_ref()
    }

    /// Get the current world state.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Get the current tick count.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Check if the server is running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Run a single simulation tick.
    pub fn tick(&mut self) -> bool {
        if !self.running {
            return false;
        }

        // Check max ticks
        if self.config.max_ticks > 0 && self.tick_count >= self.config.max_ticks {
            log::info!("Reached max ticks ({}), stopping", self.config.max_ticks);
            self.stop();
            return false;
        }

        // Run simulation tick
        let commands = self.simulation.tick_with_world(&mut self.world);
        self.tick_count += 1;

        // Log periodic status
        if self.tick_count.is_multiple_of(100) {
            log::debug!(
                "Tick {}: game_time={}ms, commands={}",
                self.tick_count,
                self.simulation.game_time_ms,
                commands.len()
            );
        }

        true
    }

    /// Run the simulation loop until stopped or max ticks reached.
    pub fn run(&mut self) -> Result<()> {
        self.start();

        let tick_duration = Duration::from_millis(1000 / self.config.tick_rate as u64);
        let mut last_tick = Instant::now();
        let start_time = Instant::now();

        log::info!(
            "Running at {} ticks/second ({}ms per tick)",
            self.config.tick_rate,
            tick_duration.as_millis()
        );

        while self.running {
            let now = Instant::now();
            let elapsed = now - last_tick;

            if elapsed >= tick_duration {
                last_tick = now;

                if !self.tick() {
                    break;
                }
            } else {
                // Sleep for remaining time
                std::thread::sleep(tick_duration - elapsed);
            }
        }

        let total_time = start_time.elapsed();
        log::info!(
            "Simulation complete: {} ticks in {:.2}s ({:.1} ticks/sec)",
            self.tick_count,
            total_time.as_secs_f64(),
            self.tick_count as f64 / total_time.as_secs_f64()
        );

        Ok(())
    }
}

fn parse_args() -> HeadlessConfig {
    let mut config = HeadlessConfig::default();
    let args: Vec<String> = std::env::args().collect();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--seed" | "-s" => {
                if i + 1 < args.len() {
                    config.random_seed = args[i + 1].parse().unwrap_or(42);
                    i += 1;
                }
            }
            "--ticks" | "-t" => {
                if i + 1 < args.len() {
                    config.max_ticks = args[i + 1].parse().unwrap_or(0);
                    i += 1;
                }
            }
            "--speed" => {
                if i + 1 < args.len() {
                    config.game_speed = args[i + 1].parse().unwrap_or(1.0);
                    i += 1;
                }
            }
            "--no-database" => {
                config.skip_database = true;
            }
            "--help" | "-h" => {
                println!("OpenEnsemble Headless Server\n");
                println!("Usage: headless [OPTIONS]\n");
                println!("Options:");
                println!("  -s, --seed <SEED>    Random seed (default: 42)");
                println!("  -t, --ticks <COUNT>  Max ticks to run (default: unlimited)");
                println!("  --speed <MULT>       Game speed multiplier (default: 1.0)");
                println!("  --no-database        Skip loading game database (testing only)");
                println!("  -h, --help           Show this help message");
                println!("\nEnvironment:");
                println!("  OPENENSEMBLE_GAME_DIR  Path to Halo Wars game directory");
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    config
}

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("OpenEnsemble Headless Server v0.1.0");
    log::info!("====================================");

    let config = parse_args();
    log::info!(
        "Config: seed={}, max_ticks={}",
        config.random_seed,
        config.max_ticks
    );

    // Create server
    let mut server = HeadlessServer::new(config)?;

    // For now, create a simple test scenario
    let test_scenario = r#"<?xml version="1.0" encoding="utf-8"?>
<Scenario>
    <Players>
        <Player Name="Player1" Civ="UNSC" Leader1="Cutter" Team="1" />
        <Player Name="Player2" Civ="Covenant" Leader1="Arbiter" Team="2" />
    </Players>
    <Objects>
        <Object IsSquad="true" Player="1" ID="0" Position="100.0,0.0,100.0">unsc_inf_marine_01</Object>
        <Object IsSquad="true" Player="2" ID="1" Position="200.0,0.0,200.0">cov_inf_grunt_01</Object>
    </Objects>
</Scenario>"#;

    server.load_scenario_xml(test_scenario)?;

    // Run simulation
    server.run()?;

    Ok(())
}
