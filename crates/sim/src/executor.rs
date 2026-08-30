//! Command executor - processes commands and applies them to the world.
//!
//! This is where commands from the network get wired up to actual game logic.

use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{
    BuildingCommand, BuildingCommandType, GameCommand, GameCommandType, PowerCommand,
    PowerCommandType, PowerInputCommand, PowerInputCommandType, PowerUserId, WorkCommand,
    power_command_flags, power_input_command_flags, work_command_flags,
};
use crate::entities::{RecoveryType, SquadMode, TrainingKind};
use crate::gameplay::{GameplayCatalog, resolve_database_ability};
use crate::order::OrderType;
use crate::player::PowerGrant;
use crate::spawn::{MAX_SPAWN_BATCH, spawn_object_at, spawn_squads_at};
use crate::world::{NativePowerInvocation, World};
use pipeline::database::hw1::Database;

mod detonate;
mod jump;
mod mines;

#[cfg(test)]
use glam::Vec3;

/// Command executor that processes commands against the world.
#[derive(Debug, Default)]
pub struct CommandExecutor<'database> {
    database: Option<&'database Database>,
    gameplay: Option<&'database GameplayCatalog>,
}

#[derive(Debug, Clone, Copy)]
struct InvalidWorkAbility;

impl<'database> CommandExecutor<'database> {
    /// Create a new command executor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an executor that can resolve database-backed create commands.
    #[must_use]
    pub const fn with_database(database: &'database Database) -> Self {
        Self {
            database: Some(database),
            gameplay: None,
        }
    }

    /// Create an executor with the active scenario database and tactic catalog.
    #[must_use]
    pub const fn with_database_and_gameplay(
        database: &'database Database,
        gameplay: &'database GameplayCatalog,
    ) -> Self {
        Self {
            database: Some(database),
            gameplay: Some(gameplay),
        }
    }

    /// Execute a list of commands against the world.
    pub fn execute_all(&self, world: &mut World, commands: &[CommandEntry]) {
        for entry in commands {
            self.execute(world, entry);
        }
    }

    /// Execute a single command against the world.
    pub fn execute(&self, world: &mut World, entry: &CommandEntry) {
        match &entry.command {
            QueuedCommand::Work(cmd) => self.execute_work(world, cmd),
            QueuedCommand::Power(cmd) => self.execute_power(world, cmd),
            QueuedCommand::PowerInput(cmd) => self.execute_power_input(world, cmd),
            QueuedCommand::Building(cmd) => self.execute_building(world, cmd),
            QueuedCommand::Game(cmd) => self.execute_game(world, cmd),
        }
    }

    /// Execute a work command.
    fn execute_work(&self, world: &mut World, cmd: &WorkCommand) {
        let order_type = OrderType::from_i32(cmd.base.id);

        match order_type {
            Some(OrderType::Move) => Self::execute_move(world, cmd),
            Some(OrderType::Attack) => self.execute_attack(world, cmd),
            Some(OrderType::Gather) => self.execute_gather(world, cmd),
            Some(OrderType::Capture) => self.execute_capture(world, cmd),
            Some(OrderType::RepairOther) => self.execute_repair_other(world, cmd),
            Some(OrderType::Cloak) => self.execute_cloak(world, cmd),
            Some(OrderType::Detonate) => self.execute_detonate(world, cmd),
            Some(OrderType::Join) => self.execute_join(world, cmd),
            Some(OrderType::Garrison) => Self::execute_garrison(world, cmd),
            Some(OrderType::Ungarrison) => Self::execute_ungarrison(world, cmd),
            Some(OrderType::Hitch) => Self::execute_hitch(world, cmd),
            Some(OrderType::Unhitch) => Self::execute_unhitch(world, cmd),
            Some(OrderType::Mines) => self.execute_mines(world, cmd),
            Some(OrderType::Jump) => self.execute_jump(world, cmd, crate::JumpOrderType::Jump),
            Some(OrderType::JumpGather) => {
                self.execute_jump(world, cmd, crate::JumpOrderType::Gather);
            }
            Some(OrderType::JumpGarrison) => {
                self.execute_jump(world, cmd, crate::JumpOrderType::Garrison);
            }
            Some(OrderType::JumpAttack) => {
                self.execute_jump(world, cmd, crate::JumpOrderType::Attack);
            }
            _ => {}
        }
    }

    fn execute_gather(&self, world: &mut World, cmd: &WorkCommand) {
        let (Some(gameplay), Ok(player_id)) = (self.gameplay, u8::try_from(cmd.base.player_id))
        else {
            return;
        };
        if cmd.unit_id.is_invalid() {
            return;
        }
        for &recipient_id in &cmd.base.recipients {
            let _accepted =
                world.issue_gather_order(player_id, recipient_id, cmd.unit_id, gameplay);
        }
    }

    fn execute_capture(&self, world: &mut World, cmd: &WorkCommand) {
        let (Some(database), Some(gameplay), Ok(player_id)) = (
            self.database,
            self.gameplay,
            u8::try_from(cmd.base.player_id),
        ) else {
            return;
        };
        if cmd.unit_id.is_invalid() {
            return;
        }
        for &recipient_id in &cmd.base.recipients {
            let _accepted =
                world.issue_capture_order(player_id, recipient_id, cmd.unit_id, database, gameplay);
        }
    }

    fn execute_repair_other(&self, world: &mut World, cmd: &WorkCommand) {
        let (Some(database), Some(gameplay), Ok(player_id), Ok(ability_id)) = (
            self.database,
            self.gameplay,
            u8::try_from(cmd.base.player_id),
            self.work_ability_id(cmd.ability_id),
        ) else {
            return;
        };
        if cmd.unit_id.is_invalid() {
            return;
        }
        for &recipient_id in &cmd.base.recipients {
            let _accepted = world.issue_repair_other_order(
                player_id,
                recipient_id,
                cmd.unit_id,
                ability_id,
                database,
                gameplay,
            );
        }
    }

    /// Execute a move order.
    fn execute_move(world: &mut World, cmd: &WorkCommand) {
        let entity_position = (!cmd.unit_id.is_invalid())
            .then(|| world.squad_move_entity_target(cmd.unit_id))
            .flatten()
            .map(|(_, position)| position);
        if let Some(target) = entity_position.or(cmd.terrain_point) {
            Self::move_owned_recipients(world, cmd, target);
        }
    }

    /// Execute an attack order.
    fn execute_attack(&self, world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        if cmd.unit_id.is_invalid() {
            return;
        }
        let squad_mode = if cmd.squad_mode == -1 {
            None
        } else {
            let Some(mode) = SquadMode::from_i32(cmd.squad_mode) else {
                return;
            };
            Some(mode)
        };
        let Ok(ability_id) = self.work_ability_id(cmd.ability_id) else {
            return;
        };
        for &recipient_id in &cmd.base.recipients {
            if ability_id.is_some_and(|ability_id| {
                self.ability_order_is_recovering(world, recipient_id, ability_id)
            }) {
                continue;
            }
            let _accepted = world.issue_attack_order_with_context(
                player_id,
                recipient_id,
                cmd.unit_id,
                cmd.range,
                squad_mode.or_else(|| {
                    ability_id.and_then(|id| self.ability_squad_mode(world, recipient_id, id))
                }),
                ability_id,
            );
        }
    }

    fn execute_garrison(world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        if cmd.unit_id.is_invalid() {
            return;
        }
        for &recipient_id in &cmd.base.recipients {
            let _result =
                world.issue_garrison_order(player_id, recipient_id, cmd.unit_id, cmd.range);
        }
    }

    fn execute_join(&self, world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        if cmd.unit_id.is_invalid() {
            return;
        }
        let Ok(ability_id) = self.work_ability_id(cmd.ability_id) else {
            return;
        };
        for &recipient_id in &cmd.base.recipients {
            let _accepted =
                world.issue_join_order(player_id, recipient_id, cmd.unit_id, ability_id);
        }
    }

    fn execute_cloak(&self, world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        let Ok(ability_id) = self.work_ability_id(cmd.ability_id) else {
            return;
        };
        for &recipient_id in &cmd.base.recipients {
            let _accepted = world.issue_cloak_order(player_id, recipient_id, ability_id);
        }
    }

    fn execute_ungarrison(world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        for &recipient_id in &cmd.base.recipients {
            let _result = world.issue_ungarrison_order(player_id, recipient_id, cmd.terrain_point);
        }
    }

    fn execute_hitch(world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        if cmd.unit_id.is_invalid() {
            return;
        }
        for &recipient_id in &cmd.base.recipients {
            let _result = world.issue_hitch_order(player_id, recipient_id, cmd.unit_id);
        }
    }

    fn execute_unhitch(world: &mut World, cmd: &WorkCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        for &recipient_id in &cmd.base.recipients {
            let _result = world.issue_unhitch_order(player_id, recipient_id, cmd.unit_id);
        }
    }

    fn work_ability_id(&self, raw: i32) -> Result<Option<u8>, InvalidWorkAbility> {
        if raw == -1 {
            return Ok(None);
        }
        let id = u8::try_from(raw).map_err(|_| InvalidWorkAbility)?;
        if self
            .database
            .is_some_and(|database| usize::from(id) >= database.abilities.len())
        {
            return Err(InvalidWorkAbility);
        }
        Ok(Some(id))
    }

    fn ability_order_is_recovering(
        &self,
        world: &World,
        recipient_id: crate::EntityId,
        requested_id: u8,
    ) -> bool {
        let (Some(database), Some(squad)) = (self.database, world.get_squad(recipient_id)) else {
            return false;
        };
        let Some(proto_object_name) = squad
            .unit_ids
            .iter()
            .find_map(|unit_id| world.get_unit(*unit_id))
            .map(|unit| unit.proto_object_name.as_str())
        else {
            return false;
        };
        let Some((_, ability)) =
            resolve_database_ability(database, proto_object_name, requested_id)
        else {
            return false;
        };
        ability
            .recover_type
            .as_deref()
            .and_then(RecoveryType::from_authored)
            .is_some_and(|recovery_type| squad.recovery.blocks(recovery_type))
    }

    fn ability_squad_mode(
        &self,
        world: &World,
        recipient_id: crate::EntityId,
        requested_id: u8,
    ) -> Option<SquadMode> {
        let database = self.database?;
        let squad = world.get_squad(recipient_id)?;
        let proto_object_name = squad
            .unit_ids
            .iter()
            .find_map(|unit_id| world.get_unit(*unit_id))?
            .proto_object_name
            .as_str();
        let (_, ability) = resolve_database_ability(database, proto_object_name, requested_id)?;
        ability
            .squad_mode
            .as_deref()
            .and_then(SquadMode::from_authored)
    }

    fn move_owned_recipients(world: &mut World, cmd: &WorkCommand, target: glam::Vec3) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        let attack_move = cmd.base.has_flag(work_command_flags::ATTACK_MOVE);
        let queue = cmd.base.has_flag(crate::Command::ALTERNATE_FLAG);
        for &recipient_id in &cmd.base.recipients {
            let accepted = world.issue_squad_move_order_to_position(
                player_id,
                recipient_id,
                target,
                attack_move,
                queue,
            );
            if !accepted && !attack_move && !queue {
                let _accepted = world.issue_move_order(player_id, recipient_id, target);
            }
        }
    }

    /// Execute a power command.
    fn execute_power(&self, world: &mut World, cmd: &PowerCommand) {
        let (Some(database), Ok(player_id)) = (self.database, u8::try_from(cmd.base.player_id))
        else {
            return;
        };
        match cmd.power_type {
            PowerCommandType::GrantPower => {
                let _granted = world.grant_player_power(
                    player_id,
                    database,
                    PowerGrant {
                        proto_power_id: cmd.proto_power_id,
                        squad_id: crate::EntityId::INVALID,
                        uses: cmd.num_uses,
                        icon_location: -1,
                        ignore_cost: false,
                        ignore_tech_prerequisites: false,
                        ignore_population: false,
                    },
                );
            }
            PowerCommandType::InvokePower2 => {
                let Ok(power_level) = u32::try_from(cmd.power_level) else {
                    return;
                };
                let _started = world.invoke_native_power(
                    database,
                    NativePowerInvocation {
                        player_id,
                        proto_power_id: cmd.proto_power_id,
                        power_level,
                        squad_id: cmd.squad_id,
                        target_location: cmd.target_location.truncate(),
                        ignore_requirements: cmd.base.has_flag(power_command_flags::NO_COST),
                        power_user_id: PowerUserId::from_raw(cmd.power_user_id.cast_unsigned()),
                    },
                );
            }
            PowerCommandType::Undefined
            | PowerCommandType::InvokePower
            | PowerCommandType::InvokeAbility => {}
        }
    }

    /// Route synchronized input to the matching running native power.
    fn execute_power_input(&self, world: &mut World, cmd: &PowerInputCommand) {
        let Some(database) = self.database else {
            return;
        };
        let input = match cmd.input_type {
            PowerInputCommandType::Confirm => {
                crate::world::NativePowerInput::Confirm(cmd.vector.truncate())
            }
            PowerInputCommandType::Position => {
                crate::world::NativePowerInput::Position(cmd.vector.truncate())
            }
            PowerInputCommandType::Direction => {
                crate::world::NativePowerInput::Direction(cmd.vector.truncate())
            }
            PowerInputCommandType::Shutdown => crate::world::NativePowerInput::Shutdown,
            PowerInputCommandType::Undefined => return,
        };
        let no_cost = cmd.base.has_flag(power_input_command_flags::NO_COST);
        let _accepted =
            world.submit_native_power_input(database, cmd.power_user_id, input, no_cost);
    }

    /// Execute supported building production commands.
    fn execute_building(&self, world: &mut World, cmd: &BuildingCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        if cmd.building_type == BuildingCommandType::ClearRallyPoint {
            for &building_id in &cmd.base.recipients {
                let _cleared = world.clear_unit_rally_point(building_id, player_id);
            }
            return;
        }
        if cmd.count == 0 {
            return;
        }
        for &building_id in &cmd.base.recipients {
            if cmd.building_type == BuildingCommandType::CustomCommand {
                if cmd.count > 0 {
                    let _queued = world.queue_custom_command(player_id, building_id, cmd.target_id);
                } else {
                    let _canceled =
                        world.cancel_custom_command(player_id, building_id, cmd.target_id);
                }
                continue;
            }
            let Some(database) = self.database else {
                continue;
            };
            match cmd.building_type {
                BuildingCommandType::Research if cmd.count > 0 => {
                    let _result =
                        world.queue_research(player_id, building_id, database, cmd.target_id);
                }
                BuildingCommandType::Research => {
                    let _result =
                        world.cancel_research(player_id, building_id, database, cmd.target_id);
                }
                BuildingCommandType::TrainUnit | BuildingCommandType::TrainSquad => {
                    let kind = if cmd.building_type == BuildingCommandType::TrainUnit {
                        TrainingKind::Unit
                    } else {
                        TrainingKind::Squad
                    };
                    if cmd.count > 0 {
                        let _result = world.queue_training(
                            player_id,
                            building_id,
                            database,
                            kind,
                            cmd.target_id,
                            cmd.count.cast_unsigned(),
                        );
                    } else {
                        let _result = world.cancel_training(
                            player_id,
                            building_id,
                            database,
                            kind,
                            cmd.target_id,
                            cmd.count.unsigned_abs(),
                        );
                    }
                }
                BuildingCommandType::Build => {
                    if cmd.count > 0 {
                        let _result = world.start_build(
                            player_id,
                            building_id,
                            database,
                            cmd.target_id,
                            cmd.target_position,
                            cmd.socket_id,
                        );
                    } else {
                        let _result = world.cancel_build(player_id, building_id, cmd.target_id);
                    }
                }
                BuildingCommandType::BuildOther => {
                    if cmd.count > 0 {
                        let _result = world.queue_build_other(
                            player_id,
                            building_id,
                            database,
                            cmd.target_id,
                        );
                    } else {
                        let _result =
                            world.cancel_build_other(player_id, building_id, cmd.target_id);
                    }
                }
                _ => {}
            }
        }
    }

    /// Execute a game command.
    fn execute_game(&self, world: &mut World, cmd: &GameCommand) {
        let Ok(player_id) = u8::try_from(cmd.base.player_id) else {
            return;
        };
        match cmd.game_type {
            GameCommandType::SetGlobalRallyPoint => {
                let _set = world.set_player_rally_point(
                    player_id,
                    cmd.position,
                    optional_entity_id(cmd.data),
                );
            }
            GameCommandType::ClearGlobalRallyPoint => {
                let _cleared = world.clear_player_rally_point(player_id);
            }
            GameCommandType::SetBuildingRallyPoint => {
                let building_id = crate::EntityId::from_u32(cmd.data.cast_unsigned());
                let _set = world.set_unit_rally_point(
                    building_id,
                    player_id,
                    cmd.position,
                    optional_entity_id(cmd.data2),
                );
            }
            GameCommandType::ClearBuildingRallyPoint => {
                let building_id = crate::EntityId::from_u32(cmd.data.cast_unsigned());
                let _cleared = world.clear_unit_rally_point(building_id, player_id);
            }
            GameCommandType::CreateSquad => {
                if let Some(database) = self.database {
                    Self::execute_create_squad(world, database, cmd);
                }
            }
            GameCommandType::CreateObject => {
                if let Some(database) = self.database {
                    Self::execute_create_object(world, database, cmd);
                }
            }
            _ => {}
        }
    }

    fn execute_create_squad(world: &mut World, database: &Database, cmd: &GameCommand) {
        let (Ok(player_id), Ok(count)) =
            (u8::try_from(cmd.base.player_id), u32::try_from(cmd.data2))
        else {
            return;
        };
        let _spawned = spawn_squads_at(
            world,
            database,
            player_id,
            cmd.data,
            count,
            cmd.position,
            glam::Vec3::Z,
        );
    }

    fn execute_create_object(world: &mut World, database: &Database, cmd: &GameCommand) {
        let (Ok(player_id), Ok(count)) =
            (u8::try_from(cmd.base.player_id), u32::try_from(cmd.data2))
        else {
            return;
        };
        if count > MAX_SPAWN_BATCH {
            return;
        }
        for _ in 0..count {
            if spawn_object_at(
                world,
                database,
                player_id,
                cmd.data,
                cmd.position,
                glam::Vec3::Z,
            )
            .is_err()
            {
                break;
            }
        }
    }
}

fn optional_entity_id(raw: i32) -> Option<crate::EntityId> {
    let entity_id = crate::EntityId::from_u32(raw.cast_unsigned());
    (!entity_id.is_invalid()).then_some(entity_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::command_queue::QueuedCommand;
    use crate::entities::SquadState;
    use crate::order::OrderType;
    use pipeline::database::hw1::Power;

    #[test]
    fn test_move_command_sets_squad_target() {
        let mut world = World::new();
        world.init_players(1);

        // Create a squad at origin
        let squad_id = world.create_squad(1);

        // Create a move command
        let work_cmd = WorkCommand {
            base: Command {
                id: OrderType::Move as i32,
                player_id: 1,
                recipients: vec![squad_id],
                ..Default::default()
            },
            terrain_point: Some(Vec3::new(100.0, 0.0, 50.0)),
            ..Default::default()
        };

        let entry = CommandEntry {
            command: QueuedCommand::Work(work_cmd),
            exec_time: 50,
            sequence: 0,
            source_client: 1,
        };

        // Execute the command
        let executor = CommandExecutor::new();
        executor.execute(&mut world, &entry);

        // Verify squad is now moving to target
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.state, SquadState::Moving);
        assert_eq!(squad.move_target, Some(Vec3::new(100.0, 0.0, 50.0)));
    }

    #[test]
    fn move_command_flags_reach_the_shared_squad_order_queue() {
        let mut world = World::new();
        world.init_players(1);
        let squad_id = world.create_squad(1);
        let first_target = Vec3::X;
        assert!(world.issue_move_order(1, squad_id, first_target));
        let second_target = Vec3::new(2.0, 0.0, 0.0);
        let mut base = Command {
            id: OrderType::Move as i32,
            player_id: 1,
            recipients: vec![squad_id],
            ..Default::default()
        };
        base.set_flag(crate::Command::ALTERNATE_FLAG, true);
        base.set_flag(work_command_flags::ATTACK_MOVE, true);
        let entry = CommandEntry {
            command: QueuedCommand::Work(WorkCommand {
                base,
                terrain_point: Some(second_target),
                ..Default::default()
            }),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::new().execute(&mut world, &entry);
        assert_eq!(
            world.get_squad(squad_id).unwrap().move_target,
            Some(first_target)
        );
        world
            .get_squad_mut(squad_id)
            .unwrap()
            .finish_current_movement();
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.move_target, Some(second_target));
        assert!(squad.is_executing_attack_move());
    }

    #[test]
    fn join_command_reaches_persistent_squad_state() {
        let mut world = World::new();
        world.init_players(1);
        let source_id = world.create_squad(1);
        let source_unit_id = world.create_unit(1);
        assert!(world.attach_unit_to_squad(source_unit_id, source_id));
        let target_id = world.create_squad(1);
        let target_unit_id = world.create_unit(1);
        assert!(world.attach_unit_to_squad(target_unit_id, target_id));
        let entry = CommandEntry {
            command: QueuedCommand::Work(WorkCommand::join_squads(1, vec![source_id], target_id)),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::new().execute(&mut world, &entry);

        assert_eq!(
            world.get_squad(source_id).unwrap().join_target(),
            Some(target_id)
        );
    }

    #[test]
    fn cloak_command_reaches_authoritative_squad_state() {
        let mut world = World::new();
        world.init_players(1);
        let squad_id = world.create_squad(1);
        let unit_id = world.create_unit(1);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        let entry = CommandEntry {
            command: QueuedCommand::Work(WorkCommand::cloak_squads(1, vec![squad_id], None)),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::new().execute(&mut world, &entry);

        assert!(world.get_squad(squad_id).unwrap().wants_to_cloak());
    }

    #[test]
    fn test_squad_moves_toward_target() {
        let mut world = World::new();
        world.init_players(1);

        // Create a squad at origin
        let squad_id = world.create_squad_at(1, Vec3::ZERO);

        // Issue move command to (100, 0, 0)
        {
            let squad = world.get_squad_mut(squad_id).unwrap();
            squad.move_to(Vec3::new(100.0, 0.0, 0.0));
        }

        // Simulate one tick (50ms = 0.05s)
        world.update_entities(0.05);

        // Squad should have moved toward target
        let squad = world.get_squad(squad_id).unwrap();
        assert!(squad.position().x > 0.0);
        assert!(squad.position().x < 100.0);
        assert_eq!(squad.state, SquadState::Moving);
    }

    #[test]
    fn test_move_command_moves_standalone_unit() {
        let mut world = World::new();
        let unit_id = world.create_unit(1);
        let target = Vec3::new(25.0, 0.0, 10.0);
        let work_cmd = WorkCommand {
            base: Command {
                id: OrderType::Move as i32,
                player_id: 1,
                recipients: vec![unit_id],
                ..Default::default()
            },
            terrain_point: Some(target),
            ..Default::default()
        };
        let entry = CommandEntry {
            command: QueuedCommand::Work(work_cmd),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::new().execute(&mut world, &entry);

        assert_eq!(world.get_unit(unit_id).unwrap().move_target, Some(target));
    }

    #[test]
    fn attack_command_preserves_mode_and_ability_context() {
        let mut world = World::new();
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        let attacker = world.create_squad(1);
        let target = world.create_unit(2);
        let work_cmd = WorkCommand {
            base: Command {
                id: OrderType::Attack as i32,
                player_id: 1,
                recipients: vec![attacker],
                ..Default::default()
            },
            unit_id: target,
            squad_mode: SquadMode::Cover as i32,
            ability_id: 3,
            ..Default::default()
        };
        let entry = CommandEntry {
            command: QueuedCommand::Work(work_cmd),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::new().execute(&mut world, &entry);

        let squad = world.get_squad(attacker).unwrap();
        assert_eq!(squad.mode, SquadMode::Cover);
        assert_eq!(squad.attack_ability_id, Some(3));
        assert_eq!(squad.attack_target, Some(target));
    }

    #[test]
    fn grant_power_command_updates_authoritative_player_entry() {
        let database = Database {
            powers: vec![Power {
                name: "test_power".to_owned(),
                ..Power::default()
            }],
            ..Database::default()
        };
        let mut world = World::new();
        world.init_players(1);
        let command = PowerCommand {
            base: Command {
                player_id: 1,
                ..Command::default()
            },
            power_type: PowerCommandType::GrantPower,
            num_uses: 3,
            proto_power_id: 0,
            ..PowerCommand::default()
        };
        let entry = CommandEntry {
            command: QueuedCommand::Power(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::with_database(&database).execute(&mut world, &entry);

        let power = world.get_player(1).unwrap().power_entry(0).unwrap();
        assert_eq!(power.finite_uses_remaining(), 3);
    }

    #[test]
    fn game_rally_commands_update_sim_state_without_a_database() {
        let mut world = World::new();
        world.init_players(1);
        let target = world.create_unit_at(1, Vec3::new(10.0, 0.0, 20.0));
        let building = world.create_building(1);
        let executor = CommandExecutor::new();

        execute_game_command(
            &executor,
            &mut world,
            GameCommand {
                base: Command {
                    player_id: 1,
                    ..Command::default()
                },
                game_type: GameCommandType::SetGlobalRallyPoint,
                data: target.as_u32().cast_signed(),
                position: Vec3::ONE,
                ..GameCommand::default()
            },
        );
        assert_eq!(
            world.player_rally_point(1).unwrap().target_entity_id(),
            Some(target)
        );

        execute_game_command(
            &executor,
            &mut world,
            GameCommand {
                base: Command {
                    player_id: 1,
                    ..Command::default()
                },
                game_type: GameCommandType::SetBuildingRallyPoint,
                data: building.as_u32().cast_signed(),
                data2: -1,
                position: Vec3::new(30.0, 0.0, 40.0),
                ..GameCommand::default()
            },
        );
        assert_eq!(
            world
                .unit_rally_point(building, 1)
                .map(crate::RallyPoint::position),
            Some(Vec3::new(30.0, 0.0, 40.0))
        );

        execute_game_command(
            &executor,
            &mut world,
            GameCommand {
                base: Command {
                    player_id: 1,
                    ..Command::default()
                },
                game_type: GameCommandType::ClearGlobalRallyPoint,
                ..GameCommand::default()
            },
        );
        assert!(world.player_rally_point(1).is_none());
    }

    #[test]
    fn building_clear_rally_command_is_not_blocked_by_zero_count() {
        let mut world = World::new();
        world.init_players(1);
        let building = world.create_building(1);
        assert!(world.set_unit_rally_point(building, 1, Vec3::ONE, None));
        let command = BuildingCommand {
            base: Command {
                player_id: 1,
                recipients: vec![building],
                ..Command::default()
            },
            building_type: BuildingCommandType::ClearRallyPoint,
            count: 0,
            ..BuildingCommand::default()
        };
        let entry = CommandEntry {
            command: QueuedCommand::Building(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        };

        CommandExecutor::new().execute(&mut world, &entry);

        assert!(world.unit_rally_point(building, 1).is_none());
    }

    fn execute_game_command(
        executor: &CommandExecutor<'_>,
        world: &mut World,
        command: GameCommand,
    ) {
        executor.execute(
            world,
            &CommandEntry {
                command: QueuedCommand::Game(command),
                exec_time: 0,
                sequence: 0,
                source_client: 1,
            },
        );
    }
}
