//! Scenario player setup like `BScenario_setupPlayers` @ `0x140381CF0`.
//!
//! The setup reads the `.scn` players and the game settings. It creates Gaia and one world
//! player for each active scenario player, balances team-0 players, fills the team
//! relations, assigns the start positions and builds the scenario-player to world-player
//! map that object loading uses.

use pipeline::database::hw1::Database;
use pipeline::hw1::scenario::{ScenarioPlayer, ScenarioRelation};

use super::game_settings::{GameSettings, SlotType};
use super::placement::{self, PlacementPlayer};
use super::{ScenarioData, apply_leader_population, find_name_index, resources, technology};
use crate::player::{DEFAULT_PLAYER_DIFFICULTY, MAX_TEAMS, PlayerId, PlayerType, TeamRelation};
use crate::random::SimRandom;
use crate::world::World;

/// Number of scenario player numbers that an object can name (`0` Gaia and `1..=8`).
const OWNER_SLOTS: usize = 9;

/// Config that makes the random leader choice return the first `Test` leader of the civ.
const USE_TEST_LEADERS_CONFIG: &str = "UseTestLeaders";

/// Failure of the scenario player setup.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScenarioSetupError {
    /// An active scenario player has a team outside `0..=4`. The engine then stops the load.
    #[error("scenario player '{player}' has team {team}, outside 0..=4")]
    InvalidTeam {
        /// Name of the scenario player.
        player: String,
        /// Team of the scenario player.
        team: i32,
    },
}

/// One active scenario player and its world player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActivePlayer {
    /// Index of the player in the `.scn` `<Players>` element.
    pub(super) scenario_index: usize,
    /// World player of the scenario player.
    pub(super) player_id: PlayerId,
    /// Index of the assigned start position in the `.scn` `<Positions>` element.
    pub(super) start: Option<usize>,
}

/// Result of the player setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlayerSetup {
    /// World player of each scenario player number (`dword_14161BA48`).
    owners: [Option<PlayerId>; OWNER_SLOTS],
    /// Active scenario players in document order.
    pub(super) players: Vec<ActivePlayer>,
}

impl Default for PlayerSetup {
    /// The map before the `Players` element: `BScenario_load` sets the entries `1..=8` to
    /// `-1`, and entry `0` (Gaia) stays `0`.
    fn default() -> Self {
        let mut owners = [None; OWNER_SLOTS];
        owners[0] = Some(crate::player::GAIA_PLAYER);
        Self {
            owners,
            players: Vec::new(),
        }
    }
}

impl PlayerSetup {
    /// World player of the `Player` attribute of a `.scn` object.
    ///
    /// `BScenario_loadObject` @ `0x14037D6A0` skips an object with a number outside `0..=8`
    /// or a number that maps to no world player.
    pub(super) fn object_owner(&self, scenario_player: i32) -> Option<PlayerId> {
        usize::try_from(scenario_player)
            .ok()
            .and_then(|index| self.owners.get(index).copied().flatten())
    }
}

/// Create the players of a scenario like `BScenario_setupPlayers`.
///
/// # Errors
///
/// Returns [`ScenarioSetupError::InvalidTeam`] if an active player has a team outside
/// `0..=4`.
pub(super) fn setup_players(
    world: &mut World,
    scenario: &ScenarioData,
    database: &Database,
    settings: &GameSettings,
) -> Result<PlayerSetup, ScenarioSetupError> {
    let players = &scenario.players()[..scenario.players().len().min(ScenarioData::MAX_PLAYERS)];
    let mut setup = PlayerSetup::default();
    let mut teams = Vec::with_capacity(players.len());
    let mut slots = Vec::with_capacity(players.len());
    let mut team_counts = [0_i32; MAX_TEAMS];
    let mut maximum_team = 0;
    let mut used_slots = 0;
    let mut next_slot = 1;
    let mut next_player: PlayerId = 1;
    for (index, player) in players.iter().enumerate() {
        let mut team = player.team;
        let mut slot = None;
        let active = if player.controllable {
            let active = next_slot <= usize::from(settings.player_count);
            if active {
                if player.use_player_settings
                    && let Some(slot_team) = settings.slot(next_slot).and_then(|slot| slot.team)
                {
                    team = slot_team;
                }
                slot = Some(next_slot);
                used_slots += 1;
            }
            next_slot += 1;
            active
        } else {
            true
        };
        teams.push(team);
        slots.push(slot);
        if !active {
            continue;
        }
        setup.owners[index + 1] = Some(next_player);
        setup.players.push(ActivePlayer {
            scenario_index: index,
            player_id: next_player,
            start: None,
        });
        next_player += 1;
        let team_index = usize::try_from(team)
            .ok()
            .filter(|team| *team < MAX_TEAMS)
            .ok_or_else(|| ScenarioSetupError::InvalidTeam {
                player: player.name.clone(),
                team,
            })?;
        maximum_team = maximum_team.max(team);
        team_counts[team_index] += 1;
    }
    balance_team_zero(
        world.sim_random_mut(),
        &setup.players,
        &mut teams,
        &mut team_counts,
        &mut maximum_team,
        used_slots,
    );
    world.init_players(u8::try_from(setup.players.len()).unwrap_or(u8::MAX));
    configure_team_relations(world, maximum_team, scenario);
    assign_starts(world, scenario, settings, &mut setup, &teams);
    create_players(world, database, settings, players, &setup, &teams, &slots);
    technology::activate_all_starting_technologies(world, database);
    Ok(setup)
}

/// Move team-0 players into team 1 or 2 like the setup loop at `0x140382085`.
fn balance_team_zero(
    rng: &mut SimRandom,
    active: &[ActivePlayer],
    teams: &mut [i32],
    team_counts: &mut [i32; MAX_TEAMS],
    maximum_team: &mut i32,
    used_slots: i32,
) {
    if team_counts[0] <= 0 || used_slots <= 0 {
        return;
    }
    let half = (used_slots / 2).max(1);
    let mut needed = [0_i32; 3];
    for team in 1..=2 {
        needed[team] = (half - team_counts[team]).max(0);
    }
    for player in active {
        if teams[player.scenario_index] != 0 {
            continue;
        }
        // The engine takes bit 30 of the new state as the first choice.
        let mut team = usize::from((rng.next_state() >> 30) & 1 == 1) + 1;
        let mut candidates = Vec::with_capacity(2);
        for candidate in 1..=2 {
            if needed[candidate] > 0 {
                candidates.push(candidate);
            }
            if team_counts[candidate] < team_counts[team] {
                team = candidate;
            }
        }
        if let Some(last) = candidates.len().checked_sub(1) {
            let pick = rng.range(0, i32::try_from(last).unwrap_or(0));
            team = candidates[usize::try_from(pick).unwrap_or(0)];
        }
        let team_value = i32::try_from(team).unwrap_or(0);
        teams[player.scenario_index] = team_value;
        *maximum_team = (*maximum_team).max(team_value);
        team_counts[team] += 1;
        needed[team] = (needed[team] - 1).max(0);
    }
}

/// Fill the relation rows of the teams `0..=maximum_team` like the loop at `0x1403822E6`.
///
/// A team is allied with itself and neutral to team 0. Other pairs take the `Diplomacies`
/// table, or are enemies without a table. The engine stores the table value as it is; the
/// retail tables use only `Ally`, `Enemy` and `Neutral`, and this code makes the other
/// values neutral.
fn configure_team_relations(world: &mut World, maximum_team: i32, scenario: &ScenarioData) {
    let diplomacy = scenario.diplomacy_table();
    let last_team = usize::try_from(maximum_team)
        .unwrap_or(0)
        .min(MAX_TEAMS - 1);
    for from in 0..=last_team {
        for to in 0..MAX_TEAMS {
            let relation = if from == to {
                TeamRelation::Ally
            } else if from == 0 || to == 0 {
                TeamRelation::Neutral
            } else {
                diplomacy.map_or(TeamRelation::Enemy, |table| match table[from][to] {
                    ScenarioRelation::Ally => TeamRelation::Ally,
                    ScenarioRelation::Enemy => TeamRelation::Enemy,
                    ScenarioRelation::Neutral
                    | ScenarioRelation::Any
                    | ScenarioRelation::SelfTeam => TeamRelation::Neutral,
                })
            };
            let (Ok(from_team), Ok(to_team)) = (u8::try_from(from), u8::try_from(to)) else {
                continue;
            };
            let _set = world.set_team_relation(from_team, to_team, relation);
        }
    }
}

/// Assign the start positions with `BScenario_assignPlayerPositions` @ `0x140383A90`.
fn assign_starts(
    world: &World,
    scenario: &ScenarioData,
    settings: &GameSettings,
    setup: &mut PlayerSetup,
    teams: &[i32],
) {
    let positions = placement::engine_positions(scenario);
    // `BScenario_load` pins a scenario player to the last position that names it.
    let mut pinned = vec![None; teams.len()];
    for (index, position) in positions.iter().enumerate() {
        if let Some(entry) = usize::try_from(position.player)
            .ok()
            .and_then(|player| player.checked_sub(1))
            .and_then(|player| pinned.get_mut(player))
        {
            *entry = Some(index);
        }
    }
    let placement_players = setup
        .players
        .iter()
        .map(|player| PlacementPlayer {
            team: teams[player.scenario_index],
            pinned: pinned[player.scenario_index],
        })
        .collect::<Vec<_>>();
    let mut placement_random = SimRandom::new();
    placement_random.set_seed(settings.random_seed);
    let starts = placement::assign_player_positions(
        &placement_players,
        &positions,
        scenario.player_placement(),
        world.is_config_defined("NoRandomPlayerPlacement"),
        &mut placement_random,
    );
    for (player, start) in setup.players.iter_mut().zip(starts) {
        player.start = start;
    }
}

/// Create the world players in document order like the loop at `0x1403824A0`.
fn create_players(
    world: &mut World,
    database: &Database,
    settings: &GameSettings,
    players: &[ScenarioPlayer],
    setup: &PlayerSetup,
    teams: &[i32],
    slots: &[Option<usize>],
) {
    let default_difficulty = default_difficulty(database);
    configure_player_slots(world, database, default_difficulty);
    let use_test_leaders = world.is_config_defined(USE_TEST_LEADERS_CONFIG);
    let mut next_slot = 1;
    for active in &setup.players {
        let scenario_player = &players[active.scenario_index];
        let slot = slots[active.scenario_index];
        let mut civ = find_name_index(&database.civs, &scenario_player.civ, |civ| &civ.name);
        if civ < 0 {
            // `BScenario_loadPlayer` keeps civ 1 for an absent or unknown civ.
            civ = 1;
        }
        let mut leader = find_name_index(&database.leaders, &scenario_player.leader, |leader| {
            &leader.name
        });
        let slot_settings = slot.and_then(|slot| settings.slot(slot));
        if scenario_player.use_player_settings
            && let Some(slot_settings) = slot_settings
        {
            if let Some(name) = &slot_settings.civ {
                civ = find_name_index(&database.civs, name, |civ| &civ.name);
            }
            if let Some(name) = &slot_settings.leader {
                leader = find_name_index(&database.leaders, name, |leader| &leader.name);
            }
        }
        let rng = world.sim_random_mut();
        if civ < 1 {
            let civ_count = i32::try_from(database.civs.len()).unwrap_or(i32::MAX);
            civ = rng.range(0, civ_count - 2) + 1;
        }
        let random_leader = usize::try_from(leader)
            .ok()
            .and_then(|index| database.leaders.get(index))
            .is_none_or(|entry| entry.random.unwrap_or(false));
        if random_leader {
            leader = pick_random_leader_for_civ(database, civ, use_test_leaders, rng);
        }
        let (name, player_type, difficulty_slot) = if let Some(slot) = slot {
            next_slot = slot + 1;
            let name = slot_settings
                .and_then(|slot| slot.name.clone())
                .unwrap_or_else(|| format!("Player {slot}"));
            let player_type = match slot_settings.map(|slot| slot.slot_type) {
                Some(SlotType::ComputerAi) => PlayerType::ComputerAi,
                _ => PlayerType::Human,
            };
            (name, player_type, slot)
        } else {
            // An NPC reads the difficulty of the next free slot.
            (scenario_player.name.clone(), PlayerType::Npc, next_slot)
        };
        let difficulty = settings
            .slot(difficulty_slot)
            .and_then(|slot| slot.difficulty)
            .unwrap_or(default_difficulty);
        let Some(player) = world.get_player_mut(active.player_id) else {
            continue;
        };
        player.name = name;
        player.player_type = player_type;
        player.difficulty = difficulty;
        player.civ_id = civ;
        player.leader_id = leader;
        player.team_id = u8::try_from(teams[active.scenario_index]).unwrap_or_default();
        apply_leader_population(player, database, leader);
        resources::apply_leader_starting_resources(player, database, leader);
        if !scenario_player.default_resources {
            resources::apply_scenario_resources(player, database, scenario_player);
        }
    }
}

/// Give every player the default difficulty and the database population and rate slots.
fn configure_player_slots(world: &mut World, database: &Database, default_difficulty: f32) {
    let (population_count, rate_count) = slot_counts(database);
    for player in world.players_mut() {
        player.difficulty = default_difficulty;
        player.configure_population_slots(population_count);
        player.configure_rate_slots(rate_count);
    }
}

/// Return gamedata `DifficultyDefault`, or the engine default without it.
fn default_difficulty(database: &Database) -> f32 {
    database
        .game_data
        .as_ref()
        .and_then(|data| data.difficulty_default)
        .unwrap_or(DEFAULT_PLAYER_DIFFICULTY)
}

/// Return the number of gamedata `Pops` and `Rates` entries.
fn slot_counts(database: &Database) -> (usize, usize) {
    let game_data = database.game_data.as_ref();
    let population_count = game_data
        .and_then(|game_data| game_data.pops.as_ref())
        .map_or(0, |pops| pops.entries.len());
    let rate_count = game_data
        .and_then(|game_data| game_data.rates.as_ref())
        .map_or(0, |rates| rates.entries.len());
    (population_count, rate_count)
}

/// Add the NPC player of a game mode like the `GG` block of `BScenario_load` @ `0x140377BE0`.
///
/// `BWorld_addPlayer` @ `0x1405C8260` sets up an NPC player with the name, civ 0, leader 0 and
/// the default difficulty. `BWorld_addTeam` then appends a team for the player. The new team is
/// allied with itself, neutral to team 0 and an enemy of the other teams, in both directions.
/// The block adds nothing if the world has 9 players or 5 teams.
pub(super) fn add_npc_player(
    world: &mut World,
    database: &Database,
    name: &str,
) -> Option<PlayerId> {
    let team = u8::try_from(world.team_count())
        .ok()
        .filter(|team| usize::from(*team) < MAX_TEAMS)?;
    let difficulty = default_difficulty(database);
    let (population_count, rate_count) = slot_counts(database);
    let player = world.add_player()?;
    name.clone_into(&mut player.name);
    player.player_type = PlayerType::Npc;
    player.difficulty = difficulty;
    player.configure_population_slots(population_count);
    player.configure_rate_slots(rate_count);
    player.civ_id = 0;
    player.leader_id = 0;
    player.team_id = team;
    apply_leader_population(player, database, 0);
    resources::apply_leader_starting_resources(player, database, 0);
    let player_id = player.id;
    technology::activate_player_starting_technologies(world, database, player_id);
    for other in 0..MAX_TEAMS {
        let Ok(other) = u8::try_from(other) else {
            continue;
        };
        let relation = if other == team {
            TeamRelation::Ally
        } else if other == 0 {
            TeamRelation::Neutral
        } else {
            TeamRelation::Enemy
        };
        let _set = world.set_mutual_team_relation(team, other, relation);
    }
    Some(player_id)
}

/// Pick a random leader of a civ like `BScenario_pickRandomLeaderForCiv` @ `0x1403861B0`.
///
/// The candidates are the leaders of the civ without the `Random` and `Test` flags. If the
/// `UseTestLeaders` config is defined, the first `Test` leader of the civ wins without a
/// random draw. Without a candidate the result is `-1`.
fn pick_random_leader_for_civ(
    database: &Database,
    civ: i32,
    use_test_leaders: bool,
    rng: &mut SimRandom,
) -> i32 {
    let of_civ = |leader: &pipeline::database::hw1::Leader| {
        !leader.random.unwrap_or(false)
            && leader.civ.as_deref().map_or(-1, |name| {
                find_name_index(&database.civs, name, |entry| &entry.name)
            }) == civ
    };
    let mut count = 0;
    for (index, leader) in database.leaders.iter().enumerate() {
        if !of_civ(leader) {
            continue;
        }
        if !leader.test.unwrap_or(false) {
            count += 1;
        } else if use_test_leaders {
            return i32::try_from(index).unwrap_or(-1);
        }
    }
    if count == 0 {
        return -1;
    }
    let pick = usize::try_from(rng.range(0, count - 1)).unwrap_or(0);
    database
        .leaders
        .iter()
        .enumerate()
        .filter(|(_, leader)| of_civ(leader) && !leader.test.unwrap_or(false))
        .nth(pick)
        .and_then(|(index, _)| i32::try_from(index).ok())
        .unwrap_or(-1)
}

#[cfg(test)]
#[path = "player_setup/tests.rs"]
mod tests;
