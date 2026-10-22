//! Game settings (lobby input) that a scenario load reads, like the retail `BGameSettings`.
//!
//! The engine reads these values from a settings table (name table at `0x1414B05A4`):
//! `PlayerCount` (0), `GameType` (5), `GameMode` (7), `RandomSeed` (8), and per lobby slot
//! `k` (from 1) the values `14 + 15k + f`: `Name` (0), `Type` (2), `Team` (3), `Civ` (4),
//! `Leader` (5) and `Difficulty` (11).

use crate::world::GameType;

/// Name of the retail default game mode (game mode index 0 of `gamemodes.xml`).
pub const SKIRMISH_GAME_MODE: &str = "Skirmish";

/// Player type of a lobby slot (setting `PlayerNType`).
///
/// `BScenario_setupPlayers` @ `0x140381CF0` makes a controllable scenario player a computer
/// AI player if the slot type is `1`, and a human player otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SlotType {
    /// A human player (any slot type other than `1`).
    #[default]
    Human,
    /// A computer AI player (slot type `1`).
    ComputerAi,
}

/// Settings of one lobby slot.
///
/// A `None` value is a setting that the table does not have. The engine then keeps the
/// scenario value or uses its default.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerSettings {
    /// Player name (`PlayerNName`). Without a name the engine uses `"Player N"`.
    pub name: Option<String>,
    /// Slot player type (`PlayerNType`).
    pub slot_type: SlotType,
    /// Civilization name (`PlayerNCiv`). An unknown name or `None` selects a random civ.
    pub civ: Option<String>,
    /// Leader name (`PlayerNLeader`). An unknown name, a `Random` leader or `None` selects a
    /// random leader of the civ.
    pub leader: Option<String>,
    /// Team (`PlayerNTeam`). Team `0` asks the engine to balance the player into team 1 or 2.
    pub team: Option<i32>,
    /// Difficulty scalar (`PlayerNDifficulty`). `None` uses `GameData/DifficultyDefault`.
    pub difficulty: Option<f32>,
}

impl PlayerSettings {
    /// Settings of a slot with a civ, a leader and a team.
    #[must_use]
    pub fn new(slot_type: SlotType, civ: &str, leader: &str, team: i32) -> Self {
        Self {
            name: None,
            slot_type,
            civ: Some(civ.to_owned()),
            leader: Some(leader.to_owned()),
            team: Some(team),
            difficulty: None,
        }
    }
}

/// Game settings that a scenario load reads.
#[derive(Debug, Clone, PartialEq)]
pub struct GameSettings {
    /// Game type (`GameType`).
    pub game_type: GameType,
    /// Name of the game mode of `gamemodes.xml` (`GameMode`). Only a skirmish uses it.
    pub game_mode: String,
    /// Number of active lobby slots (`PlayerCount`).
    ///
    /// Controllable scenario players fill slots `1..=player_count` in document order. Other
    /// controllable players do not get a world player.
    pub player_count: u8,
    /// Seed of the synchronized random streams (`RandomSeed`).
    pub random_seed: u32,
    /// Lobby slot settings. Slot `k` (from 1) is entry `k - 1`.
    pub players: Vec<PlayerSettings>,
}

impl GameSettings {
    /// Skirmish settings with one active slot for each entry of `players`.
    #[must_use]
    pub fn skirmish(players: Vec<PlayerSettings>, random_seed: u32) -> Self {
        Self {
            game_type: GameType::Skirmish,
            game_mode: SKIRMISH_GAME_MODE.to_owned(),
            player_count: u8::try_from(players.len()).unwrap_or(u8::MAX),
            random_seed,
            players,
        }
    }

    /// Skirmish settings for a 1v1: a human UNSC Cutter in team 1 against a computer AI
    /// Covenant Arbiter in team 2.
    #[must_use]
    pub fn skirmish_1v1(random_seed: u32) -> Self {
        Self::skirmish(
            vec![
                PlayerSettings::new(SlotType::Human, "UNSC", "Cutter", 1),
                PlayerSettings::new(SlotType::ComputerAi, "Covenant", "Arbiter", 2),
            ],
            random_seed,
        )
    }

    /// Campaign settings with one human slot. Campaign scenarios set civ, leader and team
    /// themselves.
    #[must_use]
    pub fn campaign(random_seed: u32) -> Self {
        Self {
            game_type: GameType::Campaign,
            game_mode: SKIRMISH_GAME_MODE.to_owned(),
            player_count: 1,
            random_seed,
            players: vec![PlayerSettings::default()],
        }
    }

    /// Settings of lobby slot `slot` (from 1), or `None` if the table has no such slot.
    #[must_use]
    pub fn slot(&self, slot: usize) -> Option<&PlayerSettings> {
        slot.checked_sub(1)
            .and_then(|index| self.players.get(index))
    }
}
