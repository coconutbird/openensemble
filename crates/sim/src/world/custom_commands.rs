//! Authoritative scenario-defined custom command registry and execution state.

use super::World;
use crate::EntityId;
use crate::player::{PlayerId, Resources};
use crate::sync::SyncChecksum;

/// Boolean behavior flags authored for one retail custom command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CustomCommandFlags(u8);

impl CustomCommandFlags {
    const QUEUE: u8 = 1 << 0;
    const ALLOW_MULTIPLE: u8 = 1 << 1;
    const SHOW_LIMIT: u8 = 1 << 2;
    const CLOSE_MENU: u8 = 1 << 3;
    const PERSISTENT: u8 = 1 << 4;
    const UNAVAILABLE: u8 = 1 << 5;
    const ALLOW_CANCEL: u8 = 1 << 6;

    #[must_use]
    pub const fn with_queue(self, enabled: bool) -> Self {
        self.with(Self::QUEUE, enabled)
    }

    #[must_use]
    pub const fn with_allow_multiple(self, enabled: bool) -> Self {
        self.with(Self::ALLOW_MULTIPLE, enabled)
    }

    #[must_use]
    pub const fn with_show_limit(self, enabled: bool) -> Self {
        self.with(Self::SHOW_LIMIT, enabled)
    }

    #[must_use]
    pub const fn with_close_menu(self, enabled: bool) -> Self {
        self.with(Self::CLOSE_MENU, enabled)
    }

    #[must_use]
    pub const fn with_persistent(self, enabled: bool) -> Self {
        self.with(Self::PERSISTENT, enabled)
    }

    #[must_use]
    pub const fn with_unavailable(self, enabled: bool) -> Self {
        self.with(Self::UNAVAILABLE, enabled)
    }

    #[must_use]
    pub const fn with_allow_cancel(self, enabled: bool) -> Self {
        self.with(Self::ALLOW_CANCEL, enabled)
    }

    #[must_use]
    pub const fn queue(self) -> bool {
        self.contains(Self::QUEUE)
    }

    #[must_use]
    pub const fn allow_multiple(self) -> bool {
        self.contains(Self::ALLOW_MULTIPLE)
    }

    #[must_use]
    pub const fn show_limit(self) -> bool {
        self.contains(Self::SHOW_LIMIT)
    }

    #[must_use]
    pub const fn close_menu(self) -> bool {
        self.contains(Self::CLOSE_MENU)
    }

    #[must_use]
    pub const fn persistent(self) -> bool {
        self.contains(Self::PERSISTENT)
    }

    #[must_use]
    pub const fn unavailable(self) -> bool {
        self.contains(Self::UNAVAILABLE)
    }

    #[must_use]
    pub const fn allow_cancel(self) -> bool {
        self.contains(Self::ALLOW_CANCEL)
    }

    const fn with(mut self, flag: u8, enabled: bool) -> Self {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
        self
    }

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

/// One retail `BCustomCommand` exposed to renderer/UI consumers.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomCommand {
    pub id: i32,
    pub unit_id: EntityId,
    pub icon_position: i32,
    pub icon_name: Option<String>,
    pub cost: Resources,
    pub timer_seconds: f32,
    pub limit: i32,
    pub name_string_id: i32,
    pub info_string_id: i32,
    pub help_string_id: i32,
    pub queued_count: i32,
    pub finished_count: i32,
    pub flags: CustomCommandFlags,
}

impl Default for CustomCommand {
    fn default() -> Self {
        Self {
            id: -1,
            unit_id: EntityId::INVALID,
            icon_position: -1,
            icon_name: None,
            cost: Resources::new(),
            timer_seconds: 0.0,
            limit: 0,
            name_string_id: -1,
            info_string_id: -1,
            help_string_id: -1,
            queued_count: 0,
            finished_count: 0,
            flags: CustomCommandFlags::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CustomCommandExecution {
    command_id: i32,
    unit_id: EntityId,
    player_id: PlayerId,
    remaining_seconds: f32,
    cost: Resources,
}

impl World {
    /// Add a command and return the monotonically assigned retail ID.
    pub fn add_custom_command(&mut self, mut command: CustomCommand) -> i32 {
        let id = self.next_custom_command_id;
        self.next_custom_command_id = self.next_custom_command_id.wrapping_add(1);
        command.id = id;
        self.custom_commands.insert(id, command);
        id
    }

    /// Remove a command and cancel/refund any work that still references it.
    pub fn remove_custom_command(&mut self, command_id: i32) -> Option<CustomCommand> {
        let removed = self.custom_commands.remove(&command_id)?;
        let canceled = take_matching(&mut self.custom_command_executions, |execution| {
            execution.command_id == command_id
        });
        for execution in canceled {
            self.refund_cost(execution.player_id, &execution.cost);
        }
        Some(removed)
    }

    #[must_use]
    pub fn custom_command(&self, command_id: i32) -> Option<&CustomCommand> {
        self.custom_commands.get(&command_id)
    }

    pub fn custom_commands(&self) -> impl Iterator<Item = &CustomCommand> {
        self.custom_commands.values()
    }

    #[must_use]
    pub fn next_custom_command_id(&self) -> i32 {
        self.next_custom_command_id
    }

    /// Validate, pay for, and enqueue one player-issued custom command.
    pub fn queue_custom_command(
        &mut self,
        player_id: PlayerId,
        unit_id: EntityId,
        command_id: i32,
    ) -> bool {
        let Some(command) = self.custom_commands.get(&command_id).cloned() else {
            return false;
        };
        if command.unit_id != unit_id
            || command.flags.unavailable()
            || self.get_unit(unit_id).is_none()
            || self.get_player(player_id).is_none()
            || (command.limit != 0
                && command.queued_count.saturating_add(command.finished_count) >= command.limit)
            || (command.queued_count > 0
                && (!command.flags.allow_multiple() || !command.flags.persistent()))
        {
            return false;
        }
        if !self
            .get_player(player_id)
            .is_some_and(|player| player.resources.can_afford(&command.cost))
        {
            return false;
        }

        if !command.flags.queue() {
            self.cancel_custom_command_queue(player_id, unit_id);
        }
        if let Some(player) = self.get_player_mut(player_id) {
            player.resources.pay(&command.cost);
        }
        if let Some(stored) = self.custom_commands.get_mut(&command_id) {
            stored.queued_count = stored.queued_count.saturating_add(1);
        }
        if !command.flags.queue() && command.timer_seconds == 0.0 {
            self.complete_custom_command(command_id);
        } else {
            self.custom_command_executions.push(CustomCommandExecution {
                command_id,
                unit_id,
                player_id,
                remaining_seconds: command.timer_seconds.max(0.0),
                cost: command.cost,
            });
        }
        true
    }

    /// Cancel the newest matching command execution and refund its cost.
    pub fn cancel_custom_command(
        &mut self,
        player_id: PlayerId,
        unit_id: EntityId,
        command_id: i32,
    ) -> bool {
        if !self
            .custom_commands
            .get(&command_id)
            .is_some_and(|command| command.flags.allow_cancel())
        {
            return false;
        }
        let Some(index) = self
            .custom_command_executions
            .iter()
            .rposition(|execution| {
                execution.command_id == command_id
                    && execution.unit_id == unit_id
                    && execution.player_id == player_id
            })
        else {
            return false;
        };
        let execution = self.custom_command_executions.remove(index);
        if let Some(command) = self.custom_commands.get_mut(&command_id) {
            command.queued_count = command.queued_count.saturating_sub(1);
        }
        self.refund_cost(player_id, &execution.cost);
        true
    }

    /// Advance the first queued custom command for each unit/player worker.
    pub fn update_custom_commands(&mut self, elapsed_seconds: f32) -> usize {
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
            return 0;
        }
        let mut completed = Vec::new();
        for index in 0..self.custom_command_executions.len() {
            let execution = self.custom_command_executions[index];
            let blocked = self.custom_command_executions[..index]
                .iter()
                .any(|earlier| {
                    earlier.unit_id == execution.unit_id && earlier.player_id == execution.player_id
                });
            if blocked {
                continue;
            }
            let remaining = &mut self.custom_command_executions[index].remaining_seconds;
            *remaining = (*remaining - elapsed_seconds).max(0.0);
            if *remaining == 0.0 {
                completed.push(index);
            }
        }
        let count = completed.len();
        for index in completed.into_iter().rev() {
            let execution = self.custom_command_executions.remove(index);
            self.complete_custom_command(execution.command_id);
        }
        count
    }

    pub(crate) fn hash_custom_commands(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.next_custom_command_id);
        checksum.hash_u32(u32::try_from(self.custom_commands.len()).unwrap_or(u32::MAX));
        for command in self.custom_commands.values() {
            command.hash_state(checksum);
        }
        checksum.hash_u32(u32::try_from(self.custom_command_executions.len()).unwrap_or(u32::MAX));
        for execution in &self.custom_command_executions {
            checksum.hash_i32(execution.command_id);
            checksum.hash_u32(execution.unit_id.as_u32());
            checksum.hash_u32(u32::from(execution.player_id));
            checksum.hash_f32(execution.remaining_seconds);
            hash_resources(checksum, &execution.cost);
        }
    }

    fn cancel_custom_command_queue(&mut self, player_id: PlayerId, unit_id: EntityId) {
        let canceled = take_matching(&mut self.custom_command_executions, |execution| {
            execution.player_id == player_id && execution.unit_id == unit_id
        });
        for execution in canceled {
            if let Some(command) = self.custom_commands.get_mut(&execution.command_id) {
                command.queued_count = command.queued_count.saturating_sub(1);
            }
            self.refund_cost(player_id, &execution.cost);
        }
    }

    fn complete_custom_command(&mut self, command_id: i32) {
        let remove = if let Some(command) = self.custom_commands.get_mut(&command_id) {
            command.queued_count = command.queued_count.saturating_sub(1);
            command.finished_count = command.finished_count.saturating_add(1);
            !command.flags.persistent()
        } else {
            false
        };
        if remove {
            self.custom_commands.remove(&command_id);
        }
    }
}

impl CustomCommand {
    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.id);
        checksum.hash_u32(self.unit_id.as_u32());
        checksum.hash_i32(self.icon_position);
        if let Some(icon_name) = &self.icon_name {
            checksum.hash_u32(1);
            checksum.hash_bytes(icon_name.as_bytes());
        } else {
            checksum.hash_u32(0);
        }
        hash_resources(checksum, &self.cost);
        checksum.hash_f32(self.timer_seconds);
        checksum.hash_i32(self.limit);
        checksum.hash_i32(self.name_string_id);
        checksum.hash_i32(self.info_string_id);
        checksum.hash_i32(self.help_string_id);
        checksum.hash_i32(self.queued_count);
        checksum.hash_i32(self.finished_count);
        for flag in [
            self.flags.queue(),
            self.flags.allow_multiple(),
            self.flags.show_limit(),
            self.flags.close_menu(),
            self.flags.persistent(),
            self.flags.unavailable(),
            self.flags.allow_cancel(),
        ] {
            checksum.hash_u32(u32::from(flag));
        }
    }
}

fn take_matching<T>(values: &mut Vec<T>, predicate: impl Fn(&T) -> bool) -> Vec<T> {
    let mut removed = Vec::new();
    let mut index = 0;
    while index < values.len() {
        if predicate(&values[index]) {
            removed.push(values.remove(index));
        } else {
            index += 1;
        }
    }
    removed
}

fn hash_resources(checksum: &mut SyncChecksum, resources: &Resources) {
    for amount in resources.amounts {
        checksum.hash_f32(amount);
    }
}

#[cfg(test)]
mod tests;
