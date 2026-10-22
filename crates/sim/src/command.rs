//! Command serialization matching vanilla Halo Wars wire format.
//!
//! Commands are the core of the deterministic lockstep networking.
//! Every command must serialize/deserialize to the exact same bytes
//! as the vanilla game for network compatibility.

use crate::EntityId;
use glam::Vec3;

/// Command types matching vanilla cCommandWork, cCommandPower, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CommandType {
    /// Unit work order, such as move, attack or gather (`cCommandWork`).
    Work = 0,
    /// Leader power activation (`cCommandPower`).
    Power = 1,
    /// Player input to an active leader power (`cCommandPowerInput`).
    PowerInput = 2,
    /// Building action, such as train, research or build (`cCommandBuilding`).
    Building = 3,
    /// Game-level request, such as resign or chat (`cCommandGame`).
    Game = 4,
    /// Command from a trigger script.
    Trigger = 5,
    /// General game event.
    GeneralEvent = 6,
}

impl CommandType {
    /// Converts a wire type byte to a `CommandType`.
    ///
    /// Returns `None` for an unknown value.
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Work),
            1 => Some(Self::Power),
            2 => Some(Self::PowerInput),
            3 => Some(Self::Building),
            4 => Some(Self::Game),
            5 => Some(Self::Trigger),
            6 => Some(Self::GeneralEvent),
            _ => None,
        }
    }
}

/// Entity types for senders/recipients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EntityType {
    /// Unit entities.
    Unit = 0,
    /// Squad entities.
    Squad = 1,
    /// Army entities.
    Army = 2,
    /// Players, identified by player ID.
    Player = 3,
    /// The game itself.
    Game = 4,
    /// Trigger scripts.
    Trigger = 5,
}

impl EntityType {
    /// Converts a wire entity type byte to an `EntityType`.
    ///
    /// Returns `None` for an unknown value.
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Unit),
            1 => Some(Self::Squad),
            2 => Some(Self::Army),
            3 => Some(Self::Player),
            4 => Some(Self::Game),
            5 => Some(Self::Trigger),
            _ => None,
        }
    }
}

/// Serialization flags used to compact the wire format.
/// These match vanilla cCmdIDByte, cCmdIDLong, etc.
#[derive(Debug, Clone, Copy, Default)]
pub struct CommandFlags(pub u16);

impl CommandFlags {
    /// The command ID is one byte (`cCmdIDByte`).
    pub const ID_BYTE: u16 = 2;
    /// The command ID is a 32-bit integer (`cCmdIDLong`).
    pub const ID_LONG: u16 = 4;
    /// One player sender is one byte, with no sender type or count.
    pub const ONE_SENDER_BYTE: u16 = 8;
    /// A recipient type byte is present because the recipients are not units.
    pub const NON_UNIT_RECIPIENT: u16 = 16;
    /// The command has exactly one waypoint.
    pub const ONE_WAYPOINT: u16 = 32;
    /// The command has two or more waypoints, after a count byte.
    pub const MULTI_WAYPOINTS: u16 = 64;
    /// Each waypoint includes its Y component.
    pub const WAYPOINT_Y: u16 = 128;
    /// All command flag bytes are present, not only the first byte.
    pub const EXTRA_FLAG_DATA: u16 = 256;
    /// An urgency count byte is present.
    pub const URGENCY_COUNT: u16 = 512;
    /// The recipients come from a cached unit set.
    pub const CACHED_UNIT_SET: u16 = 1024;

    /// Returns `true` if `flag` is set.
    #[must_use]
    pub fn has(self, flag: u16) -> bool {
        (self.0 & flag) != 0
    }

    /// Sets `flag`.
    pub fn set(&mut self, flag: u16) {
        self.0 |= flag;
    }
}

/// Base command structure matching vanilla `BCommand`.
#[derive(Debug, Clone)]
pub struct Command {
    /// Player who issued the command (-1 for none).
    pub player_id: i32,
    /// Command-specific ID (-1 for none).
    pub id: i32,
    /// Type of entities sending this command.
    pub sender_type: EntityType,
    /// Entity IDs of senders (player IDs for `EntityType::Player`).
    pub senders: Vec<i32>,
    /// Type of entities receiving this command.
    pub recipient_type: EntityType,
    /// Entity IDs of recipients.
    pub recipients: Vec<EntityId>,
    /// Target waypoints (positions).
    pub waypoints: Vec<Vec3>,
    /// Command-specific bit flags.
    pub flags: Vec<u8>,
    /// Command type.
    pub command_type: CommandType,
    /// Urgency count for command metering.
    pub urgency_count: u8,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            player_id: -1,
            id: -1,
            sender_type: EntityType::Player,
            senders: Vec::new(),
            recipient_type: EntityType::Unit,
            recipients: Vec::new(),
            waypoints: Vec::new(),
            flags: vec![0],
            command_type: CommandType::Work,
            urgency_count: 0,
        }
    }
}

impl Command {
    /// Retail `BCommand::cFlagAlternate` bit used to queue an order.
    pub const ALTERNATE_FLAG: usize = 0;

    /// Check if a specific command flag bit is set.
    #[must_use]
    pub fn has_flag(&self, bit: usize) -> bool {
        let byte_idx = bit / 8;
        let bit_idx = bit % 8;
        self.flags
            .get(byte_idx)
            .is_some_and(|b| (b & (1 << bit_idx)) != 0)
    }

    /// Set a specific command flag bit.
    pub fn set_flag(&mut self, bit: usize, value: bool) {
        let byte_idx = bit / 8;
        let bit_idx = bit % 8;

        // Extend flags array if needed
        while self.flags.len() <= byte_idx {
            self.flags.push(0);
        }

        if value {
            self.flags[byte_idx] |= 1 << bit_idx;
        } else {
            self.flags[byte_idx] &= !(1 << bit_idx);
        }
    }
}
