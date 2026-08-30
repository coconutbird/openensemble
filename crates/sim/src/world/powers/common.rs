//! Shared native-power profile, payment, bomber, and visual-object machinery.

use super::{NativePowerError, rules};
use crate::EntityId;
use crate::entities::Object;
use crate::player::{PlayerId, PopulationCost, ProtoPowerId, Resources};
use crate::sync::SyncChecksum;
use crate::world::World;
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::{Database, Power, ProtoObject};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PowerPayment {
    cost: Resources,
    pay_cost: bool,
}

impl PowerPayment {
    pub(super) fn hash_state(self, checksum: &mut SyncChecksum) {
        for amount in self.cost.amounts {
            checksum.hash_f32(amount);
        }
        checksum.hash_u32(u32::from(self.pay_cost));
    }
}

#[derive(Debug, Clone)]
pub(super) struct BomberProfile {
    pub prototype: String,
    pub bomb_time: f32,
    pub flyin_distance: f32,
    pub flyin_height: f32,
    pub bomb_height: f32,
    pub speed: f32,
    pub flyout_time: f32,
}

impl BomberProfile {
    pub(super) fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, NativePowerError> {
        Ok(Self {
            prototype: required_prototype(database, attributes, level, "Bomber")?,
            bomb_time: required_float(attributes, level, "BomberBombTime")?,
            flyin_distance: required_float(attributes, level, "BomberFlyinDistance")?,
            flyin_height: required_float(attributes, level, "BomberFlyinHeight")?,
            bomb_height: required_float(attributes, level, "BomberBombHeight")?,
            speed: required_float(attributes, level, "BomberSpeed")?,
            flyout_time: required_float(attributes, level, "BomberFlyOutTime")?,
        })
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_string(checksum, &self.prototype);
        checksum.hash_f32(self.bomb_time);
        checksum.hash_f32(self.flyin_distance);
        checksum.hash_f32(self.flyin_height);
        checksum.hash_f32(self.bomb_height);
        checksum.hash_f32(self.speed);
        checksum.hash_f32(self.flyout_time);
    }
}

/// Shared state serialized by each retail native power's `BBomberData`.
#[derive(Debug, Clone)]
pub(crate) struct Bomber {
    object_id: EntityId,
    prototype: String,
    position: Vec3,
    bomb_time: f32,
    flyin_distance: f32,
    flyin_height: f32,
    bomb_height: f32,
    speed: f32,
    flyout_time: f32,
    additional_height: f32,
    visible: bool,
}

impl Bomber {
    pub(super) fn spawn(
        world: &mut World,
        database: &Database,
        player_id: PlayerId,
        target: Vec3,
        direction: Vec3,
        profile: BomberProfile,
    ) -> Self {
        let mut position = target - direction * profile.flyin_distance;
        let terrain_height = world.terrain_height(position, true).unwrap_or(target.y);
        position.y = terrain_height + profile.flyin_height;
        let object_id = create_power_visual(
            world,
            database,
            player_id,
            position,
            direction,
            &profile.prototype,
        );
        Self {
            object_id,
            prototype: profile.prototype,
            position,
            bomb_time: profile.bomb_time,
            flyin_distance: profile.flyin_distance,
            flyin_height: profile.flyin_height,
            bomb_height: profile.bomb_height,
            speed: profile.speed,
            flyout_time: profile.flyout_time,
            additional_height: 0.0,
            visible: !object_id.is_invalid(),
        }
    }

    pub(super) fn update(
        &mut self,
        world: &mut World,
        target: Vec3,
        direction: Vec3,
        elapsed_seconds: f32,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0.0 || !self.visible {
            return;
        }
        let Some(previous) = world
            .get_object(self.object_id)
            .map(|object| object.base.position)
        else {
            self.visible = false;
            return;
        };
        self.position += direction * self.speed * dt;
        let terrain_height = world
            .terrain_height(self.position, true)
            .unwrap_or(target.y);
        self.position.y = terrain_height
            + self.additional_height
            + self.bomb_height
            + self.curve_height(elapsed_seconds);
        self.visible = elapsed_seconds < self.flyout_time + self.bomb_time;
        if !self.visible {
            let _removed = world.remove_object(self.object_id);
            return;
        }
        let delta = self.position - previous;
        let forward = if delta.length_squared() > f32::EPSILON {
            delta.normalize()
        } else {
            direction
        };
        if let Some(bomber) = world.get_object_mut(self.object_id) {
            bomber.base.set_position(self.position);
            bomber.base.set_forward(forward);
        }
    }

    pub(super) fn kill(&mut self, world: &mut World) {
        let _removed = world.remove_object(self.object_id);
        self.visible = false;
    }

    pub(super) const fn object_id(&self) -> EntityId {
        self.object_id
    }

    pub(super) fn prototype(&self) -> &str {
        &self.prototype
    }

    pub(super) const fn position(&self) -> Vec3 {
        self.position
    }

    pub(super) const fn bomb_time(&self) -> f32 {
        self.bomb_time
    }

    pub(super) const fn visible(&self) -> bool {
        self.visible
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.object_id.as_u32());
        hash_string(checksum, &self.prototype);
        checksum.hash_vec3(self.position.x, self.position.y, self.position.z);
        checksum.hash_f32(self.bomb_time);
        checksum.hash_f32(self.flyin_distance);
        checksum.hash_f32(self.flyin_height);
        checksum.hash_f32(self.bomb_height);
        checksum.hash_f32(self.speed);
        checksum.hash_f32(self.flyout_time);
        checksum.hash_f32(self.additional_height);
        checksum.hash_u32(u32::from(self.visible));
    }

    fn curve_height(&self, elapsed_seconds: f32) -> f32 {
        let height_delta = self.flyin_height - self.bomb_height;
        if self.bomb_time <= 0.0 {
            return 0.0;
        }
        if elapsed_seconds < self.bomb_time {
            let ratio = (self.bomb_time - elapsed_seconds) / self.bomb_time;
            return ratio * ratio * height_delta;
        }
        if elapsed_seconds < self.flyout_time {
            return 0.0;
        }
        let ratio = ((elapsed_seconds - self.flyout_time) / self.bomb_time).min(1.0);
        ratio * ratio * height_delta
    }
}

pub(super) fn validate_requirements(
    world: &World,
    database: &Database,
    power: &Power,
    player_id: PlayerId,
    proto_power_id: ProtoPowerId,
) -> Result<PowerPayment, NativePowerError> {
    let player = world
        .get_player(player_id)
        .ok_or(NativePowerError::PlayerNotFound(player_id))?;
    let entry = player
        .power_entry(proto_power_id)
        .ok_or(NativePowerError::PowerUnavailable)?;
    if !entry.has_available_uses() {
        return Err(NativePowerError::PowerUnavailable);
    }
    let attributes = power
        .attributes
        .as_ref()
        .ok_or(NativePowerError::MissingData("Attributes"))?;
    let cost = power_cost(database, attributes.cost.as_ref())?;
    if !entry.ignores_cost() && !player.resources.can_afford(&cost) {
        return Err(NativePowerError::InsufficientResources);
    }
    if !entry.ignores_tech_prerequisites() {
        for prerequisite in &attributes.tech_prerequisites {
            if !player.technologies.is_active(prerequisite) {
                return Err(NativePowerError::MissingTechnology(prerequisite.clone()));
            }
        }
    }
    if !entry.ignores_population() {
        let population = power_population(database, attributes)?;
        if !player.can_reserve_population(&population) {
            return Err(NativePowerError::PopulationLimit);
        }
    }
    Ok(PowerPayment {
        cost,
        pay_cost: !entry.ignores_cost(),
    })
}

pub(super) fn consume_power(
    world: &mut World,
    power: &Power,
    player_id: PlayerId,
    proto_power_id: ProtoPowerId,
    squad_id: EntityId,
    payment: &PowerPayment,
) -> Result<(), NativePowerError> {
    let power_rules = rules(power);
    let auto_recharge_ms = power
        .attributes
        .as_ref()
        .and_then(|attributes| attributes.auto_recharge)
        .unwrap_or_default();
    let game_time_ms = world.game_time_ms;
    let player = world
        .get_player_mut(player_id)
        .ok_or(NativePowerError::PlayerNotFound(player_id))?;
    if !player.consume_power_use(
        proto_power_id,
        squad_id,
        power_rules,
        auto_recharge_ms,
        game_time_ms,
    ) {
        return Err(NativePowerError::PowerUnavailable);
    }
    if payment.pay_cost {
        player.resources.pay(&payment.cost);
    }
    Ok(())
}

pub(super) fn random_direction(world: &mut World) -> Vec3 {
    let mut direction = Vec3::new(
        world.trigger_random_float(-1.0, 1.0),
        0.0,
        world.trigger_random_float(-1.0, 1.0),
    );
    if direction.length_squared() <= f32::EPSILON {
        direction = Vec3::X;
    }
    direction.normalize()
}

pub(super) fn create_power_visual(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    prototype_name: &str,
) -> EntityId {
    let Some((prototype_index, prototype)) = database
        .objects
        .iter()
        .enumerate()
        .find(|(_, prototype)| prototype.name.eq_ignore_ascii_case(prototype_name))
    else {
        return EntityId::INVALID;
    };
    let prototype_id = prototype
        .dbid
        .unwrap_or_else(|| i32::try_from(prototype_index).unwrap_or(-1));
    let object_id = world.objects.allocate_id();
    world.objects.insert(
        object_id,
        Object::new_visual(
            object_id,
            player_id,
            position,
            forward,
            prototype_id,
            prototype.name.clone(),
        ),
    );
    if let Some(lifetime_ms) = prototype_lifetime_ms(prototype) {
        world
            .power_manager
            .track_visual(object_id, world.game_time_ms.wrapping_add(lifetime_ms));
    }
    object_id
}

pub(super) fn attach_power_visual(
    world: &mut World,
    database: &Database,
    parent_id: EntityId,
    prototype_name: &str,
) -> EntityId {
    let Some((owner, position, forward)) = world.get_object(parent_id).map(|parent| {
        (
            parent.base.player_id,
            parent.base.position,
            parent.base.forward,
        )
    }) else {
        return EntityId::INVALID;
    };
    let child_id = create_power_visual(world, database, owner, position, forward, prototype_name);
    if child_id.is_invalid() {
        return child_id;
    }
    if let Some(child) = world.entity_object_state_mut(child_id) {
        child.set_attached_to(Some(parent_id));
    }
    if let Some(parent) = world.entity_object_state_mut(parent_id) {
        parent.add_attachment(child_id);
    }
    child_id
}

pub(super) fn finish_power_visual(world: &mut World, object_id: EntityId, lifetime_ms: u32) {
    if world.get_object(object_id).is_none() {
        return;
    }
    let _started = world.play_entity_animation(object_id, "Death".to_owned(), None, lifetime_ms);
    world
        .power_manager
        .track_visual(object_id, world.game_time_ms.wrapping_add(lifetime_ms));
}

pub(super) fn expire_transient_visuals(world: &mut World) {
    let current_time = world.game_time_ms;
    let visuals = std::mem::take(&mut world.power_manager.transient_visuals);
    let mut remaining = Vec::with_capacity(visuals.len());
    for visual in visuals {
        if world.get_object(visual.object_id).is_none() {
            continue;
        }
        if current_time >= visual.expires_at_ms {
            let _removed = world.remove_object(visual.object_id);
        } else {
            remaining.push(visual);
        }
    }
    world.power_manager.transient_visuals = remaining;
}

pub(super) fn validate_power_type<'power>(
    power: &'power Power,
    expected: &'static str,
) -> Result<&'power PowerAttributes, NativePowerError> {
    let attributes = power
        .attributes
        .as_ref()
        .ok_or(NativePowerError::MissingData("Attributes"))?;
    if !attributes
        .power_type
        .as_deref()
        .is_some_and(|power_type| power_type.eq_ignore_ascii_case(expected))
    {
        return Err(NativePowerError::WrongPowerType {
            power: power.name.clone(),
            expected,
        });
    }
    Ok(attributes)
}

pub(super) fn validate_level(
    attributes: &PowerAttributes,
    level: u32,
) -> Result<(), NativePowerError> {
    let level_count = attributes
        .data_levels
        .iter()
        .filter_map(|data_level| data_level.level)
        .max()
        .map_or(0, |maximum| maximum.saturating_add(1));
    if level >= level_count {
        return Err(NativePowerError::InvalidPowerLevel(level));
    }
    Ok(())
}

pub(super) fn required_float(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<f32, NativePowerError> {
    let value = required_entry(attributes, level, name, "float")?
        .value
        .trim()
        .parse::<f32>()
        .map_err(|_| NativePowerError::InvalidData(name))?;
    if !value.is_finite() {
        return Err(NativePowerError::InvalidData(name));
    }
    Ok(value)
}

pub(super) fn optional_float(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<Option<f32>, NativePowerError> {
    let Some(entry) = optional_entry(attributes, level, name, "float") else {
        return Ok(None);
    };
    let value = entry
        .value
        .trim()
        .parse::<f32>()
        .map_err(|_| NativePowerError::InvalidData(name))?;
    value
        .is_finite()
        .then_some(Some(value))
        .ok_or(NativePowerError::InvalidData(name))
}

pub(super) fn required_int(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<i32, NativePowerError> {
    required_entry(attributes, level, name, "int")?
        .value
        .trim()
        .parse()
        .map_err(|_| NativePowerError::InvalidData(name))
}

pub(super) fn optional_int(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<Option<i32>, NativePowerError> {
    optional_entry(attributes, level, name, "int")
        .map(|entry| {
            entry
                .value
                .trim()
                .parse()
                .map_err(|_| NativePowerError::InvalidData(name))
        })
        .transpose()
}

pub(super) fn required_bool(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<bool, NativePowerError> {
    parse_bool(&required_entry(attributes, level, name, "bool")?.value)
        .ok_or(NativePowerError::InvalidData(name))
}

pub(super) fn optional_bool(attributes: &PowerAttributes, level: u32, name: &str) -> Option<bool> {
    optional_entry(attributes, level, name, "bool").and_then(|entry| parse_bool(&entry.value))
}

pub(super) fn required_string(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
    data_type: &str,
) -> Result<String, NativePowerError> {
    let value = required_entry(attributes, level, name, data_type)?
        .value
        .trim();
    if value.is_empty() {
        return Err(NativePowerError::InvalidData(name));
    }
    Ok(value.to_owned())
}

pub(super) fn required_prototype(
    database: &Database,
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<String, NativePowerError> {
    let requested = required_string(attributes, level, name, "protoobject")?;
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.trim().eq_ignore_ascii_case(&requested))
        .map(|prototype| prototype.name.clone())
        .ok_or(NativePowerError::UnknownPrototype(requested))
}

pub(super) fn optional_prototype(
    database: &Database,
    attributes: &PowerAttributes,
    level: u32,
    name: &str,
) -> Option<String> {
    let requested = optional_entry(attributes, level, name, "protoobject")?
        .value
        .trim();
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.trim().eq_ignore_ascii_case(requested))
        .map(|prototype| prototype.name.clone())
}

pub(super) fn required_object_type(
    database: &Database,
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<String, NativePowerError> {
    let requested = required_string(attributes, level, name, "objecttype")?;
    if object_type_exists(database, &requested) {
        Ok(requested)
    } else {
        Err(NativePowerError::UnknownObjectType(requested))
    }
}

pub(super) fn seconds_to_milliseconds(seconds: f32) -> u32 {
    let milliseconds = (seconds * 1_000.0).trunc();
    if milliseconds.is_sign_negative() {
        0
    } else {
        milliseconds.to_u32().unwrap_or(u32::MAX)
    }
}

pub(super) fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}

fn required_entry<'attributes>(
    attributes: &'attributes PowerAttributes,
    level: u32,
    name: &'static str,
    data_type: &str,
) -> Result<&'attributes DataEntry, NativePowerError> {
    optional_entry(attributes, level, name, data_type).ok_or(NativePowerError::MissingData(name))
}

fn optional_entry<'attributes>(
    attributes: &'attributes PowerAttributes,
    level: u32,
    name: &str,
    data_type: &str,
) -> Option<&'attributes DataEntry> {
    attributes
        .data_levels
        .iter()
        .find(|data_level| data_level.level == Some(level))
        .and_then(|data_level| find_entry(data_level, name, data_type))
        .or_else(|| {
            attributes
                .base_data_level
                .as_ref()
                .and_then(|data_level| find_entry(data_level, name, data_type))
        })
}

fn find_entry<'level>(
    level: &'level DataLevel,
    name: &str,
    data_type: &str,
) -> Option<&'level DataEntry> {
    level.entries.iter().find(|entry| {
        entry.name.eq_ignore_ascii_case(name) && entry.data_type.eq_ignore_ascii_case(data_type)
    })
}

fn parse_bool(value: &str) -> Option<bool> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("true") || value == "1" {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") || value == "0" {
        Some(false)
    } else {
        None
    }
}

fn power_cost(
    database: &Database,
    authored: Option<&PowerCost>,
) -> Result<Resources, NativePowerError> {
    let mut cost = Resources::new();
    let Some(authored) = authored else {
        return Ok(cost);
    };
    for (name, amount) in [
        ("Supplies", authored.supplies),
        ("Power", authored.power),
        ("CampaignFoo", authored.campaign_foo),
        ("Collectable", authored.collectable),
    ] {
        let Some(amount) = amount else {
            continue;
        };
        if !amount.is_finite() || amount < 0.0 {
            return Err(NativePowerError::InvalidData("Cost"));
        }
        if amount == 0.0 {
            continue;
        }
        let resource_id = resource_id(database, name)
            .filter(|resource_id| *resource_id < crate::player::MAX_RESOURCES)
            .ok_or(NativePowerError::InvalidData("Cost"))?;
        cost.set(resource_id, amount);
    }
    Ok(cost)
}

pub(super) fn resource_id(database: &Database, name: &str) -> Option<usize> {
    database
        .game_data
        .as_ref()?
        .resources
        .as_ref()?
        .entries
        .iter()
        .position(|entry| entry.name.trim().eq_ignore_ascii_case(name))
}

fn power_population(
    database: &Database,
    attributes: &PowerAttributes,
) -> Result<Vec<PopulationCost>, NativePowerError> {
    let mut result = Vec::new();
    for amount in &attributes.population {
        if !amount.amount.is_finite() || amount.amount < 0.0 {
            return Err(NativePowerError::InvalidData("Pop"));
        }
        if amount.amount == 0.0 {
            continue;
        }
        let Some(name) = amount
            .population_type
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            // `BProtoPower::load` ignores Pop elements without a type
            // attribute. Shipped ODST data contains exactly that legacy form.
            continue;
        };
        let Some(population_type) = crate::scenario::population::population_type_id(database, name)
        else {
            // Retail also ignores names that do not resolve through the pop
            // table instead of invalidating the complete power definition.
            continue;
        };
        result.push(PopulationCost::new(population_type, amount.amount));
    }
    Ok(result)
}

pub(super) fn prototype_lifetime_ms(prototype: &ProtoObject) -> Option<u32> {
    let lifespan = prototype
        .lifespan
        .filter(|lifespan| lifespan.is_finite() && *lifespan != 0.0)?;
    let delay = prototype
        .death_fade_delay_time
        .filter(|delay| delay.is_finite() && *delay > 0.0)
        .unwrap_or_default();
    let fade = if prototype
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("FadeOnDeath"))
    {
        prototype
            .death_fade_time
            .filter(|fade| fade.is_finite() && *fade > 0.0)
            .unwrap_or_default()
    } else {
        0.0
    };
    Some(seconds_to_milliseconds(lifespan + delay + fade))
}

fn object_type_exists(database: &Database, requested: &str) -> bool {
    database.objects.iter().any(|prototype| {
        prototype.name.eq_ignore_ascii_case(requested)
            || prototype
                .object_types
                .iter()
                .any(|object_type| object_type.eq_ignore_ascii_case(requested))
    }) || database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.code_object_types.as_ref())
        .is_some_and(|types| {
            types.entries.iter().any(|object_type| {
                object_type.object_type.eq_ignore_ascii_case(requested)
                    || object_type.value.eq_ignore_ascii_case(requested)
            })
        })
}
