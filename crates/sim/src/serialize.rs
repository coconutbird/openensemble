//! Binary serialization matching vanilla Halo Wars wire format.
//!
//! This module provides exact byte-for-byte compatibility with the
//! vanilla game's command serialization.

use crate::EntityId;
use crate::command::{Command, CommandFlags, CommandType, EntityType};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use glam::Vec3;
use std::io::{self, Read, Write};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SerializeError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid command type: {0}")]
    InvalidCommandType(u8),
    #[error("Invalid entity type: {0}")]
    InvalidEntityType(u8),
    #[error("Buffer too small")]
    BufferTooSmall,
}

/// Serialize a command to the vanilla wire format.
///
/// Wire format (after BChannelPacket header):
/// - mType: u8
/// - flags: i16
/// - [payload based on flags]
pub fn serialize_command<W: Write>(cmd: &Command, writer: &mut W) -> Result<(), SerializeError> {
    // Calculate flags
    let mut flags = CommandFlags::default();

    if cmd.id >= 0 && cmd.id < 256 {
        flags.set(CommandFlags::ID_BYTE);
    } else if cmd.id != -1 {
        flags.set(CommandFlags::ID_LONG);
    }

    if cmd.sender_type == EntityType::Player
        && cmd.senders.len() == 1
        && (cmd.senders[0] == -1 || (cmd.senders[0] >= 0 && cmd.senders[0] < 255))
    {
        flags.set(CommandFlags::ONE_SENDER_BYTE);
    }

    if cmd.recipient_type != EntityType::Unit {
        flags.set(CommandFlags::NON_UNIT_RECIPIENT);
    }

    if cmd.waypoints.len() == 1 {
        flags.set(CommandFlags::ONE_WAYPOINT);
    } else if cmd.waypoints.len() > 1 {
        flags.set(CommandFlags::MULTI_WAYPOINTS);
    }

    // Check if any waypoint has non-zero Y
    if cmd.waypoints.iter().any(|w| w.y != 0.0) {
        flags.set(CommandFlags::WAYPOINT_Y);
    }

    // Check if flags beyond first byte are set
    if cmd.flags.len() > 1 && cmd.flags[1..].iter().any(|&b| b != 0) {
        flags.set(CommandFlags::EXTRA_FLAG_DATA);
    }

    if cmd.urgency_count > 0 {
        flags.set(CommandFlags::URGENCY_COUNT);
    }

    // Build payload
    let mut payload = Vec::new();

    // mPlayerID
    let player_id_byte = if cmd.player_id == -1 {
        255u8
    } else {
        cmd.player_id as u8
    };
    payload.write_u8(player_id_byte)?;

    // mID
    if flags.has(CommandFlags::ID_BYTE) {
        payload.write_u8(cmd.id as u8)?;
    } else if flags.has(CommandFlags::ID_LONG) {
        payload.write_i32::<LittleEndian>(cmd.id)?;
    }

    // mSenderType and mSenders
    if flags.has(CommandFlags::ONE_SENDER_BYTE) {
        let sender_byte = if cmd.senders[0] == -1 {
            255u8
        } else {
            cmd.senders[0] as u8
        };
        payload.write_u8(sender_byte)?;
    } else {
        let sender_type_byte = cmd.sender_type as u8;
        payload.write_u8(sender_type_byte)?;
        payload.write_u8(cmd.senders.len() as u8)?;
        for &sender in &cmd.senders {
            payload.write_i32::<LittleEndian>(sender)?;
        }
    }

    // mRecipientType
    if flags.has(CommandFlags::NON_UNIT_RECIPIENT) {
        payload.write_u8(cmd.recipient_type as u8)?;
    }

    // mRecipients (not using cached unit sets in this implementation)
    payload.write_u8(cmd.recipients.len() as u8)?;
    for recipient in &cmd.recipients {
        payload.write_u32::<LittleEndian>(recipient.as_u32())?;
    }

    // mWaypoints
    if flags.has(CommandFlags::MULTI_WAYPOINTS) {
        payload.write_u8(cmd.waypoints.len() as u8)?;
    }
    for waypoint in &cmd.waypoints {
        payload.write_f32::<LittleEndian>(waypoint.x)?;
        if flags.has(CommandFlags::WAYPOINT_Y) {
            payload.write_f32::<LittleEndian>(waypoint.y)?;
        }
        payload.write_f32::<LittleEndian>(waypoint.z)?;
    }

    // mFlags
    let flag_count = cmd.flags.len().max(1) * 8;
    payload.write_u8(flag_count as u8)?;
    if flags.has(CommandFlags::EXTRA_FLAG_DATA) {
        payload.write_all(&cmd.flags)?;
    } else {
        payload.write_u8(*cmd.flags.first().unwrap_or(&0))?;
    }

    // mUrgencyCount
    if flags.has(CommandFlags::URGENCY_COUNT) {
        payload.write_u8(cmd.urgency_count)?;
    }

    // Write final output: type, flags, payload
    writer.write_u8(cmd.command_type as u8)?;
    writer.write_i16::<LittleEndian>(flags.0 as i16)?;
    writer.write_all(&payload)?;

    Ok(())
}

/// Deserialize a command from the vanilla wire format.
pub fn deserialize_command<R: Read>(reader: &mut R) -> Result<Command, SerializeError> {
    let mut cmd = Command::default();

    // Read type and flags
    let type_byte = reader.read_u8()?;
    cmd.command_type =
        CommandType::from_u8(type_byte).ok_or(SerializeError::InvalidCommandType(type_byte))?;

    let flags = CommandFlags(reader.read_i16::<LittleEndian>()? as u16);

    // mPlayerID
    let player_id_byte = reader.read_u8()?;
    cmd.player_id = if player_id_byte == 255 {
        -1
    } else {
        player_id_byte as i32
    };

    // mID
    if flags.has(CommandFlags::ID_BYTE) {
        cmd.id = reader.read_u8()? as i32;
    } else if flags.has(CommandFlags::ID_LONG) {
        cmd.id = reader.read_i32::<LittleEndian>()?;
    } else {
        cmd.id = -1;
    }

    // mSenderType and mSenders
    if flags.has(CommandFlags::ONE_SENDER_BYTE) {
        let sender_byte = reader.read_u8()?;
        cmd.sender_type = EntityType::Player;
        cmd.senders = vec![if sender_byte == 255 {
            -1
        } else {
            sender_byte as i32
        }];
    } else {
        let sender_type_byte = reader.read_u8()?;
        cmd.sender_type = EntityType::from_u8(sender_type_byte)
            .ok_or(SerializeError::InvalidEntityType(sender_type_byte))?;
        let sender_count = reader.read_u8()? as usize;
        cmd.senders = Vec::with_capacity(sender_count);
        for _ in 0..sender_count {
            cmd.senders.push(reader.read_i32::<LittleEndian>()?);
        }
    }

    // mRecipientType
    if flags.has(CommandFlags::NON_UNIT_RECIPIENT) {
        let recipient_type_byte = reader.read_u8()?;
        cmd.recipient_type = EntityType::from_u8(recipient_type_byte)
            .ok_or(SerializeError::InvalidEntityType(recipient_type_byte))?;
    } else {
        cmd.recipient_type = EntityType::Unit;
    }

    // mRecipients (cached unit sets not implemented)
    let recipient_count = reader.read_u8()? as usize;
    cmd.recipients = Vec::with_capacity(recipient_count);
    for _ in 0..recipient_count {
        cmd.recipients
            .push(EntityId::from_u32(reader.read_u32::<LittleEndian>()?));
    }

    // mWaypoints
    let waypoint_count = if flags.has(CommandFlags::ONE_WAYPOINT) {
        1
    } else if flags.has(CommandFlags::MULTI_WAYPOINTS) {
        reader.read_u8()? as usize
    } else {
        0
    };
    cmd.waypoints = Vec::with_capacity(waypoint_count);
    for _ in 0..waypoint_count {
        let x = reader.read_f32::<LittleEndian>()?;
        let y = if flags.has(CommandFlags::WAYPOINT_Y) {
            reader.read_f32::<LittleEndian>()?
        } else {
            0.0
        };
        let z = reader.read_f32::<LittleEndian>()?;
        cmd.waypoints.push(Vec3::new(x, y, z));
    }

    // mFlags
    let flag_bit_count = reader.read_u8()? as usize;
    let flag_byte_count = flag_bit_count.div_ceil(8);
    if flags.has(CommandFlags::EXTRA_FLAG_DATA) {
        cmd.flags = vec![0u8; flag_byte_count];
        reader.read_exact(&mut cmd.flags)?;
    } else {
        cmd.flags = vec![reader.read_u8()?];
    }

    // mUrgencyCount
    if flags.has(CommandFlags::URGENCY_COUNT) {
        cmd.urgency_count = reader.read_u8()?;
    } else {
        cmd.urgency_count = 0;
    }

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_simple_command() {
        let cmd = Command {
            player_id: 0,
            id: 42,
            sender_type: EntityType::Player,
            senders: vec![0],
            recipient_type: EntityType::Unit,
            recipients: vec![EntityId::from_u32(0x10000001)],
            waypoints: vec![Vec3::new(100.0, 0.0, 200.0)],
            flags: vec![0b00000001],
            command_type: CommandType::Work,
            urgency_count: 0,
        };

        let mut buffer = Vec::new();
        serialize_command(&cmd, &mut buffer).unwrap();

        let mut cursor = std::io::Cursor::new(&buffer);
        let decoded = deserialize_command(&mut cursor).unwrap();

        assert_eq!(decoded.player_id, cmd.player_id);
        assert_eq!(decoded.id, cmd.id);
        assert_eq!(decoded.sender_type as u8, cmd.sender_type as u8);
        assert_eq!(decoded.senders, cmd.senders);
        assert_eq!(decoded.recipient_type as u8, cmd.recipient_type as u8);
        assert_eq!(decoded.recipients.len(), cmd.recipients.len());
        assert_eq!(decoded.waypoints.len(), cmd.waypoints.len());
        assert_eq!(decoded.command_type as u8, cmd.command_type as u8);
    }

    #[test]
    fn test_roundtrip_complex_command() {
        let cmd = Command {
            player_id: 2,
            id: 1000, // > 255, should use ID_LONG
            sender_type: EntityType::Squad,
            senders: vec![100, 101, 102],
            recipient_type: EntityType::Squad,
            recipients: vec![
                EntityId::from_u32(0x20000001),
                EntityId::from_u32(0x20000002),
            ],
            waypoints: vec![
                Vec3::new(50.0, 10.0, 100.0), // has Y component
                Vec3::new(75.0, 0.0, 150.0),
            ],
            flags: vec![0xFF, 0x01], // extra flag data
            command_type: CommandType::Power,
            urgency_count: 3,
        };

        let mut buffer = Vec::new();
        serialize_command(&cmd, &mut buffer).unwrap();

        let mut cursor = std::io::Cursor::new(&buffer);
        let decoded = deserialize_command(&mut cursor).unwrap();

        assert_eq!(decoded.player_id, cmd.player_id);
        assert_eq!(decoded.id, cmd.id);
        assert_eq!(decoded.senders.len(), cmd.senders.len());
        assert_eq!(decoded.recipients.len(), cmd.recipients.len());
        assert_eq!(decoded.waypoints.len(), cmd.waypoints.len());
        assert_eq!(decoded.urgency_count, cmd.urgency_count);
    }
}
