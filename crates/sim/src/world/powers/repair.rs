//! Retail `BPowerRepair` profile resolution and authoritative execution.

use super::common::{
    PowerPayment, consume_power, create_power_visual, hash_string, optional_bool,
    optional_prototype, required_bool, required_float, required_int, required_object_type,
    required_prototype, seconds_to_milliseconds, validate_level, validate_power_type,
    validate_requirements,
};
use super::{PowerExecutionId, RepairPowerError, RepairPowerInvocation, power_by_id};
use crate::EntityId;
use crate::entity::Entity;
use crate::sync::SyncChecksum;
use crate::world::{GeneralEvent, GeneralEventType, World};
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

#[derive(Debug, Clone, Copy)]
struct RepairIgnore {
    squad_id: EntityId,
    expires_at_ms: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct RepairFlags(u8);

impl RepairFlags {
    const SPREAD_AMONG_SQUADS: Self = Self(1 << 0);
    const ALLOW_REINFORCE: Self = Self(1 << 1);
    const IGNORE_PLACEMENT: Self = Self(1 << 2);
    const HEAL_ANY: Self = Self(1 << 3);
    const NEVER_STOPS: Self = Self(1 << 4);

    const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    fn set(&mut self, flag: Self, enabled: bool) {
        if enabled {
            self.0 |= flag.0;
        } else {
            self.0 &= !flag.0;
        }
    }
}

/// Authoritative state for one running retail `BPowerRepair` execution.
#[derive(Debug, Clone)]
pub struct RepairPowerExecution {
    pub(super) id: PowerExecutionId,
    player_id: u8,
    proto_power_id: i32,
    power_level: u32,
    owner_squad_id: EntityId,
    repair_object_id: EntityId,
    target_location: Vec3,
    repair_object_prototype: String,
    repair_attachment_prototype: Option<String>,
    repair_attachment_prototype_id: Option<i32>,
    filter_type: String,
    radius: f32,
    tick_duration_ms: u32,
    next_tick_time_ms: u32,
    repair_combat_value_per_tick: f32,
    cooldown_time_if_damaged_ms: u32,
    ticks_remaining: u32,
    flags: RepairFlags,
    repairing_squads: Vec<EntityId>,
    ignored_squads: Vec<RepairIgnore>,
}

impl RepairPowerExecution {
    #[must_use]
    pub const fn id(&self) -> PowerExecutionId {
        self.id
    }

    #[must_use]
    pub const fn player_id(&self) -> u8 {
        self.player_id
    }

    #[must_use]
    pub const fn proto_power_id(&self) -> i32 {
        self.proto_power_id
    }

    #[must_use]
    pub const fn power_level(&self) -> u32 {
        self.power_level
    }

    #[must_use]
    pub const fn owner_squad_id(&self) -> EntityId {
        self.owner_squad_id
    }

    #[must_use]
    pub const fn repair_object_id(&self) -> EntityId {
        self.repair_object_id
    }

    #[must_use]
    pub const fn target_location(&self) -> Vec3 {
        self.target_location
    }

    #[must_use]
    pub fn repair_object_prototype(&self) -> &str {
        &self.repair_object_prototype
    }

    #[must_use]
    pub fn repair_attachment_prototype(&self) -> Option<&str> {
        self.repair_attachment_prototype.as_deref()
    }

    #[must_use]
    pub fn filter_type(&self) -> &str {
        &self.filter_type
    }

    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.radius
    }

    #[must_use]
    pub const fn tick_duration_ms(&self) -> u32 {
        self.tick_duration_ms
    }

    #[must_use]
    pub const fn next_tick_time_ms(&self) -> u32 {
        self.next_tick_time_ms
    }

    #[must_use]
    pub const fn repair_combat_value_per_tick(&self) -> f32 {
        self.repair_combat_value_per_tick
    }

    #[must_use]
    pub const fn cooldown_time_if_damaged_ms(&self) -> u32 {
        self.cooldown_time_if_damaged_ms
    }

    #[must_use]
    pub const fn ticks_remaining(&self) -> u32 {
        self.ticks_remaining
    }

    #[must_use]
    pub const fn spreads_among_squads(&self) -> bool {
        self.flags.contains(RepairFlags::SPREAD_AMONG_SQUADS)
    }

    #[must_use]
    pub const fn allows_reinforcement(&self) -> bool {
        self.flags.contains(RepairFlags::ALLOW_REINFORCE)
    }

    #[must_use]
    pub const fn ignores_placement(&self) -> bool {
        self.flags.contains(RepairFlags::IGNORE_PLACEMENT)
    }

    #[must_use]
    pub const fn heals_any_relation(&self) -> bool {
        self.flags.contains(RepairFlags::HEAL_ANY)
    }

    #[must_use]
    pub const fn never_stops(&self) -> bool {
        self.flags.contains(RepairFlags::NEVER_STOPS)
    }

    #[must_use]
    pub fn repairing_squads(&self) -> &[EntityId] {
        &self.repairing_squads
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id.get());
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_u32(self.power_level);
        checksum.hash_u32(self.owner_squad_id.as_u32());
        checksum.hash_u32(self.repair_object_id.as_u32());
        checksum.hash_vec3(
            self.target_location.x,
            self.target_location.y,
            self.target_location.z,
        );
        hash_string(checksum, &self.repair_object_prototype);
        hash_optional_string(checksum, self.repair_attachment_prototype.as_deref());
        checksum.hash_i32(self.repair_attachment_prototype_id.unwrap_or(-1));
        hash_string(checksum, &self.filter_type);
        checksum.hash_f32(self.radius);
        checksum.hash_u32(self.tick_duration_ms);
        checksum.hash_u32(self.next_tick_time_ms);
        checksum.hash_f32(self.repair_combat_value_per_tick);
        checksum.hash_u32(self.cooldown_time_if_damaged_ms);
        checksum.hash_u32(self.ticks_remaining);
        checksum.hash_u32(u32::from(self.spreads_among_squads()));
        checksum.hash_u32(u32::from(self.allows_reinforcement()));
        checksum.hash_u32(u32::from(self.ignores_placement()));
        checksum.hash_u32(u32::from(self.heals_any_relation()));
        checksum.hash_u32(u32::from(self.never_stops()));
        hash_entity_ids(checksum, &self.repairing_squads);
        checksum.hash_u32(u32::try_from(self.ignored_squads.len()).unwrap_or(u32::MAX));
        for ignored in &self.ignored_squads {
            checksum.hash_u32(ignored.squad_id.as_u32());
            checksum.hash_u32(ignored.expires_at_ms);
        }
    }
}

#[derive(Debug)]
struct RepairProfile {
    repair_object_prototype: String,
    repair_attachment_prototype: Option<String>,
    repair_attachment_prototype_id: Option<i32>,
    filter_type: String,
    radius: f32,
    tick_duration_ms: u32,
    ticks: u32,
    repair_combat_value_per_tick: f32,
    cooldown_time_if_damaged_ms: u32,
    flags: RepairFlags,
}

pub(super) fn invoke(
    world: &mut World,
    database: &Database,
    invocation: RepairPowerInvocation,
) -> Result<PowerExecutionId, RepairPowerError> {
    if !invocation.target_location.is_finite() {
        return Err(RepairPowerError::InvalidTarget);
    }
    let power = power_by_id(database, invocation.proto_power_id)
        .ok_or(RepairPowerError::PowerNotFound(invocation.proto_power_id))?;
    let attributes = validate_power_type(power, "Repair")?;
    if world.get_player(invocation.player_id).is_none() {
        return Err(RepairPowerError::PlayerNotFound(invocation.player_id));
    }
    let profile = RepairProfile::resolve(database, attributes, invocation.power_level)?;
    if !profile.flags.contains(RepairFlags::IGNORE_PLACEMENT)
        && world.is_outside_playable_bounds(invocation.target_location, true)
    {
        return Err(RepairPowerError::InvalidPlacement);
    }
    if let Some(disruption_id) =
        super::disruption::disrupting_power_at(world, attributes, invocation.target_location)
    {
        return Err(RepairPowerError::Disrupted(disruption_id));
    }
    let payment = if invocation.ignore_requirements {
        PowerPayment::default()
    } else {
        validate_requirements(
            world,
            database,
            power,
            invocation.player_id,
            invocation.proto_power_id,
        )?
    };
    let repair_object_id = create_power_visual(
        world,
        database,
        invocation.player_id,
        invocation.target_location,
        Vec3::Z,
        &profile.repair_object_prototype,
    );
    if !invocation.ignore_requirements
        && let Err(error) = consume_power(
            world,
            power,
            invocation.player_id,
            invocation.proto_power_id,
            invocation.squad_id,
            &payment,
        )
    {
        let _removed = world.remove_object(repair_object_id);
        return Err(error);
    }
    let id = world.power_manager.allocate_id();
    let execution = create_execution(world, invocation, profile, repair_object_id, id);
    world.power_manager.repair_executions.push(execution);
    let _fired = world.fire_general_event(&GeneralEvent::new(
        GeneralEventType::UsedPower,
        i32::from(invocation.player_id),
    ));
    Ok(id)
}

pub(super) fn update(world: &mut World, database: &Database) {
    let active = std::mem::take(&mut world.power_manager.repair_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active {
        if update_execution(world, database, &mut execution) {
            remaining.push(execution);
        } else {
            shutdown_execution(world, &mut execution);
        }
    }
    world.power_manager.repair_executions = remaining;
}

fn create_execution(
    world: &World,
    invocation: RepairPowerInvocation,
    profile: RepairProfile,
    repair_object_id: EntityId,
    id: PowerExecutionId,
) -> RepairPowerExecution {
    RepairPowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        repair_object_id,
        target_location: invocation.target_location,
        repair_object_prototype: profile.repair_object_prototype,
        repair_attachment_prototype: profile.repair_attachment_prototype,
        repair_attachment_prototype_id: profile.repair_attachment_prototype_id,
        filter_type: profile.filter_type,
        radius: profile.radius,
        tick_duration_ms: profile.tick_duration_ms,
        next_tick_time_ms: world.game_time_ms.wrapping_add(profile.tick_duration_ms),
        repair_combat_value_per_tick: profile.repair_combat_value_per_tick,
        cooldown_time_if_damaged_ms: profile.cooldown_time_if_damaged_ms,
        ticks_remaining: profile.ticks,
        flags: profile.flags,
        repairing_squads: Vec::new(),
        ignored_squads: Vec::new(),
    }
}

fn update_execution(
    world: &mut World,
    database: &Database,
    execution: &mut RepairPowerExecution,
) -> bool {
    let current_time = world.game_time_ms;
    if current_time < execution.next_tick_time_ms {
        return true;
    }
    let Some(attributes) =
        power_by_id(database, execution.proto_power_id).and_then(|power| power.attributes.as_ref())
    else {
        return false;
    };
    if world.get_player(execution.player_id).is_none()
        || super::disruption::disrupting_power_at(world, attributes, execution.target_location)
            .is_some()
    {
        return false;
    }
    let candidates = repair_candidates(world, execution);
    while (execution.ticks_remaining > 0 || execution.never_stops())
        && current_time >= execution.next_tick_time_ms
    {
        update_repair_tick(world, database, execution, &candidates);
    }
    execution.ticks_remaining > 0 || execution.never_stops()
}

fn update_repair_tick(
    world: &mut World,
    database: &Database,
    execution: &mut RepairPowerExecution,
    candidates: &[EntityId],
) {
    let current_tick_time = execution.next_tick_time_ms;
    if !execution.never_stops() {
        execution.ticks_remaining -= 1;
    }
    execution.next_tick_time_ms = execution
        .next_tick_time_ms
        .wrapping_add(execution.tick_duration_ms);
    clear_expired_ignores(execution, current_tick_time);
    let repairable = candidates
        .iter()
        .copied()
        .filter(|&squad_id| {
            squad_is_repairable(world, database, execution, squad_id, current_tick_time)
        })
        .collect::<Vec<_>>();
    let (arriving, leaving) = diff_squads(&repairable, &execution.repairing_squads);
    execution.repairing_squads.clone_from(&repairable);
    world.repair_squads_by_combat_value(
        database,
        &repairable,
        execution.repair_combat_value_per_tick,
        execution.spreads_among_squads(),
        execution.allows_reinforcement(),
    );
    handle_arriving_squads(world, database, execution, &arriving);
    handle_leaving_squads(world, execution, &leaving);
}

fn repair_candidates(world: &World, execution: &RepairPowerExecution) -> Vec<EntityId> {
    world
        .find_squads_by_leader_type_in_area(
            &execution.filter_type,
            execution.target_location,
            execution.radius,
        )
        .into_iter()
        .filter(|&squad_id| {
            world.get_squad(squad_id).is_some_and(|squad| {
                squad.is_alive()
                    && (execution.heals_any_relation()
                        || world.players_are_allied(execution.player_id, squad.base.player_id))
            })
        })
        .collect()
}

fn squad_is_repairable(
    world: &World,
    database: &Database,
    execution: &mut RepairPowerExecution,
    squad_id: EntityId,
    current_tick_time: u32,
) -> bool {
    let Some(squad) = world.get_squad(squad_id) else {
        return false;
    };
    if execution.cooldown_time_if_damaged_ms > 0 {
        let delta = i32::from_ne_bytes(
            current_tick_time
                .wrapping_sub(squad.last_damaged_time)
                .to_ne_bytes(),
        );
        let tick_duration = i32::from_ne_bytes(execution.tick_duration_ms.to_ne_bytes());
        if delta < tick_duration {
            ignore_squad(
                execution,
                squad_id,
                current_tick_time.wrapping_add(execution.cooldown_time_if_damaged_ms),
            );
            return false;
        }
        if execution
            .ignored_squads
            .iter()
            .any(|ignored| ignored.squad_id == squad_id)
        {
            return false;
        }
    }
    world.squad_hitpoint_fraction(squad_id, database) < 1.0
}

fn handle_arriving_squads(
    world: &mut World,
    database: &Database,
    execution: &RepairPowerExecution,
    squad_ids: &[EntityId],
) {
    let Some(prototype_id) = execution.repair_attachment_prototype_id else {
        for &squad_id in squad_ids {
            if let Some(squad) = world.get_squad_mut(squad_id) {
                let _first = squad.repair.begin();
            }
        }
        return;
    };
    for &squad_id in squad_ids {
        let first = world
            .get_squad_mut(squad_id)
            .is_some_and(|squad| squad.repair.begin());
        if !first {
            continue;
        }
        let offset = repair_attachment_offset(world, database, squad_id);
        let leader_id = world.get_squad(squad_id).and_then(|squad| {
            squad
                .unit_ids
                .iter()
                .find(|unit_id| world.get_unit(**unit_id).is_some())
                .copied()
        });
        if let Some(leader_id) = leader_id {
            let _attachment = world.add_prototype_attachment_to_unit_with_offset(
                database,
                leader_id,
                prototype_id,
                offset,
            );
        }
    }
}

fn handle_leaving_squads(
    world: &mut World,
    execution: &RepairPowerExecution,
    squad_ids: &[EntityId],
) {
    for &squad_id in squad_ids {
        let remove_attachments = world
            .get_squad_mut(squad_id)
            .is_none_or(|squad| squad.repair.end());
        if !remove_attachments {
            continue;
        }
        let Some(prototype_id) = execution.repair_attachment_prototype_id else {
            continue;
        };
        let member_ids = world
            .get_squad(squad_id)
            .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
        for unit_id in member_ids {
            let _removed = world.remove_unit_attachments_by_prototype(unit_id, prototype_id);
        }
    }
}

fn shutdown_execution(world: &mut World, execution: &mut RepairPowerExecution) {
    let repairing = std::mem::take(&mut execution.repairing_squads);
    handle_leaving_squads(world, execution, &repairing);
    let _removed = world.remove_object(execution.repair_object_id);
    execution.repair_object_id = EntityId::INVALID;
}

fn clear_expired_ignores(execution: &mut RepairPowerExecution, current_tick_time: u32) {
    if execution.cooldown_time_if_damaged_ms == 0 {
        return;
    }
    execution
        .ignored_squads
        .retain(|ignored| current_tick_time < ignored.expires_at_ms);
}

fn ignore_squad(execution: &mut RepairPowerExecution, squad_id: EntityId, expires_at_ms: u32) {
    if let Some(ignored) = execution
        .ignored_squads
        .iter_mut()
        .find(|ignored| ignored.squad_id == squad_id)
    {
        ignored.expires_at_ms = expires_at_ms;
    } else {
        execution.ignored_squads.push(RepairIgnore {
            squad_id,
            expires_at_ms,
        });
    }
}

fn diff_squads(current: &[EntityId], previous: &[EntityId]) -> (Vec<EntityId>, Vec<EntityId>) {
    let arriving = current
        .iter()
        .copied()
        .filter(|squad_id| !previous.contains(squad_id))
        .collect();
    let leaving = previous
        .iter()
        .copied()
        .filter(|squad_id| !current.contains(squad_id))
        .collect();
    (arriving, leaving)
}

fn repair_attachment_offset(world: &World, database: &Database, squad_id: EntityId) -> Vec3 {
    world
        .effective_squad_prototype(squad_id, database)
        .and_then(|prototype| prototype.hp_bar.as_ref())
        .and_then(|hp_bar| hp_bar.offset)
        .map(|offset| Vec3::new(offset.x, offset.y, offset.z))
        .filter(|offset| offset.is_finite())
        .unwrap_or(Vec3::ZERO)
}

impl RepairProfile {
    fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, RepairPowerError> {
        validate_level(attributes, level)?;
        let never_stops = optional_bool(attributes, level, "NeverStops").unwrap_or(false);
        let tick_duration = required_float(attributes, level, "TickDuration")?;
        let tick_duration_ms = seconds_to_milliseconds(tick_duration);
        let radius = required_float(attributes, level, "RepairRadius")?;
        let repair_per_tick = required_float(attributes, level, "RepairCombatValuePerTick")?;
        let cooldown = required_float(attributes, level, "CooldownTimeIfDamaged")?;
        if radius <= 0.0 {
            return Err(RepairPowerError::InvalidData("RepairRadius"));
        }
        if tick_duration <= 0.0 || tick_duration_ms == 0 {
            return Err(RepairPowerError::InvalidData("TickDuration"));
        }
        if repair_per_tick <= 0.0 {
            return Err(RepairPowerError::InvalidData("RepairCombatValuePerTick"));
        }
        if cooldown < 0.0 {
            return Err(RepairPowerError::InvalidData("CooldownTimeIfDamaged"));
        }
        let ticks = resolve_ticks(attributes, level, never_stops)?;
        let repair_attachment_prototype =
            optional_prototype(database, attributes, level, "RepairAttachment");
        let repair_attachment_prototype_id = repair_attachment_prototype
            .as_deref()
            .and_then(|name| prototype_id(database, name));
        let repair_object_prototype =
            required_prototype(database, attributes, level, "RepairObject")?;
        let filter_type = required_object_type(database, attributes, level, "FilterType")?;
        let mut flags = RepairFlags::default();
        flags.set(
            RepairFlags::SPREAD_AMONG_SQUADS,
            required_bool(attributes, level, "SpreadAmongSquads")?,
        );
        flags.set(
            RepairFlags::ALLOW_REINFORCE,
            required_bool(attributes, level, "AllowReinforce")?,
        );
        flags.set(
            RepairFlags::IGNORE_PLACEMENT,
            optional_bool(attributes, level, "IgnorePlacement").unwrap_or(false),
        );
        flags.set(
            RepairFlags::HEAL_ANY,
            required_bool(attributes, level, "HealAny")?,
        );
        flags.set(RepairFlags::NEVER_STOPS, never_stops);
        Ok(Self {
            repair_object_prototype,
            repair_attachment_prototype,
            repair_attachment_prototype_id,
            filter_type,
            radius,
            tick_duration_ms,
            ticks,
            repair_combat_value_per_tick: repair_per_tick,
            cooldown_time_if_damaged_ms: seconds_to_milliseconds(cooldown),
            flags,
        })
    }
}

fn resolve_ticks(
    attributes: &PowerAttributes,
    level: u32,
    never_stops: bool,
) -> Result<u32, RepairPowerError> {
    if never_stops {
        return Ok(0);
    }
    let ticks = required_int(attributes, level, "NumTicks")?;
    u32::try_from(ticks)
        .ok()
        .filter(|ticks| *ticks > 0)
        .ok_or(RepairPowerError::InvalidData("NumTicks"))
}

fn prototype_id(database: &Database, name: &str) -> Option<i32> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, prototype)| prototype.name.trim().eq_ignore_ascii_case(name.trim()))
        .map(|(index, prototype)| {
            prototype
                .dbid
                .unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
        })
}

fn hash_optional_string(checksum: &mut SyncChecksum, value: Option<&str>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        hash_string(checksum, value);
    } else {
        checksum.hash_u32(0);
    }
}

fn hash_entity_ids(checksum: &mut SyncChecksum, values: &[EntityId]) {
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_u32(value.as_u32());
    }
}

#[cfg(test)]
mod tests;
