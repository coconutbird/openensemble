//! World state container for the simulation.
//!
//! Based on BWorld from the original source.

use crate::entities::Squad;
use crate::entity::EntityManager;
use crate::entity_id::EntityClass;
use crate::player::{GAIA_PLAYER, Player, PlayerId};
use crate::random::Random;

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
}
