//! Retail `BPowerODST` delayed drop lifecycle and synchronized live input.

use super::common::{
    PowerPayment, consume_power, hash_string, required_float, required_prototype, validate_level,
    validate_power_type, validate_requirements,
};
use super::{
    NativePowerError, NativePowerInput, NativePowerInvocation, PowerExecutionId, power_by_id,
};
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::entities::Projectile;
use crate::entities::projectiles::ProjectileLaunch;
use crate::gameplay::{AreaDamageProfile, GameplayCatalog};
use crate::player::{PlayerId, ProtoPowerId};
use crate::scenario::population::squad_population_costs;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use crate::sync::SyncChecksum;
use crate::world::{GeneralEvent, GeneralEventType, World};
use glam::Vec3;
use pipeline::database::hw1::powers::PowerAttributes;
use pipeline::database::hw1::tactics::Weapon;
use pipeline::database::hw1::{Database, Power, Squad as ProtoSquad};

const ODST_POWER_TYPE: u32 = 9;
const ODST_SQUAD: &str = "unsc_inf_odst_01";
const DROP_POD_OFFSET: Vec3 = Vec3::new(10.0, 80.0, 10.0);

/// Inputs accepted by retail's direct `InvokePower2` ODST path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OdstPowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
    pub power_user_id: PowerUserId,
}

impl From<OdstPowerInvocation> for NativePowerInvocation {
    fn from(invocation: OdstPowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
            power_user_id: invocation.power_user_id,
        }
    }
}

/// Error returned by ODST invocation.
pub type OdstPowerError = NativePowerError;

/// One squad created immediately but hidden until its drop pod arrives.
#[derive(Debug, Clone)]
pub struct OdstDrop {
    squad_id: EntityId,
    projectile_id: EntityId,
    target_location: Vec3,
    reveal_at_seconds: f32,
    projectile_launched: bool,
}

impl OdstDrop {
    #[must_use]
    pub const fn squad_id(&self) -> EntityId {
        self.squad_id
    }

    #[must_use]
    pub const fn projectile_id(&self) -> EntityId {
        self.projectile_id
    }

    #[must_use]
    pub const fn target_location(&self) -> Vec3 {
        self.target_location
    }

    #[must_use]
    pub const fn reveal_at_seconds(&self) -> f32 {
        self.reveal_at_seconds
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.squad_id.as_u32());
        checksum.hash_u32(self.projectile_id.as_u32());
        checksum.hash_vec3(
            self.target_location.x,
            self.target_location.y,
            self.target_location.z,
        );
        checksum.hash_f32(self.reveal_at_seconds);
        checksum.hash_u32(u32::from(self.projectile_launched));
    }
}

/// Authoritative state for one running retail `BPowerODST` session.
#[derive(Debug, Clone)]
pub struct OdstPowerExecution {
    id: PowerExecutionId,
    player_id: PlayerId,
    proto_power_id: ProtoPowerId,
    power_level: u32,
    owner_squad_id: EntityId,
    power_user_id: PowerUserId,
    target_location: Vec3,
    squad_spawn_delay: f32,
    projectile_prototype: String,
    odst_squad_prototype: String,
    odst_squad_prototype_id: i32,
    elapsed_seconds: f32,
    ready_for_shutdown: bool,
    ignore_requirements: bool,
    active_drops: Vec<OdstDrop>,
}

impl OdstPowerExecution {
    #[must_use]
    pub const fn id(&self) -> PowerExecutionId {
        self.id
    }

    #[must_use]
    pub const fn player_id(&self) -> PlayerId {
        self.player_id
    }

    #[must_use]
    pub const fn proto_power_id(&self) -> ProtoPowerId {
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
    pub const fn power_user_id(&self) -> PowerUserId {
        self.power_user_id
    }

    #[must_use]
    pub const fn target_location(&self) -> Vec3 {
        self.target_location
    }

    #[must_use]
    pub const fn squad_spawn_delay(&self) -> f32 {
        self.squad_spawn_delay
    }

    #[must_use]
    pub fn projectile_prototype(&self) -> &str {
        &self.projectile_prototype
    }

    #[must_use]
    pub fn odst_squad_prototype(&self) -> &str {
        &self.odst_squad_prototype
    }

    #[must_use]
    pub const fn elapsed_seconds(&self) -> f32 {
        self.elapsed_seconds
    }

    #[must_use]
    pub const fn ready_for_shutdown(&self) -> bool {
        self.ready_for_shutdown
    }

    #[must_use]
    pub fn active_drops(&self) -> &[OdstDrop] {
        &self.active_drops
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id.get());
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_u32(self.power_level);
        checksum.hash_u32(self.owner_squad_id.as_u32());
        checksum.hash_u32(self.power_user_id.raw());
        checksum.hash_vec3(
            self.target_location.x,
            self.target_location.y,
            self.target_location.z,
        );
        checksum.hash_f32(self.squad_spawn_delay);
        hash_string(checksum, &self.projectile_prototype);
        hash_string(checksum, &self.odst_squad_prototype);
        checksum.hash_i32(self.odst_squad_prototype_id);
        checksum.hash_f32(self.elapsed_seconds);
        checksum.hash_u32(u32::from(self.ready_for_shutdown));
        checksum.hash_u32(u32::from(self.ignore_requirements));
        checksum.hash_u32(u32::try_from(self.active_drops.len()).unwrap_or(u32::MAX));
        for drop in &self.active_drops {
            drop.hash_state(checksum);
        }
    }
}

#[derive(Debug)]
struct OdstProfile {
    squad_spawn_delay: f32,
    projectile_prototype: String,
    odst_squad_prototype_id: i32,
}

impl World {
    /// Validate and begin one authoritative interactive ODST drop session.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed shipped data, ownership, or a mismatched
    /// packed power-user ID. Payment and population checks happen per drop.
    pub fn invoke_odst_power(
        &mut self,
        database: &Database,
        invocation: OdstPowerInvocation,
    ) -> Result<PowerExecutionId, OdstPowerError> {
        invoke(self, database, invocation.into())
    }

    /// Submit live input to one ODST session by stable simulation execution ID.
    pub fn submit_odst_power_input(
        &mut self,
        database: &Database,
        execution_id: PowerExecutionId,
        input: NativePowerInput,
        no_cost: bool,
    ) -> bool {
        submit_input_by_execution(self, database, execution_id, input, no_cost)
    }
}

pub(super) fn invoke(
    world: &mut World,
    database: &Database,
    invocation: NativePowerInvocation,
) -> Result<PowerExecutionId, NativePowerError> {
    if !invocation.target_location.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    validate_power_user(invocation)?;
    let power = power_by_id(database, invocation.proto_power_id)
        .ok_or(NativePowerError::PowerNotFound(invocation.proto_power_id))?;
    let attributes = validate_power_type(power, "ODST")?;
    let profile = OdstProfile::resolve(database, attributes, invocation.power_level)?;
    validate_activation(world, power, attributes, invocation)?;
    let id = world.power_manager.allocate_id();
    world
        .power_manager
        .odst_executions
        .push(create_execution(id, invocation, profile));
    Ok(id)
}

fn validate_power_user(invocation: NativePowerInvocation) -> Result<(), NativePowerError> {
    let id = invocation.power_user_id;
    if id.is_valid()
        && (id.player_id() != i32::from(invocation.player_id) || id.power_type() != ODST_POWER_TYPE)
    {
        return Err(NativePowerError::InvalidData("PowerUserID"));
    }
    Ok(())
}

fn validate_activation(
    world: &World,
    _power: &Power,
    attributes: &PowerAttributes,
    invocation: NativePowerInvocation,
) -> Result<(), NativePowerError> {
    let player = world
        .get_player(invocation.player_id)
        .ok_or(NativePowerError::PlayerNotFound(invocation.player_id))?;
    if invocation.ignore_requirements {
        return Ok(());
    }
    let entry = player
        .power_entry(invocation.proto_power_id)
        .ok_or(NativePowerError::PowerUnavailable)?;
    if !entry.has_available_uses() {
        return Err(NativePowerError::PowerUnavailable);
    }
    if !entry.ignores_tech_prerequisites() {
        for prerequisite in &attributes.tech_prerequisites {
            if !player.technologies.is_active(prerequisite) {
                return Err(NativePowerError::MissingTechnology(prerequisite.clone()));
            }
        }
    }
    Ok(())
}

fn create_execution(
    id: PowerExecutionId,
    invocation: NativePowerInvocation,
    profile: OdstProfile,
) -> OdstPowerExecution {
    OdstPowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        power_user_id: invocation.power_user_id,
        target_location: invocation.target_location,
        squad_spawn_delay: profile.squad_spawn_delay,
        projectile_prototype: profile.projectile_prototype,
        odst_squad_prototype: ODST_SQUAD.to_owned(),
        odst_squad_prototype_id: profile.odst_squad_prototype_id,
        elapsed_seconds: 0.0,
        ready_for_shutdown: false,
        ignore_requirements: invocation.ignore_requirements,
        active_drops: Vec::new(),
    }
}

pub(super) fn submit_input_by_user(
    world: &mut World,
    database: &Database,
    power_user_id: PowerUserId,
    input: NativePowerInput,
    no_cost: bool,
) -> bool {
    if !power_user_id.is_valid() {
        return false;
    }
    submit_input(world, database, input, no_cost, |execution| {
        execution.power_user_id == power_user_id
    })
}

fn submit_input_by_execution(
    world: &mut World,
    database: &Database,
    execution_id: PowerExecutionId,
    input: NativePowerInput,
    no_cost: bool,
) -> bool {
    submit_input(world, database, input, no_cost, |execution| {
        execution.id == execution_id
    })
}

fn submit_input(
    world: &mut World,
    database: &Database,
    input: NativePowerInput,
    no_cost: bool,
    matches: impl Fn(&OdstPowerExecution) -> bool,
) -> bool {
    let active = std::mem::take(&mut world.power_manager.odst_executions);
    let mut remaining = Vec::with_capacity(active.len());
    let mut accepted = false;
    for mut execution in active {
        if !accepted && matches(&execution) {
            accepted = handle_input(world, database, &mut execution, input, no_cost);
        }
        remaining.push(execution);
    }
    world.power_manager.odst_executions = remaining;
    accepted
}

fn handle_input(
    world: &mut World,
    database: &Database,
    execution: &mut OdstPowerExecution,
    input: NativePowerInput,
    no_cost: bool,
) -> bool {
    match input {
        NativePowerInput::Confirm(location) => {
            confirm_drop(world, database, execution, location, no_cost)
        }
        NativePowerInput::Shutdown => {
            execution.ready_for_shutdown = true;
            true
        }
        NativePowerInput::Position(_) | NativePowerInput::Direction(_) => false,
    }
}

fn confirm_drop(
    world: &mut World,
    database: &Database,
    execution: &mut OdstPowerExecution,
    location: Vec3,
    no_cost: bool,
) -> bool {
    if execution.ready_for_shutdown {
        return false;
    }
    let Some(power) = power_by_id(database, execution.proto_power_id) else {
        return false;
    };
    let Some(attributes) = power.attributes.as_ref() else {
        return false;
    };
    let Ok(location) = validate_drop_location(world, attributes, location) else {
        return false;
    };
    let ignore_requirements = execution.ignore_requirements || no_cost;
    let payment = if ignore_requirements {
        PowerPayment::default()
    } else {
        let Ok(payment) = validate_requirements(
            world,
            database,
            power,
            execution.player_id,
            execution.proto_power_id,
        ) else {
            return false;
        };
        if !drop_population_fits(world, database, execution) {
            return false;
        }
        payment
    };
    if !ignore_requirements
        && consume_power(
            world,
            power,
            execution.player_id,
            execution.proto_power_id,
            execution.owner_squad_id,
            &payment,
        )
        .is_err()
    {
        return false;
    }
    let Some(squad_id) = spawn_hidden_squad(world, database, execution, location) else {
        return false;
    };
    execution.target_location = location;
    execution.active_drops.push(OdstDrop {
        squad_id,
        projectile_id: EntityId::INVALID,
        target_location: location,
        reveal_at_seconds: execution.elapsed_seconds + execution.squad_spawn_delay,
        projectile_launched: false,
    });
    let _fired = world.fire_general_event(&GeneralEvent::new(
        GeneralEventType::UsedPower,
        i32::from(execution.player_id),
    ));
    true
}

fn validate_drop_location(
    world: &World,
    attributes: &PowerAttributes,
    mut location: Vec3,
) -> Result<Vec3, NativePowerError> {
    if !location.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    if world.is_outside_playable_bounds(location, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(id) = super::disruption::disrupting_power_at(world, attributes, location) {
        return Err(NativePowerError::Disrupted(id));
    }
    if let Some(height) = world.terrain_height(location, true) {
        location.y = height;
    }
    Ok(location)
}

fn drop_population_fits(
    world: &World,
    database: &Database,
    execution: &OdstPowerExecution,
) -> bool {
    let Some(player) = world.get_player(execution.player_id) else {
        return false;
    };
    let Some(entry) = player.power_entry(execution.proto_power_id) else {
        return false;
    };
    if entry.ignores_population() {
        return true;
    }
    let Some(prototype) = effective_odst_squad(player, database) else {
        return false;
    };
    player.can_reserve_population(&squad_population_costs(database, prototype))
}

fn effective_odst_squad<'database>(
    player: &crate::player::Player,
    database: &'database Database,
) -> Option<&'database ProtoSquad> {
    let effective = player.technologies.resolved_squad_prototype(ODST_SQUAD);
    database
        .squads
        .iter()
        .find(|squad| squad.name.eq_ignore_ascii_case(effective))
        .or_else(|| {
            database
                .squads
                .iter()
                .find(|squad| squad.name.eq_ignore_ascii_case(ODST_SQUAD))
        })
}

fn spawn_hidden_squad(
    world: &mut World,
    database: &Database,
    execution: &OdstPowerExecution,
    location: Vec3,
) -> Option<EntityId> {
    let squad_id = spawn_squad_at(
        world,
        database,
        execution.player_id,
        execution.odst_squad_prototype_id,
        location,
        Vec3::Z,
    )
    .ok()?;
    let unit_ids = world.get_squad(squad_id)?.unit_ids.clone();
    let _selectable = world.set_entity_selectable(squad_id, false);
    for unit_id in unit_ids {
        let _render = world.set_entity_render_enabled(unit_id, false);
        if let Some(unit) = world.get_unit_mut(unit_id) {
            unit.set_invulnerable(true);
        }
    }
    Some(squad_id)
}

pub(super) fn update(
    world: &mut World,
    dt: f32,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
) {
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let active = std::mem::take(&mut world.power_manager.odst_executions);
    let mut remaining = Vec::with_capacity(active.len());
    for mut execution in active {
        execution.elapsed_seconds += dt;
        launch_pending_projectiles(world, database, gameplay, &mut execution);
        reveal_arrived_squads(world, &mut execution);
        if !execution.ready_for_shutdown || !execution.active_drops.is_empty() {
            remaining.push(execution);
        }
    }
    world.power_manager.odst_executions = remaining;
}

fn launch_pending_projectiles(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut OdstPowerExecution,
) {
    for drop in &mut execution.active_drops {
        if drop.projectile_launched {
            continue;
        }
        drop.projectile_launched = true;
        drop.projectile_id = launch_drop_projectile(
            world,
            database,
            gameplay,
            execution.player_id,
            &execution.projectile_prototype,
            drop.target_location,
        );
    }
}

fn launch_drop_projectile(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    player_id: PlayerId,
    prototype: &str,
    target: Vec3,
) -> EntityId {
    let Some(profile) = crate::gameplay::projectiles::projectile_profile(database, prototype)
    else {
        return EntityId::INVALID;
    };
    let source = target + DROP_POD_OFFSET;
    let weapon = projectile_weapon(gameplay, prototype);
    let max_range = weapon
        .and_then(|weapon| finite_positive(weapon.max_range))
        .unwrap_or_else(|| source.distance(target).max(1.0));
    let launch = ProjectileLaunch {
        source_id: EntityId::INVALID,
        target_id: EntityId::INVALID,
        source_position: source,
        target_position: target,
        target_entity_position: target,
        target_offset: Vec3::ZERO,
        target_radius: 0.0,
        max_range,
        damage: weapon
            .and_then(|weapon| finite_nonnegative(weapon.damage_per_second))
            .unwrap_or_default(),
        weapon_type: weapon.and_then(|weapon| weapon.weapon_type.clone()),
        area_damage: weapon.and_then(area_damage_profile),
        friendly_fire: weapon.is_some_and(|weapon| weapon.allow_friendly_fire == Some(true)),
        collides_with_all_units: true,
    };
    let id = world.projectiles.allocate_id();
    world
        .projectiles
        .insert(id, Projectile::new(id, player_id, launch, &profile));
    id
}

fn projectile_weapon<'gameplay>(
    gameplay: Option<&'gameplay GameplayCatalog>,
    prototype: &str,
) -> Option<&'gameplay Weapon> {
    let tactics = gameplay?.object(prototype)?.tactics();
    tactics
        .actions
        .iter()
        .filter_map(|action| action.weapon.as_deref())
        .find_map(|name| {
            tactics
                .weapons
                .iter()
                .find(|weapon| weapon.name.eq_ignore_ascii_case(name))
        })
        .or_else(|| tactics.weapons.first())
}

fn area_damage_profile(weapon: &Weapon) -> Option<AreaDamageProfile> {
    let radius = finite_nonnegative(weapon.aoe_radius)?;
    (radius > 0.0).then(|| AreaDamageProfile {
        radius,
        primary_target_factor: finite_or_zero(weapon.aoe_primary_target_factor),
        distance_factor: finite_or_zero(weapon.aoe_distance_factor),
        damage_factor: finite_or_zero(weapon.aoe_damage_factor),
        linear_damage: weapon.aoe_linear_damage == Some(true),
        ignores_y_axis: weapon.aoe_ignores_y_axis == Some(true),
        friendly_fire: weapon.allow_friendly_fire == Some(true),
    })
}

fn reveal_arrived_squads(world: &mut World, execution: &mut OdstPowerExecution) {
    let mut index = 0;
    while index < execution.active_drops.len() {
        if execution.elapsed_seconds <= execution.active_drops[index].reveal_at_seconds {
            index += 1;
            continue;
        }
        let drop = execution.active_drops.remove(index);
        reveal_squad(world, drop.squad_id);
    }
}

fn reveal_squad(world: &mut World, squad_id: EntityId) {
    let unit_ids = world
        .get_squad(squad_id)
        .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
    let _selectable = world.set_entity_selectable(squad_id, true);
    for unit_id in unit_ids {
        let _render = world.set_entity_render_enabled(unit_id, true);
        if let Some(unit) = world.get_unit_mut(unit_id) {
            unit.set_invulnerable(false);
        }
    }
}

impl OdstProfile {
    fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, NativePowerError> {
        validate_level(attributes, level)?;
        let squad_spawn_delay = required_float(attributes, level, "SquadSpawnDelay")?;
        if squad_spawn_delay < 0.0 {
            return Err(NativePowerError::InvalidData("SquadSpawnDelay"));
        }
        let projectile_prototype = required_prototype(database, attributes, level, "Projectile")?;
        if crate::gameplay::projectiles::projectile_profile(database, &projectile_prototype)
            .is_none()
        {
            return Err(NativePowerError::InvalidData("Projectile"));
        }
        let odst_squad_prototype_id = squad_prototype_id(database, ODST_SQUAD)
            .ok_or_else(|| NativePowerError::UnknownSquadPrototype(ODST_SQUAD.to_owned()))?;
        Ok(Self {
            squad_spawn_delay,
            projectile_prototype,
            odst_squad_prototype_id,
        })
    }
}

fn finite_positive(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

fn finite_nonnegative(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

#[cfg(test)]
mod tests;
