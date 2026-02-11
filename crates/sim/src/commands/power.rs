//! Power command matching vanilla BPowerCommand.

use crate::EntityId;
use crate::command::Command;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use glam::Vec4;
use std::io::{self, Read, Write};

/// Power command types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum PowerCommandType {
    #[default]
    Undefined = 0,
    GrantPower = 1,
    InvokePower = 2,
    InvokeAbility = 3,
    InvokePower2 = 4,
}

impl PowerCommandType {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Undefined),
            1 => Some(Self::GrantPower),
            2 => Some(Self::InvokePower),
            3 => Some(Self::InvokeAbility),
            4 => Some(Self::InvokePower2),
            _ => None,
        }
    }
}

/// Power command flag bits.
#[allow(dead_code)]
pub mod command_flags {
    pub const GENERIC_0: usize = 8; // cNumberCommandFlags
    pub const GENERIC_1: usize = 9;
    pub const NO_COST: usize = 10;
}

/// Power command matching vanilla BPowerCommand.
#[derive(Debug, Clone, Default)]
pub struct PowerCommand {
    /// Base command data.
    pub base: Command,
    /// Power command type.
    pub power_type: PowerCommandType,
    /// Number of uses.
    pub num_uses: i32,
    /// Proto power ID.
    pub proto_power_id: i32,
    /// Power level.
    pub power_level: i32,
    /// Ability ID.
    pub ability_id: i32,
    /// Ability squads.
    pub ability_squads: Vec<EntityId>,
    /// Power units.
    pub power_units: Vec<EntityId>,
    /// Target location (BVector has 4 components).
    pub target_location: Vec4,
    /// Multiple target locations.
    pub target_locations: Vec<Vec4>,
    /// Squad ID.
    pub squad_id: EntityId,
    /// Power user ID.
    pub power_user_id: i32,
}

impl PowerCommand {
    /// Serialize the power-specific fields (after base command).
    pub fn serialize_fields<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_i32::<LittleEndian>(self.power_type as i32)?;
        writer.write_i32::<LittleEndian>(self.num_uses)?;
        writer.write_i32::<LittleEndian>(self.proto_power_id)?;
        writer.write_i32::<LittleEndian>(self.power_level)?;
        writer.write_i32::<LittleEndian>(self.ability_id)?;
        writer.write_i32::<LittleEndian>(self.squad_id.as_u32() as i32)?;
        writer.write_u32::<LittleEndian>(self.power_user_id as u32)?;

        // Ability squads
        writer.write_u32::<LittleEndian>(self.ability_squads.len() as u32)?;
        for squad in &self.ability_squads {
            writer.write_i32::<LittleEndian>(squad.as_u32() as i32)?;
        }

        // Target locations
        writer.write_u32::<LittleEndian>(self.target_locations.len() as u32)?;
        for loc in &self.target_locations {
            writer.write_f32::<LittleEndian>(loc.x)?;
            writer.write_f32::<LittleEndian>(loc.y)?;
            writer.write_f32::<LittleEndian>(loc.z)?;
            writer.write_f32::<LittleEndian>(loc.w)?;
        }

        // Main target location
        writer.write_f32::<LittleEndian>(self.target_location.x)?;
        writer.write_f32::<LittleEndian>(self.target_location.y)?;
        writer.write_f32::<LittleEndian>(self.target_location.z)?;
        writer.write_f32::<LittleEndian>(self.target_location.w)?;

        // Power units
        writer.write_u32::<LittleEndian>(self.power_units.len() as u32)?;
        for unit in &self.power_units {
            writer.write_i32::<LittleEndian>(unit.as_u32() as i32)?;
        }

        Ok(())
    }

    /// Deserialize the power-specific fields (after base command).
    pub fn deserialize_fields<R: Read>(&mut self, reader: &mut R) -> io::Result<()> {
        let power_type = reader.read_i32::<LittleEndian>()?;
        self.power_type = PowerCommandType::from_i32(power_type).unwrap_or_default();
        self.num_uses = reader.read_i32::<LittleEndian>()?;
        self.proto_power_id = reader.read_i32::<LittleEndian>()?;
        self.power_level = reader.read_i32::<LittleEndian>()?;
        self.ability_id = reader.read_i32::<LittleEndian>()?;
        let squad_id = reader.read_i32::<LittleEndian>()?;
        self.squad_id = EntityId::from_u32(squad_id as u32);
        self.power_user_id = reader.read_u32::<LittleEndian>()? as i32;

        // Ability squads
        let num_squads = reader.read_u32::<LittleEndian>()? as usize;
        self.ability_squads.clear();
        for _ in 0..num_squads {
            let id = reader.read_i32::<LittleEndian>()?;
            self.ability_squads.push(EntityId::from_u32(id as u32));
        }

        // Target locations
        let num_locs = reader.read_u32::<LittleEndian>()? as usize;
        self.target_locations.clear();
        for _ in 0..num_locs {
            let loc = Vec4::new(
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
            );
            self.target_locations.push(loc);
        }

        // Main target location
        self.target_location = Vec4::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        );

        // Power units
        let num_units = reader.read_u32::<LittleEndian>()? as usize;
        self.power_units.clear();
        for _ in 0..num_units {
            let id = reader.read_i32::<LittleEndian>()?;
            self.power_units.push(EntityId::from_u32(id as u32));
        }

        Ok(())
    }
}
