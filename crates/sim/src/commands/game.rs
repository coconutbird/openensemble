//! Game command matching vanilla `BGameCommand`.

use crate::command::Command;
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
    #[default]
    QuickBuild = 0,
    SwitchPlayer = 1,
    AddResources = 2,
    AddPopulation = 3,
    FogOfWar = 4,
    DestroySquad = 5,
    DestroyUnit = 6,
    Resign = 7,
    Disconnect = 8,
    Flare = 9,
    SetGlobalRallyPoint = 10,
    ClearGlobalRallyPoint = 11,
    SetBuildingRallyPoint = 12,
    ClearBuildingRallyPoint = 13,
    SelectPower = 14,
    RevealMap = 15,
    CreateSquad = 16,
    CreateObject = 17,
    Tribute = 18,
    Repair = 19,
    AttackMove = 20,
    UnitPower = 21,
    LookAtPosBroadcast = 22,
    GameSpeed = 23,
    ToggleAI = 24,
    ReverseHotDrop = 25,
    ActivateSkull = 26,
    SubUpdating = 27,
}

impl GameCommandType {
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
