//! Network packet types matching vanilla Halo Wars wire format.
//!
//! `BChannelPacket` is the base for all game commands sent over the network.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Read, Write};
use thiserror::Error;

/// Failure while the code reads or writes a channel packet header.
#[derive(Error, Debug)]
pub enum PacketError {
    /// The reader or writer failed.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    /// The packet type byte has an unknown value.
    #[error("Invalid packet type: {0}")]
    InvalidType(u8),
}

/// Channel packet header matching vanilla `BChannelPacket`.
///
/// Wire format (6 bytes):
/// - mTimeOffset: u16 (offset from base time)
/// - mPacketID: u16 (sequence number)
/// - mType: u8 (packet type)
/// - mChannel: u8 (channel ID)
#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelPacketHeader {
    /// Time offset from base time.
    pub time_offset: u16,
    /// Packet sequence ID.
    pub packet_id: u16,
    /// Packet type (command type + offset).
    pub packet_type: u8,
    /// Channel ID.
    pub channel: u8,
}

impl ChannelPacketHeader {
    /// Size of the header in bytes.
    pub const SIZE: usize = 6;

    /// Serialize the header to the wire format.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer cannot accept all header bytes.
    pub fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), PacketError> {
        writer.write_u16::<LittleEndian>(self.time_offset)?;
        writer.write_u16::<LittleEndian>(self.packet_id)?;
        writer.write_u8(self.packet_type)?;
        writer.write_u8(self.channel)?;
        Ok(())
    }

    /// Deserialize the header from the wire format.
    ///
    /// # Errors
    ///
    /// Returns an error if the reader does not contain a complete header.
    pub fn deserialize<R: Read>(reader: &mut R) -> Result<Self, PacketError> {
        Ok(Self {
            time_offset: reader.read_u16::<LittleEndian>()?,
            packet_id: reader.read_u16::<LittleEndian>()?,
            packet_type: reader.read_u8()?,
            channel: reader.read_u8()?,
        })
    }

    /// Reads the packet type (byte 4) from raw bytes without a full parse.
    ///
    /// Returns `None` if `data` is too short.
    #[must_use]
    pub fn peek_type(data: &[u8]) -> Option<u8> {
        data.get(4).copied()
    }

    /// Reads the channel ID (byte 5) from raw bytes without a full parse.
    ///
    /// Returns `None` if `data` is too short.
    #[must_use]
    pub fn peek_channel(data: &[u8]) -> Option<u8> {
        data.get(5).copied()
    }

    /// Reads the packet sequence ID (bytes 2 and 3) from raw bytes without a full parse.
    ///
    /// Returns `None` if `data` is too short.
    #[must_use]
    pub fn peek_packet_id(data: &[u8]) -> Option<u16> {
        if data.len() >= 4 {
            Some(u16::from_le_bytes([data[2], data[3]]))
        } else {
            None
        }
    }

    /// Reads the time offset (bytes 0 and 1) from raw bytes without a full parse.
    ///
    /// Returns `None` if `data` is too short.
    #[must_use]
    pub fn peek_time_offset(data: &[u8]) -> Option<u16> {
        if data.len() >= 2 {
            Some(u16::from_le_bytes([data[0], data[1]]))
        } else {
            None
        }
    }
}

/// Channel packet types from vanilla.
/// Command packets start at `cCommandPacketsStart`.
pub mod channel_packet_type {
    // From mpPackets.h - BChannelPacketType
    /// Game state sync data.
    pub const SYNC_PACKET: u8 = 0;
    /// A client asks the host for the game settings.
    pub const REQUEST_SETTINGS_PACKET: u8 = 1;
    /// The host locks the game settings.
    pub const LOCK_SETTINGS_PACKET: u8 = 2;
    /// The host tells the clients that the settings are locked.
    pub const SETTINGS_LOCKED_PACKET: u8 = 3;
    /// Pre-launch data from the host.
    pub const PRE_LAUNCH_DATA_HOST_DATA_PACKET: u8 = 4;
    /// Pre-launch data from a client.
    pub const PRE_LAUNCH_DATA_CLIENT_DATA_PACKET: u8 = 5;
    /// The host starts the game.
    pub const START_GAME_PACKET: u8 = 6;
    /// Launch progress update.
    pub const LAUNCH_UPDATE_PACKET: u8 = 7;
    /// A peer asks to abort the launch.
    pub const LAUNCH_ABORT_REQUEST_PACKET: u8 = 8;
    /// The launch is aborted.
    pub const LAUNCH_ABORT_PACKET: u8 = 9;
    /// A peer is ready to launch.
    pub const LAUNCH_READY_PACKET: u8 = 10;
    /// Start of a vote.
    pub const START_VOTE: u8 = 11;
    /// Abort of a vote.
    pub const ABORT_VOTE: u8 = 12;
    /// Result of a vote.
    pub const VOTE_RESULT: u8 = 13;
    /// First game settings from the host.
    pub const INITIAL_SETTINGS_PACKET: u8 = 14;
    /// Changed game settings.
    pub const SETTINGS_PACKET: u8 = 15;
    /// Last game settings before the launch.
    pub const FINAL_SETTINGS_PACKET: u8 = 16;
    /// Number of multiplayer channel packet types.
    pub const NUMBER_OF_MP_CHANNEL_PACKETS: u8 = 17;

    // From packets.h - command packets start here
    /// First packet type for commands (`cCommandPacketsStart`).
    pub const COMMAND_PACKETS_START: u8 = NUMBER_OF_MP_CHANNEL_PACKETS;

    // Command types (offset from COMMAND_PACKETS_START)
    /// Work command packet (`CommandType::Work`).
    pub const COMMAND_WORK: u8 = COMMAND_PACKETS_START;
    /// Power command packet (`CommandType::Power`).
    pub const COMMAND_POWER: u8 = COMMAND_PACKETS_START + 1;
    /// Power input command packet (`CommandType::PowerInput`).
    pub const COMMAND_POWER_INPUT: u8 = COMMAND_PACKETS_START + 2;
    /// Building command packet (`CommandType::Building`).
    pub const COMMAND_BUILDING: u8 = COMMAND_PACKETS_START + 3;
    /// Game command packet (`CommandType::Game`).
    pub const COMMAND_GAME: u8 = COMMAND_PACKETS_START + 4;
    /// Trigger command packet (`CommandType::Trigger`).
    pub const COMMAND_TRIGGER: u8 = COMMAND_PACKETS_START + 5;
    /// General event command packet (`CommandType::GeneralEvent`).
    pub const COMMAND_GENERAL_EVENT: u8 = COMMAND_PACKETS_START + 6;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_roundtrip() {
        let header = ChannelPacketHeader {
            time_offset: 1234,
            packet_id: 5678,
            packet_type: channel_packet_type::COMMAND_WORK,
            channel: 1,
        };

        let mut buffer = Vec::new();
        header.serialize(&mut buffer).unwrap();
        assert_eq!(buffer.len(), ChannelPacketHeader::SIZE);

        let mut cursor = std::io::Cursor::new(&buffer);
        let decoded = ChannelPacketHeader::deserialize(&mut cursor).unwrap();

        assert_eq!(decoded.time_offset, header.time_offset);
        assert_eq!(decoded.packet_id, header.packet_id);
        assert_eq!(decoded.packet_type, header.packet_type);
        assert_eq!(decoded.channel, header.channel);
    }
}
