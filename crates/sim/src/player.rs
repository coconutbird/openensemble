//! Player state for simulation.
//!
//! Based on `BPlayer` from the original source.

/// Player ID type (0-based index).
pub type PlayerId = u8;

/// Team ID type.
pub type TeamId = u8;

/// Civilization ID.
pub type CivId = i32;

/// Leader ID.
pub type LeaderId = i32;

/// Maximum number of resource types (matches `BCost::cMaxNumResources`).
pub const MAX_RESOURCES: usize = 4;

/// Maximum number of population types.
pub const MAX_POP_TYPES: usize = 4;

/// Player state enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PlayerState {
    #[default]
    Playing = 0,
    Resigned = 1,
    Defeated = 2,
    Disconnected = 3,
    Won = 4,
}

/// Player type enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PlayerType {
    Npc = 0,
    ComputerAi = 1,
    #[default]
    Human = 2,
}

/// Resource storage (equivalent to `BCost`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Resources {
    /// Individual resource amounts.
    pub amounts: [f32; MAX_RESOURCES],
}

impl Resources {
    /// Create new empty resources.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a resource amount.
    #[must_use]
    pub fn get(&self, id: usize) -> f32 {
        self.amounts.get(id).copied().unwrap_or(0.0)
    }

    /// Set a resource amount.
    pub fn set(&mut self, id: usize, amount: f32) {
        if let Some(r) = self.amounts.get_mut(id) {
            *r = amount;
        }
    }

    /// Add to a resource.
    pub fn add(&mut self, id: usize, amount: f32) {
        if let Some(r) = self.amounts.get_mut(id) {
            *r += amount;
        }
    }

    /// Subtract from a resource.
    pub fn subtract(&mut self, id: usize, amount: f32) {
        if let Some(r) = self.amounts.get_mut(id) {
            *r -= amount;
        }
    }

    /// Get the total of all resources.
    #[must_use]
    pub fn total(&self) -> f32 {
        self.amounts.iter().sum()
    }

    /// Check if we have at least the given amounts.
    #[must_use]
    pub fn can_afford(&self, cost: &Resources) -> bool {
        self.amounts
            .iter()
            .zip(cost.amounts.iter())
            .all(|(have, need)| *have >= *need)
    }

    /// Pay a cost (subtract amounts).
    pub fn pay(&mut self, cost: &Resources) {
        for (have, need) in self.amounts.iter_mut().zip(cost.amounts.iter()) {
            *have -= need;
        }
    }
}

/// Population tracking (equivalent to `BPlayerPop`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Population {
    /// Current population count.
    pub count: f32,
    /// Current population cap.
    pub cap: f32,
    /// Maximum possible cap.
    pub max: f32,
    /// Future/reserved population (for units in production).
    pub future: f32,
}

impl Population {
    /// Create new empty population.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if we have room for more population.
    #[must_use]
    pub fn has_room(&self, amount: f32) -> bool {
        self.count + self.future + amount <= self.cap
    }

    /// Reserve future population (for units being built).
    pub fn reserve(&mut self, amount: f32) {
        self.future += amount;
    }

    /// Commit reserved population (unit finished building).
    pub fn commit(&mut self, amount: f32) {
        self.future -= amount;
        self.count += amount;
    }

    /// Release population (unit died).
    pub fn release(&mut self, amount: f32) {
        self.count -= amount;
    }
}

/// Player state container.
#[derive(Debug, Clone)]
pub struct Player {
    /// Player ID (0-based).
    pub id: PlayerId,
    /// Team ID.
    pub team_id: TeamId,
    /// Civilization ID.
    pub civ_id: CivId,
    /// Leader ID.
    pub leader_id: LeaderId,
    /// Player state.
    pub state: PlayerState,
    /// Player type (human, AI, NPC).
    pub player_type: PlayerType,
    /// Current resources.
    pub resources: Resources,
    /// Population per type.
    pub population: [Population; MAX_POP_TYPES],
    /// Player name.
    pub name: String,
}

impl Player {
    /// Create a new player with the given ID.
    #[must_use]
    pub fn new(id: PlayerId) -> Self {
        Self {
            id,
            team_id: 0,
            civ_id: -1,
            leader_id: -1,
            state: PlayerState::Playing,
            player_type: PlayerType::Human,
            resources: Resources::new(),
            population: [Population::new(); MAX_POP_TYPES],
            name: String::new(),
        }
    }

    /// Check if player is still playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.state == PlayerState::Playing
    }

    /// Check if player is human.
    #[must_use]
    pub fn is_human(&self) -> bool {
        self.player_type == PlayerType::Human
    }

    /// Check if player is AI.
    #[must_use]
    pub fn is_ai(&self) -> bool {
        self.player_type == PlayerType::ComputerAi
    }

    /// Get a resource amount.
    #[must_use]
    pub fn get_resource(&self, id: usize) -> f32 {
        self.resources.get(id)
    }

    /// Set a resource amount.
    pub fn set_resource(&mut self, id: usize, amount: f32) {
        self.resources.set(id, amount);
    }

    /// Add to a resource.
    pub fn add_resource(&mut self, id: usize, amount: f32) {
        self.resources.add(id, amount);
    }

    /// Get population for a type.
    #[must_use]
    pub fn get_population(&self, pop_type: usize) -> Option<&Population> {
        self.population.get(pop_type)
    }

    /// Get mutable population for a type.
    pub fn get_population_mut(&mut self, pop_type: usize) -> Option<&mut Population> {
        self.population.get_mut(pop_type)
    }
}

/// Gaia player ID (neutral/world player).
pub const GAIA_PLAYER: PlayerId = 0;
