//! Work command matching vanilla `BWorkCommand`.

use crate::EntityId;
use crate::command::{Command, CommandType, EntityType};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use glam::Vec3;
use std::io::{self, Read, Write};

/// Work command flags for serialization.
mod flags {
    pub const UNIT: u16 = 1;
    pub const PROTO: u16 = 2;
    pub const RANGE: u16 = 4;
    pub const ANGLE: u16 = 8;
    pub const TERRAIN_POINT: u16 = 16;
    pub const SPEED: u16 = 32;
    pub const SQUAD_MODE: u16 = 64;
    pub const ABILITY: u16 = 128;
    pub const HIT_ZONE: u16 = 256;
    pub const OVERRIDE_POSITION: u16 = 512;
    pub const OVERRIDE_RANGE: u16 = 1024;
}

/// Work command flag bits in the base command flag set.
pub mod command_flags {
    /// Attack-move flag (extends the base command flags).
    pub const ATTACK_MOVE: usize = 1;
}

/// Work command matching vanilla `BWorkCommand`.
#[derive(Debug, Clone)]
pub struct WorkCommand {
    /// Base command data.
    pub base: Command,
    /// Target unit ID.
    pub unit_id: EntityId,
    /// Hit zone index (-1 for none).
    pub hit_zone_index: i32,
    /// Build proto ID (-1 for none).
    pub build_proto_id: i32,
    /// Attack range.
    pub range: f32,
    /// Target terrain position.
    pub terrain_point: Option<Vec3>,
    /// Speed multiplier.
    pub speed_multiplier: f32,
    /// Squad mode (-1 for none).
    pub squad_mode: i32,
    /// Ability ID (-1 for none).
    pub ability_id: i32,
    /// Facing angle.
    pub angle: f32,
    /// Override squad plotter position.
    pub override_position: bool,
    /// Override squad plotter range.
    pub override_range: bool,
}

impl Default for WorkCommand {
    fn default() -> Self {
        Self {
            base: Command::default(),
            unit_id: EntityId::INVALID,
            hit_zone_index: -1,
            build_proto_id: -1,
            range: 0.0,
            terrain_point: None,
            speed_multiplier: 1.0,
            squad_mode: -1,
            ability_id: -1,
            angle: 0.0,
            override_position: false,
            override_range: false,
        }
    }
}

impl WorkCommand {
    /// Build a move command for player-owned squads.
    #[must_use]
    pub fn move_squads(player_id: i32, recipients: Vec<EntityId>, target: Vec3) -> Self {
        Self {
            base: Command {
                player_id,
                id: crate::order::OrderType::Move as i32,
                sender_type: EntityType::Player,
                senders: vec![player_id],
                recipient_type: EntityType::Squad,
                recipients,
                command_type: CommandType::Work,
                ..Command::default()
            },
            terrain_point: Some(target),
            ..Self::default()
        }
    }

    /// Build an attack command for player-owned squads.
    #[must_use]
    pub fn attack_squads(player_id: i32, recipients: Vec<EntityId>, target: EntityId) -> Self {
        Self {
            base: Command {
                player_id,
                id: crate::order::OrderType::Attack as i32,
                sender_type: EntityType::Player,
                senders: vec![player_id],
                recipient_type: EntityType::Squad,
                recipients,
                command_type: CommandType::Work,
                ..Command::default()
            },
            unit_id: target,
            ..Self::default()
        }
    }

    /// Build a command-ability Jump to a world location.
    #[must_use]
    pub fn jump_squads(
        player_id: i32,
        recipients: Vec<EntityId>,
        target: Vec3,
        ability_id: Option<u8>,
    ) -> Self {
        jump_command(
            player_id,
            recipients,
            crate::order::OrderType::Jump,
            EntityId::INVALID,
            Some(target),
            ability_id,
        )
    }

    /// Build a `JumpGather` command retaining its resource target for landing.
    #[must_use]
    pub fn jump_gather_squads(
        player_id: i32,
        recipients: Vec<EntityId>,
        target: EntityId,
        ability_id: Option<u8>,
    ) -> Self {
        jump_command(
            player_id,
            recipients,
            crate::order::OrderType::JumpGather,
            target,
            None,
            ability_id,
        )
    }

    /// Build a `JumpGarrison` command retaining its container target for landing.
    #[must_use]
    pub fn jump_garrison_squads(
        player_id: i32,
        recipients: Vec<EntityId>,
        target: EntityId,
        ability_id: Option<u8>,
    ) -> Self {
        jump_command(
            player_id,
            recipients,
            crate::order::OrderType::JumpGarrison,
            target,
            None,
            ability_id,
        )
    }

    /// Build a `JumpAttack` command retaining its attack target for landing placement.
    #[must_use]
    pub fn jump_attack_squads(
        player_id: i32,
        recipients: Vec<EntityId>,
        target: EntityId,
        ability_id: Option<u8>,
    ) -> Self {
        jump_command(
            player_id,
            recipients,
            crate::order::OrderType::JumpAttack,
            target,
            None,
            ability_id,
        )
    }

    /// Build a gather command for player-owned squads.
    #[must_use]
    pub fn gather_squads(player_id: i32, recipients: Vec<EntityId>, target: EntityId) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Gather),
            unit_id: target,
            ..Self::default()
        }
    }

    /// Build a capture command for player-owned squads.
    #[must_use]
    pub fn capture_squads(player_id: i32, recipients: Vec<EntityId>, target: EntityId) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Capture),
            unit_id: target,
            ..Self::default()
        }
    }

    /// Build a targeted `RepairOther` command for player-owned squads.
    #[must_use]
    pub fn repair_squads(
        player_id: i32,
        recipients: Vec<EntityId>,
        target: EntityId,
        ability_id: Option<u8>,
    ) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::RepairOther),
            unit_id: target,
            ability_id: ability_id.map_or(-1, i32::from),
            ..Self::default()
        }
    }

    /// Build a targeted Detonate work command for player-owned squads.
    #[must_use]
    pub fn detonate_squads(
        player_id: i32,
        recipients: Vec<EntityId>,
        target: EntityId,
        ability_id: u8,
    ) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Detonate),
            unit_id: target,
            ability_id: i32::from(ability_id),
            ..Self::default()
        }
    }

    /// Build a Mines work command targeting a world location.
    #[must_use]
    pub fn place_mines_at(
        player_id: i32,
        recipients: Vec<EntityId>,
        target: Vec3,
        ability_id: u8,
    ) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Mines),
            terrain_point: Some(target),
            ability_id: i32::from(ability_id),
            ..Self::default()
        }
    }

    /// Build a Cloak work command for player-owned squads.
    #[must_use]
    pub fn cloak_squads(player_id: i32, recipients: Vec<EntityId>, ability_id: Option<u8>) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Cloak),
            ability_id: ability_id.map_or(-1, i32::from),
            ..Self::default()
        }
    }

    /// Build a persistent Join command for player-owned squads.
    #[must_use]
    pub fn join_squads(player_id: i32, recipients: Vec<EntityId>, target: EntityId) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Join),
            unit_id: target,
            ..Self::default()
        }
    }

    /// Build a garrison command for player-owned squads.
    #[must_use]
    pub fn garrison_squads(player_id: i32, recipients: Vec<EntityId>, target: EntityId) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Garrison),
            unit_id: target,
            ..Self::default()
        }
    }

    /// Build an ungarrison command for player-owned passenger squads.
    #[must_use]
    pub fn ungarrison_squads(
        player_id: i32,
        recipients: Vec<EntityId>,
        rally_point: Option<Vec3>,
    ) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Ungarrison),
            terrain_point: rally_point,
            ..Self::default()
        }
    }

    /// Build a hitch command for player-owned towing squads.
    #[must_use]
    pub fn hitch_squads(player_id: i32, recipients: Vec<EntityId>, target: EntityId) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Hitch),
            unit_id: target,
            ..Self::default()
        }
    }

    /// Build an unhitch command for player-owned towing squads.
    #[must_use]
    pub fn unhitch_squads(player_id: i32, recipients: Vec<EntityId>, target: EntityId) -> Self {
        Self {
            base: squad_work_command(player_id, recipients, crate::order::OrderType::Unhitch),
            unit_id: target,
            ..Self::default()
        }
    }

    /// Serialize the work-specific fields (after base command).
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the writer fails or a field does not fit the wire format.
    pub fn serialize_fields<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut work_flags: u16 = 0;

        if !self.unit_id.is_invalid() {
            work_flags |= flags::UNIT;
        }
        if self.build_proto_id != -1 {
            work_flags |= flags::PROTO;
        }
        if self.range != 0.0 {
            work_flags |= flags::RANGE;
        }
        if self.terrain_point.is_some() {
            work_flags |= flags::TERRAIN_POINT;
        }
        if (self.speed_multiplier - 1.0).abs() > f32::EPSILON {
            work_flags |= flags::SPEED;
        }
        if self.squad_mode != -1 {
            work_flags |= flags::SQUAD_MODE;
        }
        if self.ability_id != -1 {
            work_flags |= flags::ABILITY;
        }
        if self.angle != 0.0 {
            work_flags |= flags::ANGLE;
        }
        if self.hit_zone_index != -1 {
            work_flags |= flags::HIT_ZONE;
        }
        if self.override_position {
            work_flags |= flags::OVERRIDE_POSITION;
        }
        if self.override_range {
            work_flags |= flags::OVERRIDE_RANGE;
        }

        writer.write_u16::<LittleEndian>(work_flags)?;

        if work_flags & flags::UNIT != 0 {
            writer.write_u32::<LittleEndian>(self.unit_id.as_u32())?;
        }
        if work_flags & flags::PROTO != 0 {
            writer.write_i32::<LittleEndian>(self.build_proto_id)?;
        }
        if work_flags & flags::RANGE != 0 {
            writer.write_f32::<LittleEndian>(self.range)?;
        }
        if let Some(pt) = self.terrain_point
            && work_flags & flags::TERRAIN_POINT != 0
        {
            writer.write_f32::<LittleEndian>(pt.x)?;
            writer.write_f32::<LittleEndian>(pt.y)?;
            writer.write_f32::<LittleEndian>(pt.z)?;
        }
        if work_flags & flags::SPEED != 0 {
            writer.write_f32::<LittleEndian>(self.speed_multiplier)?;
        }
        if work_flags & flags::SQUAD_MODE != 0 {
            writer.write_u8(u8::try_from(self.squad_mode).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "squad mode does not fit in u8")
            })?)?;
        }
        if work_flags & flags::ABILITY != 0 {
            writer.write_u8(u8::try_from(self.ability_id).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "ability ID does not fit in u8")
            })?)?;
        }
        if work_flags & flags::ANGLE != 0 {
            writer.write_f32::<LittleEndian>(self.angle)?;
        }
        if work_flags & flags::HIT_ZONE != 0 {
            writer.write_u8(u8::try_from(self.hit_zone_index).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "hit zone index does not fit in u8",
                )
            })?)?;
        }

        Ok(())
    }

    /// Deserialize the work-specific fields (after base command).
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the reader does not contain a complete command.
    pub fn deserialize_fields<R: Read>(&mut self, reader: &mut R) -> io::Result<()> {
        let work_flags = reader.read_u16::<LittleEndian>()?;

        if work_flags & flags::UNIT != 0 {
            let id = reader.read_u32::<LittleEndian>()?;
            self.unit_id = EntityId::from_u32(id);
        } else {
            self.unit_id = EntityId::INVALID;
        }

        if work_flags & flags::PROTO != 0 {
            self.build_proto_id = reader.read_i32::<LittleEndian>()?;
        } else {
            self.build_proto_id = -1;
        }

        if work_flags & flags::RANGE != 0 {
            self.range = reader.read_f32::<LittleEndian>()?;
        } else {
            self.range = 0.0;
        }

        if work_flags & flags::TERRAIN_POINT != 0 {
            self.terrain_point = Some(Vec3::new(
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
            ));
        } else {
            self.terrain_point = None;
        }

        if work_flags & flags::SPEED != 0 {
            self.speed_multiplier = reader.read_f32::<LittleEndian>()?;
        } else {
            self.speed_multiplier = 1.0;
        }

        if work_flags & flags::SQUAD_MODE != 0 {
            self.squad_mode = i32::from(reader.read_u8()?);
        } else {
            self.squad_mode = -1;
        }

        if work_flags & flags::ABILITY != 0 {
            self.ability_id = i32::from(reader.read_u8()?);
        } else {
            self.ability_id = -1;
        }

        if work_flags & flags::ANGLE != 0 {
            self.angle = reader.read_f32::<LittleEndian>()?;
        } else {
            self.angle = 0.0;
        }

        if work_flags & flags::HIT_ZONE != 0 {
            self.hit_zone_index = i32::from(reader.read_u8()?);
        } else {
            self.hit_zone_index = -1;
        }

        self.override_position = work_flags & flags::OVERRIDE_POSITION != 0;
        self.override_range = work_flags & flags::OVERRIDE_RANGE != 0;

        Ok(())
    }
}

fn jump_command(
    player_id: i32,
    recipients: Vec<EntityId>,
    order_type: crate::order::OrderType,
    target: EntityId,
    terrain_point: Option<Vec3>,
    ability_id: Option<u8>,
) -> WorkCommand {
    WorkCommand {
        base: squad_work_command(player_id, recipients, order_type),
        unit_id: target,
        terrain_point,
        ability_id: ability_id.map_or(-1, i32::from),
        ..WorkCommand::default()
    }
}

fn squad_work_command(
    player_id: i32,
    recipients: Vec<EntityId>,
    order_type: crate::order::OrderType,
) -> Command {
    Command {
        player_id,
        id: order_type as i32,
        sender_type: EntityType::Player,
        senders: vec![player_id],
        recipient_type: EntityType::Squad,
        recipients,
        command_type: CommandType::Work,
        ..Command::default()
    }
}
