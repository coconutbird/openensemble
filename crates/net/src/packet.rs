//! Network packet types.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor, Read};

/// Packet type identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    /// Connection request.
    Connect = 0,
    /// Connection accepted.
    ConnectAck = 1,
    /// Disconnect notification.
    Disconnect = 2,
    /// Ping request.
    Ping = 3,
    /// Ping response.
    Pong = 4,
    /// Game command packet.
    Command = 5,
    /// Time sync packet.
    TimeSync = 6,
    /// Sync checksum packet.
    SyncCheck = 7,
    /// Ready state change.
    Ready = 8,
    /// Game start signal.
    GameStart = 9,
    /// Game pause/resume.
    GamePause = 10,
}

impl PacketType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Connect),
            1 => Some(Self::ConnectAck),
            2 => Some(Self::Disconnect),
            3 => Some(Self::Ping),
            4 => Some(Self::Pong),
            5 => Some(Self::Command),
            6 => Some(Self::TimeSync),
            7 => Some(Self::SyncCheck),
            8 => Some(Self::Ready),
            9 => Some(Self::GameStart),
            10 => Some(Self::GamePause),
            _ => None,
        }
    }
}

/// Network packet header.
#[derive(Debug, Clone)]
pub struct NetPacket {
    /// Packet type.
    pub packet_type: PacketType,
    /// Sequence number.
    pub sequence: u32,
    /// Acknowledgment number.
    pub ack: u32,
    /// Ack bitfield (32 previous packets).
    pub ack_bits: u32,
    /// Payload data.
    pub payload: Vec<u8>,
}

impl NetPacket {
    /// Header size in bytes.
    pub const HEADER_SIZE: usize = 13;

    /// Create a new packet.
    pub fn new(packet_type: PacketType, sequence: u32) -> Self {
        Self {
            packet_type,
            sequence,
            ack: 0,
            ack_bits: 0,
            payload: Vec::new(),
        }
    }

    /// Create a packet with payload.
    pub fn with_payload(packet_type: PacketType, sequence: u32, payload: Vec<u8>) -> Self {
        Self {
            packet_type,
            sequence,
            ack: 0,
            ack_bits: 0,
            payload,
        }
    }

    /// Serialize the packet to bytes.
    pub fn serialize(&self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(Self::HEADER_SIZE + self.payload.len());
        buf.write_u8(self.packet_type as u8)?;
        buf.write_u32::<LittleEndian>(self.sequence)?;
        buf.write_u32::<LittleEndian>(self.ack)?;
        buf.write_u32::<LittleEndian>(self.ack_bits)?;
        buf.extend_from_slice(&self.payload);
        Ok(buf)
    }

    /// Deserialize a packet from bytes.
    pub fn deserialize(data: &[u8]) -> io::Result<Self> {
        if data.len() < Self::HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Packet too short",
            ));
        }

        let mut cursor = Cursor::new(data);
        let packet_type = PacketType::from_u8(cursor.read_u8()?)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid packet type"))?;
        let sequence = cursor.read_u32::<LittleEndian>()?;
        let ack = cursor.read_u32::<LittleEndian>()?;
        let ack_bits = cursor.read_u32::<LittleEndian>()?;

        let mut payload = Vec::new();
        cursor.read_to_end(&mut payload)?;

        Ok(Self {
            packet_type,
            sequence,
            ack,
            ack_bits,
            payload,
        })
    }
}
