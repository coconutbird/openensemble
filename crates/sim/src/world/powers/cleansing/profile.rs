//! Database resolution for retail's `BPowerCleansing` profile.

use super::super::NativePowerError;
use super::super::common::{
    optional_bool, optional_prototype, required_float, required_prototype, resource_id,
    seconds_to_milliseconds, validate_level,
};
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

#[derive(Debug, Clone)]
pub(super) struct CleansingProfile {
    pub beam_prototype: String,
    pub projectile_prototype: String,
    pub air_impact_prototype: Option<String>,
    pub tick_length: f32,
    pub supplies_per_tick: f32,
    pub supplies_resource_id: usize,
    pub minimum_beam_distance: f32,
    pub maximum_beam_distance: f32,
    pub command_interval_ms: u32,
    pub maximum_beam_speed: f32,
    pub requires_los: bool,
}

impl CleansingProfile {
    pub(super) fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, NativePowerError> {
        validate_level(attributes, level)?;
        let tick_length = required_float(attributes, level, "TickLength")?;
        if tick_length <= 0.0 {
            return Err(NativePowerError::InvalidData("TickLength"));
        }
        let supplies_per_tick = required_float(attributes, level, "SuppliesPerTick")?;
        if supplies_per_tick <= 0.0 {
            return Err(NativePowerError::InvalidData("SuppliesPerTick"));
        }
        let minimum_beam_distance = required_float(attributes, level, "MinBeamDistance")?.max(0.0);
        let maximum_beam_distance =
            required_float(attributes, level, "MaxBeamDistance")?.max(minimum_beam_distance);
        let maximum_beam_speed = required_float(attributes, level, "MaxBeamSpeed")?.max(0.0);
        let command_interval = required_float(attributes, level, "CommandInterval")?.max(0.1);
        let supplies_resource_id =
            resource_id(database, "Supplies").ok_or(NativePowerError::InvalidData("Supplies"))?;
        Ok(Self {
            beam_prototype: required_prototype(database, attributes, level, "Beam")?,
            projectile_prototype: required_prototype(database, attributes, level, "Projectile")?,
            air_impact_prototype: optional_prototype(
                database,
                attributes,
                level,
                "AirImpactObject",
            ),
            tick_length,
            supplies_per_tick,
            supplies_resource_id,
            minimum_beam_distance,
            maximum_beam_distance,
            command_interval_ms: seconds_to_milliseconds(command_interval),
            maximum_beam_speed,
            requires_los: optional_bool(attributes, level, "RequiresLOS").unwrap_or(true),
        })
    }
}
