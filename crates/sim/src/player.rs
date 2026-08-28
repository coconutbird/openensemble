//! Player state for simulation.
//!
//! Based on `BPlayer` from the original source.

mod powers;
mod research;
mod technology;

pub use powers::{PowerEntry, PowerEntryItem, ProtoPowerId};
pub(crate) use powers::{PowerGrant, PowerRules};
pub use research::{PlayerResearchState, TechStatus};
pub use technology::PlayerTechState;
pub(crate) use technology::{
    AppliedSquadTransform, ProtoDataModification, ProtoDataRelativity, ProtoDataType,
};

/// Player ID type (0-based index).
pub type PlayerId = u8;

/// Team ID type.
pub type TeamId = u8;

/// Retail fallback used when no game-settings difficulty is supplied.
pub const DEFAULT_PLAYER_DIFFICULTY: f32 = 0.4;

/// Maximum number of teams supported by the original game.
pub const MAX_TEAMS: usize = 5;

/// Diplomacy relation between two teams.
///
/// Numeric values match the concrete relation values in the original
/// `BRelationType` enum. `Any` and `Self` are query filters rather than stored
/// diplomacy, so they are intentionally omitted here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TeamRelation {
    /// Teams cooperate and may not be targeted as enemies.
    Ally = 2,
    /// Teams are hostile.
    Enemy = 3,
    /// Teams have no hostile or allied relationship.
    #[default]
    Neutral = 4,
}

/// Civilization ID.
pub type CivId = i32;

/// Leader ID.
pub type LeaderId = i32;

/// Maximum number of resource types (matches `BCost::cMaxNumResources`).
pub const MAX_RESOURCES: usize = 4;

/// Safety limit for population types loaded from game data.
///
/// Retail sizes this table from `GameData/Pops`; the stock database currently
/// defines five entries. This is therefore a validation ceiling, not the
/// number of slots stored by [`Player`].
pub const MAX_POP_TYPES: usize = 256;

/// Safety limit for rate types loaded from `GameData/Rates`.
pub const MAX_RATE_TYPES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
struct PlayerRate {
    amount: f32,
    multiplier: f32,
}

impl Default for PlayerRate {
    fn default() -> Self {
        Self {
            amount: 0.0,
            multiplier: 1.0,
        }
    }
}

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
#[derive(Debug, Clone, Copy, Default, PartialEq)]
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

    /// Refund a previously paid cost.
    pub fn refund(&mut self, cost: &Resources) {
        for (have, amount) in self.amounts.iter_mut().zip(cost.amounts.iter()) {
            *have += amount;
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
        let required = retail_population_requirement(amount);
        let total = self.count + self.future + required;
        total <= self.cap && total <= self.max
    }

    /// Reserve future population (for units being built).
    pub fn reserve(&mut self, amount: f32) {
        self.future += amount;
    }

    /// Commit reserved population (unit finished building).
    pub fn commit(&mut self, amount: f32) {
        self.future = (self.future - amount).max(0.0);
        self.count += amount;
    }

    /// Release population reserved by canceled production.
    pub fn unreserve(&mut self, amount: f32) {
        self.future = (self.future - amount).max(0.0);
    }

    /// Add population for an entity created outside a production reservation.
    pub fn add(&mut self, amount: f32) {
        self.count += amount;
    }

    /// Release population (unit died).
    pub fn release(&mut self, amount: f32) {
        self.count = (self.count - amount).max(0.0);
    }
}

/// One population amount resolved to a runtime `GameData/Pops` slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopulationCost {
    /// Runtime population-table index.
    pub population_type: usize,
    /// Raw authored population amount.
    pub amount: f32,
}

impl PopulationCost {
    /// Construct a resolved population amount.
    #[must_use]
    pub const fn new(population_type: usize, amount: f32) -> Self {
        Self {
            population_type,
            amount,
        }
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
    /// Continuous retail difficulty scalar selected for this player.
    pub difficulty: f32,
    /// Current resources.
    pub resources: Resources,
    /// Lifetime resources gained, unaffected by spending and refunds.
    pub total_resources: Resources,
    /// Per-second resource income assigned by retail trickle effects.
    resource_trickle_rate: Resources,
    /// Scenario-layered `GameData/Rates` amounts and multipliers.
    rates: Vec<PlayerRate>,
    /// Population per type.
    pub population: Vec<Population>,
    /// Player name.
    pub name: String,
    /// Player-specific technology effects and transformed prototype state.
    pub technologies: PlayerTechState,
    /// Retail power-menu entries and their authoritative remaining uses.
    powers: powers::PlayerPowerState,
    /// Player-global technology work currently assigned to buildings.
    pub research: PlayerResearchState,
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
            difficulty: DEFAULT_PLAYER_DIFFICULTY,
            resources: Resources::new(),
            total_resources: Resources::new(),
            resource_trickle_rate: Resources::new(),
            rates: Vec::new(),
            population: Vec::new(),
            name: String::new(),
            technologies: PlayerTechState::default(),
            powers: powers::PlayerPowerState::default(),
            research: PlayerResearchState::default(),
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
        self.total_resources.add(id, amount);
    }

    /// Get a lifetime resource total.
    #[must_use]
    pub fn get_total_resource(&self, id: usize) -> f32 {
        self.total_resources.get(id)
    }

    /// Seed lifetime totals from the current starting balance.
    pub(crate) fn initialize_resource_totals(&mut self) {
        self.total_resources = self.resources;
    }

    /// Read this player's authoritative per-second resource trickle.
    #[must_use]
    pub fn resource_trickle_rate(&self) -> Resources {
        self.resource_trickle_rate
    }

    /// Replace every resource trickle rate, matching retail trigger semantics.
    pub fn set_resource_trickle_rate(&mut self, rate: Resources) {
        self.resource_trickle_rate = rate;
    }

    /// Size the player's retail rate table from `GameData/Rates`.
    pub fn configure_rate_slots(&mut self, count: usize) {
        self.rates
            .resize(count.min(MAX_RATE_TYPES), PlayerRate::default());
    }

    /// Return the number of configured scenario rate slots.
    #[must_use]
    pub fn rate_slot_count(&self) -> usize {
        self.rates.len()
    }

    /// Return one effective rate (`amount * multiplier`).
    #[must_use]
    pub fn get_rate(&self, rate_id: usize) -> f32 {
        self.rates
            .get(rate_id)
            .map_or(0.0, |rate| rate.amount * rate.multiplier)
    }

    /// Replace one base rate amount.
    pub fn set_rate_amount(&mut self, rate_id: usize, amount: f32) -> bool {
        let Some(rate) = self.rates.get_mut(rate_id) else {
            return false;
        };
        rate.amount = amount;
        true
    }

    /// Add to one base rate amount.
    pub fn add_rate_amount(&mut self, rate_id: usize, amount: f32) -> bool {
        let Some(rate) = self.rates.get_mut(rate_id) else {
            return false;
        };
        rate.amount += amount;
        true
    }

    /// Replace one rate multiplier.
    pub fn set_rate_multiplier(&mut self, rate_id: usize, multiplier: f32) -> bool {
        let Some(rate) = self.rates.get_mut(rate_id) else {
            return false;
        };
        rate.multiplier = multiplier;
        true
    }

    pub(crate) fn rate_components(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        self.rates.iter().map(|rate| (rate.amount, rate.multiplier))
    }

    pub(crate) fn update_resource_trickle(&mut self, elapsed_seconds: f32) {
        if self.state != PlayerState::Playing
            || !elapsed_seconds.is_finite()
            || elapsed_seconds <= 0.0
        {
            return;
        }
        for (resource_id, rate) in self.resource_trickle_rate.amounts.into_iter().enumerate() {
            if rate != 0.0 {
                self.add_resource(resource_id, rate * elapsed_seconds);
            }
        }
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

    /// Size the population table from `GameData/Pops`.
    pub fn configure_population_slots(&mut self, count: usize) {
        self.population
            .resize(count.min(MAX_POP_TYPES), Population::new());
    }

    /// Configure one leader-authored population cap and maximum.
    pub fn set_population_limits(&mut self, pop_type: usize, cap: f32, maximum: f32) -> bool {
        let Some(population) = self.population.get_mut(pop_type) else {
            return false;
        };
        if !cap.is_finite() || !maximum.is_finite() || cap < 0.0 || maximum < 0.0 {
            return false;
        }
        population.max = maximum.max(cap);
        population.cap = cap.min(population.max);
        true
    }

    /// Check a complete multi-type population reservation atomically.
    #[must_use]
    pub fn can_reserve_population(&self, costs: &[PopulationCost]) -> bool {
        let Some(totals) = population_totals(self.population.len(), costs) else {
            return false;
        };
        self.population
            .iter()
            .zip(totals)
            .all(|(population, amount)| population.has_room(amount))
    }

    /// Reserve raw future population for accepted production.
    pub fn reserve_population(&mut self, costs: &[PopulationCost]) -> bool {
        if !self.can_reserve_population(costs) {
            return false;
        }
        self.adjust_population(costs, PopulationAdjustment::Reserve);
        true
    }

    /// Move a production reservation into the live entity count.
    pub fn commit_reserved_population(&mut self, costs: &[PopulationCost]) {
        self.adjust_population(costs, PopulationAdjustment::Commit);
    }

    /// Release future population after cancellation or failed completion.
    pub fn release_reserved_population(&mut self, costs: &[PopulationCost]) {
        self.adjust_population(costs, PopulationAdjustment::Unreserve);
    }

    /// Add live population for scenario/debug entity creation.
    pub fn add_population(&mut self, costs: &[PopulationCost]) {
        self.adjust_population(costs, PopulationAdjustment::Add);
    }

    /// Release live population when its owning entity leaves the world.
    pub fn release_population(&mut self, costs: &[PopulationCost]) {
        self.adjust_population(costs, PopulationAdjustment::Release);
    }

    /// Apply or remove population-cap additions supplied by a live object.
    pub fn adjust_population_cap(&mut self, additions: &[PopulationCost], add: bool) {
        let Some(totals) = population_totals(self.population.len(), additions) else {
            return;
        };
        for (population, amount) in self.population.iter_mut().zip(totals) {
            population.cap = if add {
                (population.cap + amount).min(population.max)
            } else {
                (population.cap - amount).max(0.0)
            };
        }
    }

    fn adjust_population(&mut self, costs: &[PopulationCost], adjustment: PopulationAdjustment) {
        let Some(totals) = population_totals(self.population.len(), costs) else {
            return;
        };
        for (population, amount) in self.population.iter_mut().zip(totals) {
            match adjustment {
                PopulationAdjustment::Reserve => population.reserve(amount),
                PopulationAdjustment::Commit => population.commit(amount),
                PopulationAdjustment::Unreserve => population.unreserve(amount),
                PopulationAdjustment::Add => population.add(amount),
                PopulationAdjustment::Release => population.release(amount),
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PopulationAdjustment {
    Reserve,
    Commit,
    Unreserve,
    Add,
    Release,
}

fn population_totals(slot_count: usize, costs: &[PopulationCost]) -> Option<Vec<f32>> {
    let mut totals = vec![0.0; slot_count];
    for cost in costs {
        if cost.population_type >= slot_count || !cost.amount.is_finite() || cost.amount < 0.0 {
            return None;
        }
        totals[cost.population_type] += cost.amount;
    }
    Some(totals)
}

fn retail_population_requirement(amount: f32) -> f32 {
    (amount + 0.5).floor()
}

/// Gaia player ID (neutral/world player).
pub const GAIA_PLAYER: PlayerId = 0;
