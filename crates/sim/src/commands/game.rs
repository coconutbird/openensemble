//! Game command matching vanilla `BGameCommand`.

use crate::command::{Command, CommandType, EntityType};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use glam::Vec3;
use std::io::{self, Read, Write};

/// Game command serialization flags.
mod flags {
    pub const DATA: u8 = 1;
    pub const DATA2: u8 = 2;
    pub const POS_X: u8 = 4;
    pub const POS_Y: u8 = 8;
    pub const POS_Z: u8 = 16;
    pub const POS2: u8 = 32;
}

/// Game command types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum GameCommandType {
    /// Debug: complete construction at once.
    #[default]
    QuickBuild = 0,
    /// Debug: control a different player.
    SwitchPlayer = 1,
    /// Debug: give resources to the player.
    AddResources = 2,
    /// Debug: give population to the player.
    AddPopulation = 3,
    /// Debug: change the fog of war. `BGameCommand_execute` (`0x140199690`) of the retail build
    /// has no case for it, so the command does nothing.
    FogOfWar = 4,
    /// Debug: destroy a squad.
    DestroySquad = 5,
    /// Debug: destroy a unit.
    DestroyUnit = 6,
    /// The player resigns from the game.
    Resign = 7,
    /// The player disconnects from the game.
    Disconnect = 8,
    /// Show a flare at `position` to allies.
    Flare = 9,
    /// Set the global rally point of the player to `position`.
    SetGlobalRallyPoint = 10,
    /// Clear the global rally point of the player.
    ClearGlobalRallyPoint = 11,
    /// Set the rally point of the building in `data` to `position`.
    SetBuildingRallyPoint = 12,
    /// Clear the rally point of the building in `data`.
    ClearBuildingRallyPoint = 13,
    /// Select a leader power.
    SelectPower = 14,
    /// Debug: reveal the map. `BGameCommand_execute` (`0x140199690`) of the retail build has no
    /// case for it, so the command does nothing.
    RevealMap = 15,
    /// Debug: create `data2` squads of prototype `data` at `position`.
    CreateSquad = 16,
    /// Debug: create `data2` objects of prototype `data` at `position`.
    CreateObject = 17,
    /// Send resources to another player.
    Tribute = 18,
    /// Repair a unit or squad.
    Repair = 19,
    /// Attack-move order.
    AttackMove = 20,
    /// Use a unit power.
    UnitPower = 21,
    /// Broadcast the camera look-at position to other players.
    LookAtPosBroadcast = 22,
    /// Change the game speed.
    GameSpeed = 23,
    /// Turn the AI of a player on or off.
    ToggleAI = 24,
    /// Reverse a hot drop.
    ReverseHotDrop = 25,
    /// Activate a skull modifier.
    ActivateSkull = 26,
    /// Change the sub-update state.
    SubUpdating = 27,
}

impl GameCommandType {
    /// Converts a wire value to a `GameCommandType`.
    ///
    /// Returns `None` for an unknown value.
    #[must_use]
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::QuickBuild),
            1 => Some(Self::SwitchPlayer),
            2 => Some(Self::AddResources),
            3 => Some(Self::AddPopulation),
            4 => Some(Self::FogOfWar),
            5 => Some(Self::DestroySquad),
            6 => Some(Self::DestroyUnit),
            7 => Some(Self::Resign),
            8 => Some(Self::Disconnect),
            9 => Some(Self::Flare),
            10 => Some(Self::SetGlobalRallyPoint),
            11 => Some(Self::ClearGlobalRallyPoint),
            12 => Some(Self::SetBuildingRallyPoint),
            13 => Some(Self::ClearBuildingRallyPoint),
            14 => Some(Self::SelectPower),
            15 => Some(Self::RevealMap),
            16 => Some(Self::CreateSquad),
            17 => Some(Self::CreateObject),
            18 => Some(Self::Tribute),
            19 => Some(Self::Repair),
            20 => Some(Self::AttackMove),
            21 => Some(Self::UnitPower),
            22 => Some(Self::LookAtPosBroadcast),
            23 => Some(Self::GameSpeed),
            24 => Some(Self::ToggleAI),
            25 => Some(Self::ReverseHotDrop),
            26 => Some(Self::ActivateSkull),
            27 => Some(Self::SubUpdating),
            _ => None,
        }
    }
}

/// Game command matching vanilla `BGameCommand`.
#[derive(Debug, Clone, Default)]
pub struct GameCommand {
    /// Base command data.
    pub base: Command,
    /// Game command type.
    pub game_type: GameCommandType,
    /// Generic data field (can be i32 or f32).
    pub data: i32,
    /// Second generic data field.
    pub data2: i32,
    /// Position.
    pub position: Vec3,
    /// Second position.
    pub position2: Vec3,
}

impl GameCommand {
    /// Build a vanilla-compatible debug command that creates squad prototypes.
    #[must_use]
    pub fn create_squads(player_id: i32, proto_squad_id: i32, count: i32, position: Vec3) -> Self {
        Self {
            base: Command {
                player_id,
                sender_type: EntityType::Player,
                senders: vec![player_id],
                recipient_type: EntityType::Game,
                command_type: CommandType::Game,
                ..Command::default()
            },
            game_type: GameCommandType::CreateSquad,
            data: proto_squad_id,
            data2: count,
            position,
            ..Self::default()
        }
    }

    /// Build a vanilla-compatible debug command that creates object prototypes.
    #[must_use]
    pub fn create_objects(
        player_id: i32,
        proto_object_id: i32,
        count: i32,
        position: Vec3,
    ) -> Self {
        Self {
            base: Command {
                player_id,
                sender_type: EntityType::Player,
                senders: vec![player_id],
                recipient_type: EntityType::Game,
                command_type: CommandType::Game,
                ..Command::default()
            },
            game_type: GameCommandType::CreateObject,
            data: proto_object_id,
            data2: count,
            position,
            ..Self::default()
        }
    }

    /// Get data as f32.
    #[must_use]
    pub fn data_float(&self) -> f32 {
        f32::from_bits(self.data.cast_unsigned())
    }

    /// Set data as f32.
    pub fn set_data_float(&mut self, value: f32) {
        self.data = value.to_bits().cast_signed();
    }

    /// Get data2 as f32.
    #[must_use]
    pub fn data2_float(&self) -> f32 {
        f32::from_bits(self.data2.cast_unsigned())
    }

    /// Set data2 as f32.
    pub fn set_data2_float(&mut self, value: f32) {
        self.data2 = value.to_bits().cast_signed();
    }

    /// Serialize the game-specific fields (after base command).
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the writer cannot accept the serialized fields.
    pub fn serialize_fields<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut game_flags: u8 = 0;

        if self.data != 0 {
            game_flags |= flags::DATA;
        }
        if self.data2 != 0 {
            game_flags |= flags::DATA2;
        }
        if self.position.x != 0.0 {
            game_flags |= flags::POS_X;
        }
        if self.position.y != 0.0 {
            game_flags |= flags::POS_Y;
        }
        if self.position.z != 0.0 {
            game_flags |= flags::POS_Z;
        }
        if self.position2 != Vec3::ZERO {
            game_flags |= flags::POS2;
        }

        // Write type as u8
        writer.write_u8(self.game_type as u8)?;
        writer.write_u8(game_flags)?;

        if game_flags & flags::DATA != 0 {
            writer.write_i32::<LittleEndian>(self.data)?;
        }
        if game_flags & flags::DATA2 != 0 {
            writer.write_i32::<LittleEndian>(self.data2)?;
        }
        if game_flags & flags::POS_X != 0 {
            writer.write_f32::<LittleEndian>(self.position.x)?;
        }
        if game_flags & flags::POS_Y != 0 {
            writer.write_f32::<LittleEndian>(self.position.y)?;
        }
        if game_flags & flags::POS_Z != 0 {
            writer.write_f32::<LittleEndian>(self.position.z)?;
        }
        if game_flags & flags::POS2 != 0 {
            writer.write_f32::<LittleEndian>(self.position2.x)?;
            writer.write_f32::<LittleEndian>(self.position2.y)?;
            writer.write_f32::<LittleEndian>(self.position2.z)?;
        }

        Ok(())
    }

    /// Deserialize the game-specific fields (after base command).
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the reader does not contain a complete command.
    pub fn deserialize_fields<R: Read>(&mut self, reader: &mut R) -> io::Result<()> {
        let type_byte = reader.read_u8()?;
        self.game_type = if type_byte == 255 {
            // -1 as u8
            GameCommandType::QuickBuild // or handle as invalid
        } else {
            GameCommandType::from_i32(i32::from(type_byte)).unwrap_or_default()
        };

        let game_flags = reader.read_u8()?;

        self.data = 0;
        if game_flags & flags::DATA != 0 {
            self.data = reader.read_i32::<LittleEndian>()?;
        }

        self.data2 = 0;
        if game_flags & flags::DATA2 != 0 {
            self.data2 = reader.read_i32::<LittleEndian>()?;
        }

        self.position = Vec3::ZERO;
        if game_flags & flags::POS_X != 0 {
            self.position.x = reader.read_f32::<LittleEndian>()?;
        }
        if game_flags & flags::POS_Y != 0 {
            self.position.y = reader.read_f32::<LittleEndian>()?;
        }
        if game_flags & flags::POS_Z != 0 {
            self.position.z = reader.read_f32::<LittleEndian>()?;
        }

        self.position2 = Vec3::ZERO;
        if game_flags & flags::POS2 != 0 {
            self.position2.x = reader.read_f32::<LittleEndian>()?;
            self.position2.y = reader.read_f32::<LittleEndian>()?;
            self.position2.z = reader.read_f32::<LittleEndian>()?;
        }

        Ok(())
    }
}
