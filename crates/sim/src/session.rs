//! Session management for multiplayer games.
//!
//! Handles client join/leave, ready states, and game start coordination.

use crate::time_sync::TimeSync;
use std::collections::HashMap;

/// Maximum number of players in a session.
pub const MAX_PLAYERS: usize = 6;

/// Maximum number of clients (players + observers).
pub const MAX_CLIENTS: usize = 16;

/// Client state in the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClientState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Loading,
    Ready,
    Playing,
    Disconnecting,
}

/// Player information.
#[derive(Debug, Clone)]
pub struct PlayerInfo {
    /// Player ID (0-5 for players).
    pub player_id: u8,
    /// Client ID (network identifier).
    pub client_id: u64,
    /// Player name.
    pub name: String,
    /// Team ID.
    pub team: u8,
    /// Civilization/leader ID.
    pub leader_id: i32,
    /// Is this player the host?
    pub is_host: bool,
    /// Is this an AI player?
    pub is_ai: bool,
    /// Current state.
    pub state: ClientState,
    /// Ping in milliseconds.
    pub ping_ms: u32,
}

impl Default for PlayerInfo {
    fn default() -> Self {
        Self {
            player_id: 0,
            client_id: 0,
            name: String::new(),
            team: 0,
            leader_id: -1,
            is_host: false,
            is_ai: false,
            state: ClientState::Disconnected,
            ping_ms: 0,
        }
    }
}

/// Session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionState {
    #[default]
    Lobby,
    Loading,
    Playing,
    Paused,
    Ending,
}

/// Game session managing players and synchronization.
#[derive(Debug)]
pub struct Session {
    /// Session state.
    pub state: SessionState,
    /// Local player ID.
    pub local_player_id: u8,
    /// Host player ID.
    pub host_player_id: u8,
    /// Players in the session.
    pub players: HashMap<u8, PlayerInfo>,
    /// Time synchronization.
    pub time_sync: TimeSync,
    /// Current game time (in milliseconds).
    pub game_time_ms: u32,
    /// Game speed multiplier.
    pub game_speed: f32,
    /// Is the game paused?
    pub paused: bool,
    /// Random seed for the session.
    pub random_seed: u64,
    /// Map name/ID.
    pub map_name: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            state: SessionState::Lobby,
            local_player_id: 0,
            host_player_id: 0,
            players: HashMap::new(),
            time_sync: TimeSync::default(),
            game_time_ms: 0,
            game_speed: 1.0,
            paused: false,
            random_seed: 0,
            map_name: String::new(),
        }
    }
}

impl Session {
    /// Create a new session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a player to the session.
    pub fn add_player(&mut self, info: PlayerInfo) -> bool {
        if self.players.len() >= MAX_PLAYERS {
            return false;
        }
        if self.players.contains_key(&info.player_id) {
            return false;
        }
        self.players.insert(info.player_id, info);
        true
    }

    /// Remove a player from the session.
    pub fn remove_player(&mut self, player_id: u8) -> Option<PlayerInfo> {
        self.players.remove(&player_id)
    }

    /// Get a player by ID.
    pub fn get_player(&self, player_id: u8) -> Option<&PlayerInfo> {
        self.players.get(&player_id)
    }

    /// Get a mutable player by ID.
    pub fn get_player_mut(&mut self, player_id: u8) -> Option<&mut PlayerInfo> {
        self.players.get_mut(&player_id)
    }

    /// Check if all players are ready.
    pub fn all_players_ready(&self) -> bool {
        self.players
            .values()
            .all(|p| p.state == ClientState::Ready || p.is_ai)
    }

    /// Get the number of human players.
    pub fn human_player_count(&self) -> usize {
        self.players.values().filter(|p| !p.is_ai).count()
    }
}
