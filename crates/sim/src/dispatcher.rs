//! Command dispatcher for routing incoming packets to the right command type.

use crate::command::CommandType;
use crate::commands::{
    BuildingCommand, CommandPayload, GameCommand, PowerCommand, PowerInputCommand, WorkCommand,
};
use crate::packet::{ChannelPacketHeader, PacketError, channel_packet_type};
use crate::serialize::{SerializeError, deserialize_command};
use std::io::{Cursor, Read};
use thiserror::Error;

/// Failure while the dispatcher parses a command packet.
#[derive(Error, Debug)]
pub enum DispatchError {
    /// The channel packet header is not valid.
    #[error("Packet error: {0}")]
    Packet(#[from] PacketError),
    /// The base command is not valid.
    #[error("Serialize error: {0}")]
    Serialize(#[from] SerializeError),
    /// The reader failed in the command-specific fields.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// The packet type is not a supported command packet.
    #[error("Unknown packet type: {0}")]
    UnknownPacketType(u8),
    /// The command type in the payload does not match the packet type.
    #[error("Command type mismatch: expected {expected:?}, got {actual:?}")]
    TypeMismatch {
        /// Command type that the packet type requires.
        expected: CommandType,
        /// Command type in the base command.
        actual: CommandType,
    },
}

/// Command dispatcher for parsing incoming network packets.
pub struct CommandDispatcher;

impl CommandDispatcher {
    /// Dispatch a raw packet to the appropriate command type.
    ///
    /// The packet should include the channel packet header followed by command data.
    ///
    /// # Errors
    ///
    /// Returns an error if the header or command payload is invalid or incomplete.
    pub fn dispatch(data: &[u8]) -> Result<(ChannelPacketHeader, CommandPayload), DispatchError> {
        let mut cursor = Cursor::new(data);

        // Read the channel packet header
        let header = ChannelPacketHeader::deserialize(&mut cursor)?;

        // Dispatch based on packet type
        let command = Self::dispatch_by_type(header.packet_type, &mut cursor)?;

        Ok((header, command))
    }

    /// Dispatch based on packet type, reading from the provided reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the packet type is unknown or its payload is invalid.
    pub fn dispatch_by_type<R: Read>(
        packet_type: u8,
        reader: &mut R,
    ) -> Result<CommandPayload, DispatchError> {
        match packet_type {
            channel_packet_type::COMMAND_WORK => {
                let base = deserialize_command(reader)?;
                let mut cmd = WorkCommand {
                    base,
                    ..Default::default()
                };
                cmd.deserialize_fields(reader)?;
                Ok(CommandPayload::Work(cmd))
            }
            channel_packet_type::COMMAND_POWER => {
                let base = deserialize_command(reader)?;
                let mut cmd = PowerCommand {
                    base,
                    ..Default::default()
                };
                cmd.deserialize_fields(reader)?;
                Ok(CommandPayload::Power(cmd))
            }
            channel_packet_type::COMMAND_POWER_INPUT => {
                let base = deserialize_command(reader)?;
                let mut cmd = PowerInputCommand {
                    base,
                    ..Default::default()
                };
                cmd.deserialize_fields(reader)?;
                Ok(CommandPayload::PowerInput(cmd))
            }
            channel_packet_type::COMMAND_BUILDING => {
                let base = deserialize_command(reader)?;
                let mut cmd = BuildingCommand {
                    base,
                    ..Default::default()
                };
                cmd.deserialize_fields(reader)?;
                Ok(CommandPayload::Building(cmd))
            }
            channel_packet_type::COMMAND_GAME => {
                let base = deserialize_command(reader)?;
                let mut cmd = GameCommand {
                    base,
                    ..Default::default()
                };
                cmd.deserialize_fields(reader)?;
                Ok(CommandPayload::Game(cmd))
            }
            // TODO: Add other command types
            _ => Err(DispatchError::UnknownPacketType(packet_type)),
        }
    }

    /// Get the command type from a packet type byte.
    #[must_use]
    pub fn packet_type_to_command_type(packet_type: u8) -> Option<CommandType> {
        match packet_type {
            channel_packet_type::COMMAND_WORK => Some(CommandType::Work),
            channel_packet_type::COMMAND_POWER => Some(CommandType::Power),
            channel_packet_type::COMMAND_POWER_INPUT => Some(CommandType::PowerInput),
            channel_packet_type::COMMAND_BUILDING => Some(CommandType::Building),
            channel_packet_type::COMMAND_GAME => Some(CommandType::Game),
            channel_packet_type::COMMAND_TRIGGER => Some(CommandType::Trigger),
            channel_packet_type::COMMAND_GENERAL_EVENT => Some(CommandType::GeneralEvent),
            _ => None,
        }
    }

    /// Get the packet type byte for a command type.
    #[must_use]
    pub fn command_type_to_packet_type(cmd_type: CommandType) -> u8 {
        match cmd_type {
            CommandType::Work => channel_packet_type::COMMAND_WORK,
            CommandType::Power => channel_packet_type::COMMAND_POWER,
            CommandType::PowerInput => channel_packet_type::COMMAND_POWER_INPUT,
            CommandType::Building => channel_packet_type::COMMAND_BUILDING,
            CommandType::Game => channel_packet_type::COMMAND_GAME,
            CommandType::Trigger => channel_packet_type::COMMAND_TRIGGER,
            CommandType::GeneralEvent => channel_packet_type::COMMAND_GENERAL_EVENT,
        }
    }
}
