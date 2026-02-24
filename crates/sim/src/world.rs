//! World state container for the simulation.
//!
//! Based on BWorld from the original source.

use crate::entities::Squad;
use crate::entity::EntityManager;
use crate::entity_id::EntityClass;
use crate::player::{GAIA_PLAYER, Player, PlayerId};
use crate::random::Random;
use crate::sync::SyncChecksum;

/// Maximum supported players.
pub const MAX_PLAYERS: usize = 8;

/// World state container.
///
/// Contains all game state: players, entities, time, etc.
#[derive(Debug)]
pub struct World {
    /// All players (index = player ID).
    players: Vec<Player>,
    /// Current game time in milliseconds.
    pub game_time_ms: u32,
    /// Deterministic RNG for the world.
    pub rng: Random,
    /// Squad entity manager.
    pub squads: EntityManager<Squad>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Create a new empty world.
    pub fn new() -> Self {
        Self {
            players: Vec::new(),
            game_time_ms: 0,
            rng: Random::new(),
            squads: EntityManager::new(EntityClass::Squad),
        }
    }

    /// Create a world with a specific random seed.
    pub fn with_seed(seed: u64) -> Self {
        let mut world = Self::new();
        world.rng.set_seed64(seed);
        world
    }

    /// Initialize the world with the given number of players.
    ///
    /// Creates player 0 as Gaia (neutral) and players 1..=count as active players.
    pub fn init_players(&mut self, player_count: u8) {
        self.players.clear();

        // Player 0 is always Gaia (neutral/world player)
        let mut gaia = Player::new(GAIA_PLAYER);
        gaia.name = "Gaia".to_string();
        gaia.player_type = crate::player::PlayerType::Npc;
        self.players.push(gaia);

        // Create active players
        for i in 1..=player_count {
            let player = Player::new(i);
            self.players.push(player);
        }
    }

    /// Get the number of players (including Gaia).
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Get a player by ID.
    pub fn get_player(&self, id: PlayerId) -> Option<&Player> {
        self.players.get(id as usize)
    }

    /// Get a mutable player by ID.
    pub fn get_player_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.players.get_mut(id as usize)
    }

    /// Iterate over all players.
    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.players.iter()
    }

    /// Iterate over all players mutably.
    pub fn players_mut(&mut self) -> impl Iterator<Item = &mut Player> {
        self.players.iter_mut()
    }

    /// Iterate over active (non-Gaia) players.
    pub fn active_players(&self) -> impl Iterator<Item = &Player> {
        self.players.iter().skip(1)
    }

    /// Iterate over active (non-Gaia) players mutably.
    pub fn active_players_mut(&mut self) -> impl Iterator<Item = &mut Player> {
        self.players.iter_mut().skip(1)
    }

    /// Get current game time in milliseconds.
    pub fn game_time(&self) -> u32 {
        self.game_time_ms
    }

    /// Advance game time by the given milliseconds.
    pub fn advance_time(&mut self, ms: u32) {
        self.game_time_ms = self.game_time_ms.wrapping_add(ms);
    }

    /// Reset the world to initial state.
    pub fn reset(&mut self) {
        self.players.clear();
        self.game_time_ms = 0;
        self.squads.clear();
    }

    /// Create a new squad for the given player.
    pub fn create_squad(&mut self, player_id: PlayerId) -> crate::entity_id::EntityId {
        let id = self.squads.allocate_id();
        let squad = Squad::new(id, player_id);
        self.squads.insert(id, squad);
        id
    }

    /// Create a new squad at a specific position.
    pub fn create_squad_at(
        &mut self,
        player_id: PlayerId,
        position: glam::Vec3,
    ) -> crate::entity_id::EntityId {
        let id = self.squads.allocate_id();
        let mut squad = Squad::new(id, player_id);
        squad.set_position(position);
        self.squads.insert(id, squad);
        id
    }

    /// Get a squad by ID.
    pub fn get_squad(&self, id: crate::entity_id::EntityId) -> Option<&Squad> {
        self.squads.get(id)
    }

    /// Get a mutable squad by ID.
    pub fn get_squad_mut(&mut self, id: crate::entity_id::EntityId) -> Option<&mut Squad> {
        self.squads.get_mut(id)
    }

    /// Update all entities for one tick.
    pub fn update_entities(&mut self, dt: f32) {
        self.squads.update_all(dt);
    }

    /// Compute a checksum of the entire world state for sync verification.
    ///
    /// This hashes all deterministic state: game time, players, entities.
    /// Two simulations with the same inputs should produce identical checksums.
    pub fn checksum(&self) -> u32 {
        let mut cs = SyncChecksum::new();

        // Hash game time
        cs.hash_u32(self.game_time_ms);

        // Hash player count
        cs.hash_u32(self.players.len() as u32);

        // Hash each player's state
        for player in &self.players {
            cs.hash_u32(player.id as u32);
            cs.hash_u32(player.team_id as u32);
            cs.hash_i32(player.civ_id);
            cs.hash_i32(player.leader_id);
            cs.hash_u32(player.state as u32);
            cs.hash_u32(player.player_type as u32);

            // Hash resources
            for &amount in &player.resources.amounts {
                cs.hash_f32(amount);
            }

            // Hash population
            for pop in &player.population {
                cs.hash_f32(pop.count);
                cs.hash_f32(pop.max);
                cs.hash_f32(pop.cap);
                cs.hash_f32(pop.future);
            }
        }

        // Hash squad count
        cs.hash_u32(self.squads.len() as u32);

        // Hash each squad's state (iteration order is deterministic via BTreeMap)
        for (_id, squad) in self.squads.iter() {
            // Base entity data
            cs.hash_u32(squad.base.id.as_u32());
            cs.hash_u32(squad.base.player_id as u32);
            cs.hash_vec3(
                squad.base.position.x,
                squad.base.position.y,
                squad.base.position.z,
            );
            cs.hash_vec3(
                squad.base.forward.x,
                squad.base.forward.y,
                squad.base.forward.z,
            );
            cs.hash_vec3(
                squad.base.velocity.x,
                squad.base.velocity.y,
                squad.base.velocity.z,
            );
            cs.hash_u32(squad.base.alive as u32);

            // Squad-specific data
            cs.hash_u32(squad.state as u32);
            cs.hash_f32(squad.speed);
            cs.hash_i32(squad.proto_squad_id);

            // Move target (if any)
            if let Some(target) = squad.move_target {
                cs.hash_u32(1); // has target
                cs.hash_vec3(target.x, target.y, target.z);
            } else {
                cs.hash_u32(0); // no target
            }
        }

        cs.value()
    }

    /// Compute a full checksum that also includes RNG state.
    ///
    /// This is useful for detecting divergence in the random number generator,
    /// which would cause future simulation divergence even if current state matches.
    pub fn checksum_with_rng(&self) -> u32 {
        let mut cs = SyncChecksum::new();

        // Start with regular world checksum
        cs.hash_u32(self.checksum());

        // Hash RNG state by sampling it (non-destructive check)
        // We can't read internal RNG state directly, so we hash a characteristic
        // We'll use a copy to sample without affecting the original
        let mut rng_copy = self.rng.clone();
        for _ in 0..8 {
            cs.hash_u32(rng_copy.u_rand());
        }

        cs.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{Scenario, load_scenario_into_world};
    use crate::simulation::Simulation;

    /// Sample scenario for testing
    const TEST_SCENARIO: &str = r#"<?xml version="1.0"?>
<Scenario>
  <Players>
    <Player id="1" name="TestPlayer1" civ="UNSC" leader="Cutter" team="1" />
    <Player id="2" name="TestPlayer2" civ="Covenant" leader="Arbiter" team="2" />
  </Players>
  <Objects>
    <Squad player="1" protoSquad="unsc_inf_marine_01" x="10.0" z="20.0" />
    <Squad player="2" protoSquad="cov_inf_grunt_01" x="50.0" z="60.0" />
  </Objects>
</Scenario>"#;

    /// Run a simulation for a given number of ticks and return the final checksum.
    fn run_simulation(seed: u64, ticks: u32) -> (u32, u32) {
        let scenario = Scenario::from_xml_str(TEST_SCENARIO).unwrap();
        let mut loaded = load_scenario_into_world(&scenario);
        loaded.world.rng.set_seed64(seed);

        let mut sim = Simulation::with_seed(seed);
        sim.start();

        for _ in 0..ticks {
            sim.tick_with_world(&mut loaded.world);
        }

        (loaded.world.checksum(), loaded.world.checksum_with_rng())
    }

    #[test]
    fn test_world_checksum_deterministic() {
        // Same world should always produce same checksum
        let mut world1 = World::with_seed(12345);
        world1.init_players(2);
        world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        let mut world2 = World::with_seed(12345);
        world2.init_players(2);
        world2.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        assert_eq!(world1.checksum(), world2.checksum());
    }

    #[test]
    fn test_world_checksum_different_state() {
        // Different state should produce different checksum
        let mut world1 = World::with_seed(12345);
        world1.init_players(2);
        world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        let mut world2 = World::with_seed(12345);
        world2.init_players(2);
        world2.create_squad_at(1, glam::Vec3::new(15.0, 0.0, 25.0)); // Different position

        assert_ne!(world1.checksum(), world2.checksum());
    }

    #[test]
    fn test_simulation_determinism_same_seed() {
        // Run simulation twice with same seed - must produce identical checksums
        let seed = 42;
        let ticks = 100;

        let (cs1, cs1_rng) = run_simulation(seed, ticks);
        let (cs2, cs2_rng) = run_simulation(seed, ticks);

        assert_eq!(cs1, cs2, "World checksum mismatch after {} ticks", ticks);
        assert_eq!(
            cs1_rng, cs2_rng,
            "World+RNG checksum mismatch after {} ticks",
            ticks
        );
    }

    #[test]
    fn test_simulation_different_seeds_different_rng() {
        // Different seeds should produce different RNG states
        // Note: World state (without RNG) may be identical if no random operations occur
        let (_, cs1_rng) = run_simulation(42, 100);
        let (_, cs2_rng) = run_simulation(43, 100);

        assert_ne!(
            cs1_rng, cs2_rng,
            "Different seeds should produce different RNG states"
        );
    }

    #[test]
    fn test_simulation_determinism_long_run() {
        // Run for longer to catch subtle non-determinism
        let seed = 99999;
        let ticks = 1000;

        let (cs1, cs1_rng) = run_simulation(seed, ticks);
        let (cs2, cs2_rng) = run_simulation(seed, ticks);

        assert_eq!(cs1, cs2, "World checksum mismatch after {} ticks", ticks);
        assert_eq!(
            cs1_rng, cs2_rng,
            "World+RNG checksum mismatch after {} ticks",
            ticks
        );
    }

    #[test]
    fn test_simulation_checksum_changes_over_time() {
        // Checksum should change as simulation progresses (state evolves)
        let seed = 12345;

        let (cs_10, _) = run_simulation(seed, 10);
        let (cs_100, _) = run_simulation(seed, 100);

        // At minimum, game_time_ms changes, so checksums should differ
        assert_ne!(cs_10, cs_100, "Checksum should change over time");
    }

    /// Run simulation with movement commands for determinism testing.
    fn run_simulation_with_movement(seed: u64, ticks: u32) -> (u32, u32) {
        let scenario = Scenario::from_xml_str(TEST_SCENARIO).unwrap();
        let mut loaded = load_scenario_into_world(&scenario);
        loaded.world.rng.set_seed64(seed);

        let mut sim = Simulation::with_seed(seed);
        sim.start();

        // Issue move commands to squads after a few ticks
        for tick in 0..ticks {
            if tick == 10 {
                // Issue move orders to all squads
                for (_id, squad) in loaded.world.squads.iter_mut() {
                    let target = glam::Vec3::new(100.0, 0.0, 100.0);
                    squad.move_to(target);
                }
            }
            sim.tick_with_world(&mut loaded.world);
        }

        (loaded.world.checksum(), loaded.world.checksum_with_rng())
    }

    #[test]
    fn test_simulation_determinism_with_movement() {
        // Run simulation with movement twice - must be identical
        let seed = 7777;
        let ticks = 200;

        let (cs1, cs1_rng) = run_simulation_with_movement(seed, ticks);
        let (cs2, cs2_rng) = run_simulation_with_movement(seed, ticks);

        assert_eq!(
            cs1, cs2,
            "World checksum mismatch with movement after {} ticks",
            ticks
        );
        assert_eq!(
            cs1_rng, cs2_rng,
            "World+RNG checksum mismatch with movement after {} ticks",
            ticks
        );
    }

    #[test]
    fn test_movement_changes_checksum() {
        // Directly verify that issuing a move command changes the checksum
        let mut world1 = World::with_seed(12345);
        world1.init_players(2);
        let squad_id = world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        let mut world2 = World::with_seed(12345);
        world2.init_players(2);
        let squad_id2 = world2.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        // Initially should be equal
        assert_eq!(world1.checksum(), world2.checksum());

        // Issue move command to world2's squad
        if let Some(squad) = world2.get_squad_mut(squad_id2) {
            squad.move_to(glam::Vec3::new(100.0, 0.0, 100.0));
        }

        // Now checksums should differ (squad has move_target set)
        assert_ne!(
            world1.checksum(),
            world2.checksum(),
            "Move command should change checksum"
        );

        // Run some ticks on world2 to actually move
        for _ in 0..20 {
            world2.update_entities(0.05); // 50ms
            world2.game_time_ms += 50;
        }

        // Position should have changed
        let squad1_pos = world1.get_squad(squad_id).unwrap().position();
        let squad2_pos = world2.get_squad(squad_id2).unwrap().position();
        assert_ne!(
            squad1_pos, squad2_pos,
            "Squad should have moved after update"
        );
    }
}
