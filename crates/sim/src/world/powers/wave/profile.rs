//! Database resolution for retail's `BPowerWave` profile.

use super::super::NativePowerError;
use super::super::common::{
    hash_string, optional_prototype, required_bool, required_float, required_int,
    required_prototype, required_string, resource_id, seconds_to_milliseconds, validate_level,
};
use crate::sync::SyncChecksum;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

#[derive(Debug, Clone)]
pub(super) struct WaveProfile {
    pub tick_length: f32,
    pub supplies_per_tick: f32,
    pub supplies_resource_id: usize,
    pub maximum_ball_speed_stagnant: f32,
    pub maximum_ball_speed_pulling: f32,
    pub explode_time: f32,
    pub pulling_range: f32,
    pub explosion_force_on_debris: f32,
    pub health_to_capture: f32,
    pub nudge_strength: f32,
    pub initial_lateral_pull_strength: f32,
    pub captured_radial_spacing: f32,
    pub captured_spring_strength: f32,
    pub captured_spring_dampening: f32,
    pub captured_spring_rest_length: f32,
    pub captured_minimum_lateral_speed: f32,
    pub rip_attachment_chance_pulling: f32,
    pub pickup_object_rate: f32,
    pub debris_angular_damping: f32,
    pub maximum_explosion_damage_bank_per_captured: f32,
    pub explosion_damage_bank_per_tick: f32,
    pub command_interval_ms: u32,
    pub minimum_ball_distance: f32,
    pub maximum_ball_distance: f32,
    pub maximum_captured_objects: usize,
    pub lightning_per_tick: usize,
    pub nudge_chance_pulling: u8,
    pub throw_part_chance_pulling: u8,
    pub lightning_chance_pulling: u8,
    pub ball_prototype: String,
    pub lightning_projectile: String,
    pub lightning_beam_visual: Option<String>,
    pub debris_projectile: String,
    pub explode_projectile: String,
    pub pickup_attachment: String,
    pub explode_sound: String,
    pub throw_units_on_explosion: bool,
    pub minimum_damage_bank_percent_to_throw: f32,
}

impl WaveProfile {
    pub(super) fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, NativePowerError> {
        validate_level(attributes, level)?;
        let tick_length = positive(attributes, level, "TickLength")?;
        let command_interval_ms = resolve_command_interval(attributes, level)?;
        let (minimum_ball_distance, maximum_ball_distance) =
            resolve_ball_distance_range(attributes, level)?;
        let ball_prototype = resolve_ball_prototype(database, attributes, level)?;
        Ok(Self {
            tick_length,
            supplies_per_tick: nonnegative(attributes, level, "SuppliesPerTick")?,
            supplies_resource_id: resource_id(database, "Supplies")
                .ok_or(NativePowerError::InvalidData("Supplies"))?,
            maximum_ball_speed_stagnant: nonnegative(attributes, level, "MaxBallSpeedStagnant")?,
            maximum_ball_speed_pulling: nonnegative(attributes, level, "MaxBallSpeedPulling")?,
            explode_time: nonnegative(attributes, level, "ExplodeTime")?,
            pulling_range: nonnegative(attributes, level, "PullingRange")?,
            explosion_force_on_debris: nonnegative(attributes, level, "ExplosionForceOnDebris")?,
            health_to_capture: nonnegative(attributes, level, "HealthToCapture")?,
            nudge_strength: nonnegative(attributes, level, "NudgeStrength")?,
            initial_lateral_pull_strength: nonnegative(
                attributes,
                level,
                "InitialLateralPullStrength",
            )?,
            captured_radial_spacing: nonnegative(attributes, level, "CapturedRadialSpacing")?,
            captured_spring_strength: nonnegative(attributes, level, "CapturedSpringStrength")?,
            captured_spring_dampening: nonnegative(attributes, level, "CapturedSpringDampening")?,
            captured_spring_rest_length: nonnegative(
                attributes,
                level,
                "CapturedSpringRestLength",
            )?,
            captured_minimum_lateral_speed: nonnegative(
                attributes,
                level,
                "CapturedMinLateralSpeed",
            )?,
            rip_attachment_chance_pulling: percentage_float(
                attributes,
                level,
                "RipAttachmentChancePulling",
            )?,
            pickup_object_rate: nonnegative(attributes, level, "PickupObjectRate")?,
            debris_angular_damping: nonnegative(attributes, level, "DebrisAngularDamping")?,
            maximum_explosion_damage_bank_per_captured: nonnegative(
                attributes,
                level,
                "MaxExplosionDamageBankPerCaptured",
            )?,
            explosion_damage_bank_per_tick: nonnegative(
                attributes,
                level,
                "ExplosionDamageBankPerTick",
            )?,
            command_interval_ms,
            minimum_ball_distance,
            maximum_ball_distance,
            maximum_captured_objects: nonnegative_usize(attributes, level, "MaxCapturedObjects")?,
            lightning_per_tick: nonnegative_usize(attributes, level, "LightningPerTick")?,
            nudge_chance_pulling: percentage_byte(attributes, level, "NudgeChancePulling")?,
            throw_part_chance_pulling: percentage_byte(
                attributes,
                level,
                "ThrowPartChancePulling",
            )?,
            lightning_chance_pulling: percentage_byte(attributes, level, "LightningChancePulling")?,
            ball_prototype,
            lightning_projectile: required_prototype(
                database,
                attributes,
                level,
                "LightningProjectile",
            )?,
            lightning_beam_visual: optional_prototype(
                database,
                attributes,
                level,
                "LightningBeamVisual",
            ),
            debris_projectile: required_prototype(database, attributes, level, "DebrisProjectile")?,
            explode_projectile: required_prototype(
                database,
                attributes,
                level,
                "ExplodeProjectile",
            )?,
            pickup_attachment: required_prototype(database, attributes, level, "PickupAttachment")?,
            explode_sound: required_string(attributes, level, "ExplodeSound", "sound")?,
            throw_units_on_explosion: required_bool(attributes, level, "ThrowUnitsOnExplosion")?,
            minimum_damage_bank_percent_to_throw: fraction(
                attributes,
                level,
                "MinDamageBankPercentToThrow",
            )?,
        })
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        for value in self.float_values() {
            checksum.hash_f32(value);
        }
        checksum.hash_u32(u32::try_from(self.supplies_resource_id).unwrap_or(u32::MAX));
        checksum.hash_u32(self.command_interval_ms);
        checksum.hash_u32(u32::try_from(self.maximum_captured_objects).unwrap_or(u32::MAX));
        checksum.hash_u32(u32::try_from(self.lightning_per_tick).unwrap_or(u32::MAX));
        checksum.hash_u32(u32::from(self.nudge_chance_pulling));
        checksum.hash_u32(u32::from(self.throw_part_chance_pulling));
        checksum.hash_u32(u32::from(self.lightning_chance_pulling));
        hash_string(checksum, &self.ball_prototype);
        hash_string(checksum, &self.lightning_projectile);
        hash_optional_string(checksum, self.lightning_beam_visual.as_deref());
        hash_string(checksum, &self.debris_projectile);
        hash_string(checksum, &self.explode_projectile);
        hash_string(checksum, &self.pickup_attachment);
        hash_string(checksum, &self.explode_sound);
        checksum.hash_u32(u32::from(self.throw_units_on_explosion));
    }

    fn float_values(&self) -> [f32; 23] {
        [
            self.tick_length,
            self.supplies_per_tick,
            self.maximum_ball_speed_stagnant,
            self.maximum_ball_speed_pulling,
            self.explode_time,
            self.pulling_range,
            self.explosion_force_on_debris,
            self.health_to_capture,
            self.nudge_strength,
            self.initial_lateral_pull_strength,
            self.captured_radial_spacing,
            self.captured_spring_strength,
            self.captured_spring_dampening,
            self.captured_spring_rest_length,
            self.captured_minimum_lateral_speed,
            self.rip_attachment_chance_pulling,
            self.pickup_object_rate,
            self.debris_angular_damping,
            self.maximum_explosion_damage_bank_per_captured,
            self.explosion_damage_bank_per_tick,
            self.minimum_ball_distance,
            self.maximum_ball_distance,
            self.minimum_damage_bank_percent_to_throw,
        ]
    }
}

fn resolve_command_interval(
    attributes: &PowerAttributes,
    level: u32,
) -> Result<u32, NativePowerError> {
    let seconds = positive(attributes, level, "CommandInterval")?;
    let milliseconds = seconds_to_milliseconds(seconds);
    (milliseconds > 0)
        .then_some(milliseconds)
        .ok_or(NativePowerError::InvalidData("CommandInterval"))
}

fn resolve_ball_distance_range(
    attributes: &PowerAttributes,
    level: u32,
) -> Result<(f32, f32), NativePowerError> {
    let minimum = nonnegative(attributes, level, "MinBallDistance")?;
    let maximum = nonnegative(attributes, level, "MaxBallDistance")?;
    (maximum >= minimum)
        .then_some((minimum, maximum))
        .ok_or(NativePowerError::InvalidData("MaxBallDistance"))
}

fn resolve_ball_prototype(
    database: &Database,
    attributes: &PowerAttributes,
    level: u32,
) -> Result<String, NativePowerError> {
    let prototype = required_prototype(database, attributes, level, "BallObject")?;
    is_unit_prototype(database, &prototype)
        .then_some(prototype)
        .ok_or(NativePowerError::InvalidData("BallObject"))
}

fn positive(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<f32, NativePowerError> {
    let value = required_float(attributes, level, name)?;
    (value > 0.0)
        .then_some(value)
        .ok_or(NativePowerError::InvalidData(name))
}

fn nonnegative(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<f32, NativePowerError> {
    let value = required_float(attributes, level, name)?;
    (value >= 0.0)
        .then_some(value)
        .ok_or(NativePowerError::InvalidData(name))
}

fn percentage_float(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<f32, NativePowerError> {
    let value = required_float(attributes, level, name)?;
    (0.0..=100.0)
        .contains(&value)
        .then_some(value / 100.0)
        .ok_or(NativePowerError::InvalidData(name))
}

fn fraction(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<f32, NativePowerError> {
    let value = required_float(attributes, level, name)?;
    (0.0..=1.0)
        .contains(&value)
        .then_some(value)
        .ok_or(NativePowerError::InvalidData(name))
}

fn nonnegative_usize(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<usize, NativePowerError> {
    usize::try_from(required_int(attributes, level, name)?)
        .map_err(|_| NativePowerError::InvalidData(name))
}

fn percentage_byte(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<u8, NativePowerError> {
    let value = required_int(attributes, level, name)?;
    if !(0..=100).contains(&value) {
        return Err(NativePowerError::InvalidData(name));
    }
    u8::try_from(value * 255 / 100).map_err(|_| NativePowerError::InvalidData(name))
}

fn hash_optional_string(checksum: &mut SyncChecksum, value: Option<&str>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        hash_string(checksum, value);
    } else {
        checksum.hash_u32(0);
    }
}

fn is_unit_prototype(database: &Database, name: &str) -> bool {
    database.objects.iter().any(|prototype| {
        prototype.name.eq_ignore_ascii_case(name)
            && prototype
                .object_class
                .as_deref()
                .is_some_and(|class| class.eq_ignore_ascii_case("Unit"))
    })
}
