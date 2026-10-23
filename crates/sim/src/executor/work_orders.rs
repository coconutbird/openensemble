//! Work orders Repair, Unpack, Transport, `PointBlankAttack` and `PlayBlockingAnimation`.
//!
//! The cases of `BSquad_executeOrder` @ `0x140399780` give each order its squad action. The
//! order target follows `BSquad_determineTarget` @ `0x1403A2F40`: the command target entity
//! and the command position (the terrain point, else the last waypoint).

use super::CommandExecutor;
use crate::EntityId;
use crate::commands::WorkCommand;
use crate::world::World;
use glam::Vec3;

impl CommandExecutor<'_> {
    /// Order 4: `BSquadActionRepair` on each recipient.
    pub(super) fn execute_repair(&self, world: &mut World, cmd: &WorkCommand) {
        let (Some(database), Ok(player_id)) = (self.database, u8::try_from(cmd.base.player_id))
        else {
            return;
        };
        for &squad_id in &cmd.base.recipients {
            let _accepted = world.issue_squad_repair_order(player_id, squad_id, database);
        }
    }

    /// Order 17: a blocking animation of the order animation type (`BSimOrder+88`, the
    /// command squad-mode field).
    pub(super) fn execute_play_blocking_animation(world: &mut World, cmd: &WorkCommand) {
        let (Ok(player_id), Ok(animation_type)) = (
            u8::try_from(cmd.base.player_id),
            u8::try_from(cmd.squad_mode),
        ) else {
            return;
        };
        for &squad_id in &cmd.base.recipients {
            let _accepted =
                world.issue_play_blocking_animation_order(player_id, squad_id, animation_type);
        }
    }

    /// Order 20: go to the command position, then unpack with the command ability.
    pub(super) fn execute_unpack(&self, world: &mut World, cmd: &WorkCommand) {
        let (Ok(player_id), Ok(ability_id)) = (
            u8::try_from(cmd.base.player_id),
            self.work_ability_id(cmd.ability_id),
        ) else {
            return;
        };
        let target = command_position(world, cmd);
        for &squad_id in &cmd.base.recipients {
            let _accepted =
                world.issue_unpack_order(player_id, squad_id, target, ability_id, cmd.angle);
        }
    }

    /// Order 23: a controllable transport to the command position.
    pub(super) fn execute_transport(world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        let dropoff = command_position(world, cmd);
        for &squad_id in &cmd.base.recipients {
            let _accepted = world.issue_transport_order(player_id, squad_id, dropoff);
        }
    }

    /// Order 30: a command-priority `PointBlankAttack` opportunity for the members.
    pub(super) fn execute_point_blank_attack(world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        let target_id = (!cmd.unit_id.is_invalid()).then_some(cmd.unit_id);
        let target_position = command_position(world, cmd);
        for &squad_id in &cmd.base.recipients {
            let _accepted = world.issue_point_blank_attack_order(
                player_id,
                squad_id,
                target_id,
                target_position,
            );
        }
    }
}

/// The order position: the command terrain point, else the last waypoint, else the position of
/// the command target entity.
fn command_position(world: &World, cmd: &WorkCommand) -> Option<Vec3> {
    cmd.terrain_point
        .or_else(|| cmd.base.waypoints.last().copied())
        .or_else(|| target_position(world, cmd.unit_id))
}

fn target_position(world: &World, target_id: EntityId) -> Option<Vec3> {
    if target_id.is_invalid() {
        return None;
    }
    world
        .squad_move_entity_target(target_id)
        .map(|(_, position)| position)
}
