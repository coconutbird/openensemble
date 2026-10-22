//! Game type and end-of-game state of the world (`BWorld_checkForGameOver`).

use super::World;
use crate::entity::Entity;
use crate::player::{PlayerId, PlayerState, PlayerStateChange, PlayerType, TeamId};
use crate::sync::SyncChecksum;

/// `BTimeManager_getUpdateTime` keeps the world updating while more than this many seconds of
/// the game-over delay remain.
const GAME_OVER_DELAY_EPSILON: f32 = 0.000_001;

/// The game type, game setting 5 (`GameType`).
///
/// Retail reads this setting to select the game-over rule and the game-mode scripts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum GameType {
    /// A skirmish game (0).
    #[default]
    Skirmish = 0,
    /// A campaign game (1).
    Campaign = 1,
    /// A scenario game (2).
    Scenario = 2,
}

/// The result of a game that ended (`BWorld_checkForGameOver`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameResult {
    /// The team whose players won.
    ///
    /// - Skirmish: the last team of teams 1 and 2 that has a playing or winning player, or
    ///   `None` if neither team has one.
    /// - Campaign and scenario: the team of the human player who won, else team 0. Retail then
    ///   also marks the players of team 0 as winners.
    pub winning_team: Option<TeamId>,
    /// The game time in milliseconds when the game ended.
    pub game_time_ms: u32,
}

/// End-of-game state of the world.
#[derive(Debug, Default)]
pub(super) struct GameEndState {
    /// Game setting 5 (`GameType`).
    game_type: GameType,
    /// The `gamemodes.xml` name of game setting 7 (`GameMode`), or `None` if the setting names
    /// no game mode.
    game_mode: Option<String>,
    /// Gamedata `GameOverDelay` in seconds. Retail uses 0 if gamedata does not set it.
    game_over_delay: f32,
    /// Player state changes that the game layer did not take yet.
    pub(super) player_state_changes: Vec<PlayerStateChange>,
    /// `world+1484 & 8`: the game is over, with its result.
    result: Option<GameResult>,
    /// `world+1348`: the seconds that the world still updates after the game ended.
    remaining_delay: f32,
}

impl GameEndState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.game_type as u8));
        if let Some(game_mode) = &self.game_mode {
            checksum.hash_u32(1);
            checksum.hash_string(game_mode);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_f32(self.game_over_delay);
        if let Some(result) = self.result {
            checksum.hash_u32(1);
            checksum.hash_u32(result.winning_team.map_or(u32::MAX, u32::from));
            checksum.hash_u32(result.game_time_ms);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_f32(self.remaining_delay);
    }
}

impl World {
    /// Return the game type.
    #[must_use]
    pub const fn game_type(&self) -> GameType {
        self.game_end.game_type
    }

    /// Set the game type. Scenario loaders set it from the game settings before they create
    /// players.
    pub const fn set_game_type(&mut self, game_type: GameType) {
        self.game_end.game_type = game_type;
    }

    /// Return the `gamemodes.xml` name of game setting 7 (`GameMode`).
    #[must_use]
    pub fn game_mode(&self) -> Option<&str> {
        self.game_end.game_mode.as_deref()
    }

    /// Set the game mode. Scenario loaders set the `gamemodes.xml` name that game setting 7
    /// selects, or `None` if no game mode has the setting name.
    pub fn set_game_mode(&mut self, game_mode: Option<String>) {
        self.game_end.game_mode = game_mode;
    }

    /// Set the gamedata `GameOverDelay` in seconds. A missing or invalid value is 0.
    pub fn set_game_over_delay(&mut self, seconds: Option<f32>) {
        self.game_end.game_over_delay = seconds.filter(|value| value.is_finite()).unwrap_or(0.0);
    }

    /// Return the result of the game, or `None` while the game continues.
    #[must_use]
    pub const fn game_result(&self) -> Option<GameResult> {
        self.game_end.result
    }

    /// Return whether the game is over. Retail then stops the trigger update and the unit
    /// cleanup of defeated players.
    #[must_use]
    pub const fn is_game_over(&self) -> bool {
        self.game_end.result.is_some()
    }

    /// Spend `elapsed_seconds` of the game-over delay like `BTimeManager_getUpdateTime`.
    ///
    /// Returns whether the world updates this tick. While the game continues, it always
    /// updates. After the game ended, it updates while more than a microsecond of the gamedata
    /// `GameOverDelay` remains, and then stops for good.
    pub fn advance_game_over_delay(&mut self, elapsed_seconds: f32) -> bool {
        if self.game_end.result.is_none() {
            return true;
        }
        if self.game_end.remaining_delay > GAME_OVER_DELAY_EPSILON {
            self.game_end.remaining_delay -= elapsed_seconds;
            return self.game_end.remaining_delay > GAME_OVER_DELAY_EPSILON;
        }
        false
    }

    /// End the game if the player states call for it, like `BWorld_checkForGameOver`.
    ///
    /// Retail calls this from `BWorld_notify` for the world events 53, 54 and 56, so it runs
    /// in the same tick as the state change. When the game ends, every player of the winning
    /// team who did not win yet changes to `Won`.
    pub(super) fn check_for_game_over(&mut self) {
        if self.game_end.result.is_some() {
            return;
        }
        let check = match self.game_end.game_type {
            GameType::Skirmish => self.skirmish_game_over(),
            GameType::Campaign | GameType::Scenario => self.human_left_game_over(),
        };
        let GameOverCheck::Ends(winning_team) = check else {
            return;
        };
        self.game_end.result = Some(GameResult {
            winning_team,
            game_time_ms: self.game_time_ms,
        });
        self.game_end.remaining_delay = self.game_end.game_over_delay;
        let Some(winning_team) = winning_team else {
            return;
        };
        for index in 1..self.players.len() {
            let Ok(player_id) = PlayerId::try_from(index) else {
                break;
            };
            if self.players[index].team_id == winning_team {
                let _won = self.set_player_state(player_id, PlayerState::Won);
            }
        }
    }

    /// Kill the units of each player that resigned or was defeated since the last update, like
    /// `BWorld_updatePlayerUnitCleanup`.
    ///
    /// Retail runs this before the world update and skips it after the game ended. It kills
    /// every live unit of the player whose prototype does not have the `UngarrisonToGaia` flag.
    pub(crate) fn update_player_unit_cleanup(&mut self) {
        if self.game_end.result.is_some() {
            return;
        }
        for index in 1..self.players.len() {
            let Ok(player_id) = PlayerId::try_from(index) else {
                break;
            };
            if !self.players[index].units_cleanup_pending() {
                continue;
            }
            let unit_ids: Vec<_> = self
                .units
                .iter()
                .filter(|(_, unit)| unit.base.player_id == player_id)
                .map(|(unit_id, _)| unit_id)
                .collect();
            for unit_id in unit_ids {
                let kill = self.get_unit(unit_id).is_some_and(|unit| {
                    unit.base.player_id == player_id
                        && unit.is_alive()
                        && !unit.ungarrison_to_gaia()
                });
                if kill {
                    let _killed = self.kill_unit(unit_id, false);
                }
            }
            self.players[index].clear_units_cleanup_pending();
        }
    }

    /// The skirmish rule.
    ///
    /// Only teams 1 and 2 count. A team is alive while one of its players plays or won. The game
    /// ends when fewer than two teams are alive (and fewer than `team count - 1`), or when no
    /// human player plays. The last alive team wins.
    fn skirmish_game_over(&self) -> GameOverCheck {
        let team_count = self.team_count();
        let mut alive_teams = 0_u16;
        let mut last_alive_team = None;
        for team in 1..team_count.min(3) {
            let alive = self.players.iter().any(|player| {
                u16::from(player.team_id) == team
                    && matches!(player.state, PlayerState::Playing | PlayerState::Won)
            });
            if alive {
                alive_teams += 1;
                last_alive_team = TeamId::try_from(team).ok();
            }
        }
        if (alive_teams < 2 && alive_teams + 1 < team_count)
            || !self.players.iter().any(|player| {
                player.state == PlayerState::Playing && player.player_type == PlayerType::Human
            })
        {
            GameOverCheck::Ends(last_alive_team)
        } else {
            GameOverCheck::Continues
        }
    }

    /// The campaign and scenario rule.
    ///
    /// The game ends at the first team (in team order) with a human player who does not play.
    /// That team wins if one of its human players won; otherwise team 0 wins.
    fn human_left_game_over(&self) -> GameOverCheck {
        for team in 1..self.team_count() {
            let mut left = false;
            let mut won = false;
            for player in &self.players {
                if u16::from(player.team_id) != team || player.player_type != PlayerType::Human {
                    continue;
                }
                left |= player.state != PlayerState::Playing;
                won |= player.state == PlayerState::Won;
            }
            if left {
                let winning_team = if won {
                    TeamId::try_from(team).ok()
                } else {
                    Some(0)
                };
                return GameOverCheck::Ends(winning_team);
            }
        }
        GameOverCheck::Continues
    }

    /// Return the retail team count: team 0 and teams 1 to the highest player team
    /// (`BScenario_setupPlayers` calls `BWorld_addTeam` for each).
    pub(crate) fn team_count(&self) -> u16 {
        self.players
            .iter()
            .map(|player| u16::from(player.team_id) + 1)
            .max()
            .unwrap_or(0)
    }
}

/// The outcome of one game-over rule.
enum GameOverCheck {
    /// The game continues.
    Continues,
    /// The game ends, and this team wins.
    Ends(Option<TeamId>),
}

#[cfg(test)]
mod tests;
