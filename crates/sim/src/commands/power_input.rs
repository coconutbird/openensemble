//! Live input command for an already-running retail player power.

use crate::command::{Command, CommandType};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use glam::Vec4;
use std::io::{self, Read, Write};

const PLAYER_MASK: u32 = 0xF000_0000;
const POWER_TYPE_MASK: u32 = 0x0FF0_0000;
const REF_COUNT_MASK: u32 = 0x000F_FFFF;
const PLAYER_SHIFT: u32 = 28;
const POWER_TYPE_SHIFT: u32 = 20;

/// Packed retail `BPowerUserID` used to route live power input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PowerUserId(u32);

impl PowerUserId {
    /// Retail's invalid all-bits-set sentinel.
    pub const INVALID: Self = Self(u32::MAX);

    /// Pack the three serialized ID components using retail's bit layout.
    #[must_use]
    pub const fn new(player_id: i32, power_type: u32, ref_count: u32) -> Self {
        Self(
            (player_id.cast_unsigned() << PLAYER_SHIFT) & PLAYER_MASK
                | (power_type << POWER_TYPE_SHIFT) & POWER_TYPE_MASK
                | ref_count & REF_COUNT_MASK,
        )
    }

    /// Preserve an ID already packed by retail.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Return the packed 32-bit representation.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Whether this is not retail's invalid sentinel.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 != u32::MAX
    }

    /// Player component stored in the high nibble.
    #[must_use]
    pub const fn player_id(self) -> i32 {
        ((self.0 & PLAYER_MASK) >> PLAYER_SHIFT).cast_signed()
    }

    /// Native power-type component stored in the next byte.
    #[must_use]
    pub const fn power_type(self) -> u32 {
        (self.0 & POWER_TYPE_MASK) >> POWER_TYPE_SHIFT
    }

    /// Monotonic user reference stored in the low twenty bits.
    #[must_use]
    pub const fn ref_count(self) -> u32 {
        self.0 & REF_COUNT_MASK
    }
}

impl Default for PowerUserId {
    fn default() -> Self {
        Self::INVALID
    }
}

/// Retail `BPowerInputCommand` operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum PowerInputCommandType {
    #[default]
    Undefined = 0,
    Confirm = 1,
    Position = 2,
    Direction = 3,
    Shutdown = 4,
}

impl PowerInputCommandType {
    #[must_use]
    pub const fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Undefined),
            1 => Some(Self::Confirm),
            2 => Some(Self::Position),
            3 => Some(Self::Direction),
            4 => Some(Self::Shutdown),
            _ => None,
        }
    }
}

/// Power-input command flags extending the base command flag set.
pub mod command_flags {
    /// Bypass resource payment for this input.
    pub const NO_COST: usize = 10;
}

/// Wire-compatible input for a running native power.
#[derive(Debug, Clone)]
pub struct PowerInputCommand {
    pub base: Command,
    pub input_type: PowerInputCommandType,
    pub vector: Vec4,
    pub power_user_id: PowerUserId,
}

impl Default for PowerInputCommand {
    fn default() -> Self {
        Self {
            base: Command {
                command_type: CommandType::PowerInput,
                ..Command::default()
            },
            input_type: PowerInputCommandType::Undefined,
            vector: Vec4::ZERO,
            power_user_id: PowerUserId::INVALID,
        }
    }
}

impl PowerInputCommand {
    /// Serialize fields following the shared base command.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the destination cannot accept all fields.
    pub fn serialize_fields<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_i32::<LittleEndian>(self.input_type as i32)?;
        for component in self.vector.to_array() {
            writer.write_f32::<LittleEndian>(component)?;
        }
        writer.write_i32::<LittleEndian>(self.power_user_id.player_id())?;
        writer.write_u32::<LittleEndian>(self.power_user_id.power_type())?;
        writer.write_u32::<LittleEndian>(self.power_user_id.ref_count())?;
        Ok(())
    }

    /// Deserialize fields following the shared base command.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the source does not contain every field.
    pub fn deserialize_fields<R: Read>(&mut self, reader: &mut R) -> io::Result<()> {
        let input_type = reader.read_i32::<LittleEndian>()?;
        self.input_type = PowerInputCommandType::from_i32(input_type).unwrap_or_default();
        self.vector = Vec4::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        );
        let player_id = reader.read_i32::<LittleEndian>()?;
        let power_type = reader.read_u32::<LittleEndian>()?;
        let ref_count = reader.read_u32::<LittleEndian>()?;
        self.power_user_id = PowerUserId::new(player_id, power_type, ref_count);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_user_id_round_trips_retail_components() {
        let id = PowerUserId::new(7, 5, 0xABCDE);
        assert_eq!(id.player_id(), 7);
        assert_eq!(id.power_type(), 5);
        assert_eq!(id.ref_count(), 0xABCDE);
        assert_eq!(PowerUserId::from_raw(id.raw()), id);
        assert!(!PowerUserId::INVALID.is_valid());
    }

    #[test]
    fn fields_round_trip_component_serialization() {
        let original = PowerInputCommand {
            input_type: PowerInputCommandType::Direction,
            vector: Vec4::new(1.0, 2.0, 3.0, 4.0),
            power_user_id: PowerUserId::new(2, 5, 41),
            ..PowerInputCommand::default()
        };
        let mut bytes = Vec::new();
        original.serialize_fields(&mut bytes).unwrap();
        let mut decoded = PowerInputCommand::default();
        decoded.deserialize_fields(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded.input_type, original.input_type);
        assert_eq!(decoded.vector, original.vector);
        assert_eq!(decoded.power_user_id, original.power_user_id);
    }
}
