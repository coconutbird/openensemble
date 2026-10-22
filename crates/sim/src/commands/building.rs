//! Building command matching retail `BBuildingCommand`.

use crate::EntityId;
use crate::command::{Command, CommandType, EntityType};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use glam::Vec3;
use std::io::{self, Read, Write};

/// Command-button types from retail `BProtoObjectCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum BuildingCommandType {
    /// Invalid/uninitialized command type.
    #[default]
    Undefined = -1,
    /// Queue or cancel a technology research.
    Research = 0,
    /// Queue or cancel the training of a standalone unit.
    TrainUnit = 1,
    /// Place or cancel a building at a position or socket.
    Build = 2,
    /// Queue or cancel the training of a squad.
    TrainSquad = 3,
    /// Unload the units that the recipient carries.
    UnloadUnits = 4,
    /// Reinforce a squad.
    Reinforce = 5,
    /// Change the squad mode of the recipients.
    ChangeMode = 6,
    /// Use an ability.
    Ability = 7,
    /// Start the self-destruct of the recipient.
    Kill = 8,
    /// Cancel a pending self-destruct.
    CancelKill = 9,
    /// Send resources to another player.
    Tribute = 10,
    /// Run a custom command that a trigger script defines.
    CustomCommand = 11,
    /// Use a leader power from a building.
    Power = 12,
    /// Build an add-on building in a socket of the recipient.
    BuildOther = 13,
    /// Lock a training queue.
    TrainLock = 14,
    /// Unlock a training queue.
    TrainUnlock = 15,
    /// Set the rally point of the recipient.
    RallyPoint = 16,
    /// Clear the rally point of the recipient.
    ClearRallyPoint = 17,
    /// Destroy a base.
    DestroyBase = 18,
    /// Cancel the destruction of a base.
    CancelDestroyBase = 19,
    /// Reverse a hot drop.
    ReverseHotDrop = 20,
}

impl BuildingCommandType {
    /// Converts a wire value to a `BuildingCommandType`.
    ///
    /// Returns `None` for an unknown value.
    #[must_use]
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            -1 => Some(Self::Undefined),
            0 => Some(Self::Research),
            1 => Some(Self::TrainUnit),
            2 => Some(Self::Build),
            3 => Some(Self::TrainSquad),
            4 => Some(Self::UnloadUnits),
            5 => Some(Self::Reinforce),
            6 => Some(Self::ChangeMode),
            7 => Some(Self::Ability),
            8 => Some(Self::Kill),
            9 => Some(Self::CancelKill),
            10 => Some(Self::Tribute),
            11 => Some(Self::CustomCommand),
            12 => Some(Self::Power),
            13 => Some(Self::BuildOther),
            14 => Some(Self::TrainLock),
            15 => Some(Self::TrainUnlock),
            16 => Some(Self::RallyPoint),
            17 => Some(Self::ClearRallyPoint),
            18 => Some(Self::DestroyBase),
            19 => Some(Self::CancelDestroyBase),
            20 => Some(Self::ReverseHotDrop),
            _ => None,
        }
    }
}

/// Fixed payload appended by retail `BBuildingCommand` after `BCommand`.
#[derive(Debug, Clone)]
pub struct BuildingCommand {
    /// Common command header (`BCommand`).
    pub base: Command,
    /// Action that the building does. Wire field 1, `i32`.
    pub building_type: BuildingCommandType,
    /// Runtime table index for the command target, not its authored DBID.
    pub target_id: i32,
    /// World position for a placed building. Wire fields 3 to 5, three `f32` values.
    pub target_position: Vec3,
    /// Positive queues work; negative cancels it.
    pub count: i32,
    /// Socket entity for a socketed building, or `EntityId::INVALID`. Wire field 7, `i32`.
    pub socket_id: EntityId,
}

impl Default for BuildingCommand {
    fn default() -> Self {
        Self {
            base: Command {
                command_type: CommandType::Building,
                ..Command::default()
            },
            building_type: BuildingCommandType::Undefined,
            target_id: -1,
            target_position: Vec3::ZERO,
            count: 0,
            socket_id: EntityId::INVALID,
        }
    }
}

impl BuildingCommand {
    /// Construct retail's direct object-command mode transition.
    #[must_use]
    pub fn change_mode(
        player_id: i32,
        recipients: Vec<EntityId>,
        mode: crate::entities::SquadMode,
    ) -> Self {
        Self {
            base: production_base(player_id, recipients),
            building_type: BuildingCommandType::ChangeMode,
            target_id: mode as i32,
            count: 1,
            ..Self::default()
        }
    }

    /// Construct a retail research/cancel command for one or more buildings.
    #[must_use]
    pub fn research(
        player_id: i32,
        recipients: Vec<EntityId>,
        technology_id: i32,
        count: i32,
    ) -> Self {
        Self {
            base: Command {
                player_id,
                sender_type: EntityType::Player,
                senders: vec![player_id],
                recipient_type: EntityType::Unit,
                recipients,
                command_type: CommandType::Building,
                ..Command::default()
            },
            building_type: BuildingCommandType::Research,
            target_id: technology_id,
            count,
            ..Self::default()
        }
    }

    /// Construct a retail standalone-unit training/cancel command.
    #[must_use]
    pub fn train_units(
        player_id: i32,
        recipients: Vec<EntityId>,
        prototype_id: i32,
        count: i32,
    ) -> Self {
        Self::training(
            player_id,
            recipients,
            BuildingCommandType::TrainUnit,
            prototype_id,
            count,
        )
    }

    /// Construct a retail squad training/cancel command.
    #[must_use]
    pub fn train_squads(
        player_id: i32,
        recipients: Vec<EntityId>,
        prototype_id: i32,
        count: i32,
    ) -> Self {
        Self::training(
            player_id,
            recipients,
            BuildingCommandType::TrainSquad,
            prototype_id,
            count,
        )
    }

    /// Construct a retail direct-building construction/cancel command.
    #[must_use]
    pub fn build(
        player_id: i32,
        recipients: Vec<EntityId>,
        prototype_id: i32,
        position: Vec3,
        count: i32,
        socket_id: EntityId,
    ) -> Self {
        Self {
            base: production_base(player_id, recipients),
            building_type: BuildingCommandType::Build,
            target_id: prototype_id,
            target_position: position,
            count,
            socket_id,
        }
    }

    /// Construct a retail socket-building construction/cancel command.
    #[must_use]
    pub fn build_other(
        player_id: i32,
        recipients: Vec<EntityId>,
        prototype_id: i32,
        count: i32,
    ) -> Self {
        Self {
            base: production_base(player_id, recipients),
            building_type: BuildingCommandType::BuildOther,
            target_id: prototype_id,
            count,
            ..Self::default()
        }
    }

    fn training(
        player_id: i32,
        recipients: Vec<EntityId>,
        building_type: BuildingCommandType,
        prototype_id: i32,
        count: i32,
    ) -> Self {
        Self {
            base: production_base(player_id, recipients),
            building_type,
            target_id: prototype_id,
            count,
            ..Self::default()
        }
    }

    /// Serialize the seven fixed retail fields following the base command.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the writer fails.
    pub fn serialize_fields<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_i32::<LittleEndian>(self.building_type as i32)?;
        writer.write_i32::<LittleEndian>(self.target_id)?;
        writer.write_f32::<LittleEndian>(self.target_position.x)?;
        writer.write_f32::<LittleEndian>(self.target_position.y)?;
        writer.write_f32::<LittleEndian>(self.target_position.z)?;
        writer.write_i32::<LittleEndian>(self.count)?;
        writer.write_i32::<LittleEndian>(self.socket_id.as_u32().cast_signed())?;
        Ok(())
    }

    /// Deserialize the seven fixed retail fields following the base command.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the reader does not contain a complete payload.
    pub fn deserialize_fields<R: Read>(&mut self, reader: &mut R) -> io::Result<()> {
        let building_type = reader.read_i32::<LittleEndian>()?;
        self.building_type = BuildingCommandType::from_i32(building_type).unwrap_or_default();
        self.target_id = reader.read_i32::<LittleEndian>()?;
        self.target_position = Vec3::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        );
        self.count = reader.read_i32::<LittleEndian>()?;
        self.socket_id = EntityId::from_u32(reader.read_i32::<LittleEndian>()?.cast_unsigned());
        Ok(())
    }
}

fn production_base(player_id: i32, recipients: Vec<EntityId>) -> Command {
    Command {
        player_id,
        sender_type: EntityType::Player,
        senders: vec![player_id],
        recipient_type: EntityType::Unit,
        recipients,
        command_type: CommandType::Building,
        ..Command::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_fields_match_retail_order_and_roundtrip() {
        let mut command =
            BuildingCommand::research(2, vec![EntityId::from_u32(0x1000_0004)], 0x1122_3344, -1);
        command.target_position = Vec3::new(1.0, 2.0, 3.0);
        command.socket_id = EntityId::from_u32(0x1000_0008);

        let mut bytes = Vec::new();
        command.serialize_fields(&mut bytes).unwrap();
        assert_eq!(bytes.len(), 28);
        assert_eq!(&bytes[0..4], &0_i32.to_le_bytes());
        assert_eq!(&bytes[4..8], &0x1122_3344_i32.to_le_bytes());
        assert_eq!(&bytes[20..24], &(-1_i32).to_le_bytes());
        assert_eq!(&bytes[24..28], &0x1000_0008_i32.to_le_bytes());

        let mut decoded = BuildingCommand::default();
        decoded.deserialize_fields(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded.building_type, BuildingCommandType::Research);
        assert_eq!(decoded.target_id, command.target_id);
        assert_eq!(decoded.target_position, command.target_position);
        assert_eq!(decoded.count, command.count);
        assert_eq!(decoded.socket_id, command.socket_id);
    }
}
