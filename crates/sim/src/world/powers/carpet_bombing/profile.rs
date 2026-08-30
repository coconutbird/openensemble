//! Scenario-layered Carpet Bombing profile resolution.

use super::{BOMB_TICK_SECONDS, MAX_AUTHORED_BOMB_CLUSTERS};
use crate::world::powers::NativePowerError;
use crate::world::powers::common::{
    BomberProfile, optional_bool, required_float, required_int, required_prototype, validate_level,
};
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

#[derive(Debug)]
pub(super) struct CarpetBombingProfile {
    pub projectile_prototype: String,
    pub impact_prototype: String,
    pub explosion_prototype: String,
    pub bomber: BomberProfile,
    pub requires_los: bool,
    pub initial_delay: f32,
    pub fuse_time: f32,
    pub maximum_bomb_clusters: u32,
    pub maximum_bomb_offset: f32,
    pub bomb_spacing: f32,
    pub length_multiplier: f32,
    pub wedge_length_multiplier: f32,
    pub wedge_minimum_offset: f32,
    pub nudge_multiplier: f32,
}

impl CarpetBombingProfile {
    pub(super) fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, NativePowerError> {
        validate_level(attributes, level)?;
        let initial_delay = nonnegative(attributes, level, "InitialDelay")?;
        let fuse_time = nonnegative(attributes, level, "FuseTime")?;
        let maximum_bomb_clusters = required_int(attributes, level, "MaxBombs")?;
        let maximum_bomb_clusters = u32::try_from(maximum_bomb_clusters.max(0))
            .map_err(|_| NativePowerError::InvalidData("MaxBombs"))?;
        if maximum_bomb_clusters > MAX_AUTHORED_BOMB_CLUSTERS {
            return Err(NativePowerError::InvalidData("MaxBombs"));
        }
        let maximum_bomb_offset = positive(attributes, level, "MaxBombOffset")?;
        let bomb_spacing = nonnegative(attributes, level, "BombSpacing")?;
        if maximum_bomb_offset <= bomb_spacing {
            return Err(NativePowerError::InvalidData("BombSpacing"));
        }
        let length_multiplier = positive(attributes, level, "LengthMultiplier")?;
        let wedge_length_multiplier = positive(attributes, level, "WedgeLengthMultiplier")?;
        let wedge_minimum_offset = nonnegative(attributes, level, "WedgeMinOffset")?;
        let nudge_multiplier = nonnegative(attributes, level, "NudgeMultiplier")?;
        let bomber_prototype = required_prototype(database, attributes, level, "Bomber")?;
        let flyin_distance = nonnegative(attributes, level, "BomberFlyinDistance")?;
        let flyin_height = nonnegative(attributes, level, "BomberFlyinHeight")?;
        let bomb_height = nonnegative(attributes, level, "BomberBombHeight")?;
        let speed = positive(attributes, level, "BomberSpeed")?;
        let flyout_time =
            initial_delay + maximum_bomb_clusters.to_f32().unwrap_or(f32::MAX) * BOMB_TICK_SECONDS;
        Ok(Self {
            projectile_prototype: required_prototype(database, attributes, level, "Projectile")?,
            impact_prototype: required_prototype(database, attributes, level, "Impact")?,
            explosion_prototype: required_prototype(database, attributes, level, "Explosion")?,
            bomber: BomberProfile {
                prototype: bomber_prototype,
                bomb_time: initial_delay,
                flyin_distance,
                flyin_height,
                bomb_height,
                speed,
                flyout_time,
            },
            requires_los: optional_bool(attributes, level, "RequiresLOS").unwrap_or(true),
            initial_delay,
            fuse_time,
            maximum_bomb_clusters,
            maximum_bomb_offset,
            bomb_spacing,
            length_multiplier,
            wedge_length_multiplier,
            wedge_minimum_offset,
            nudge_multiplier,
        })
    }
}

fn positive(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<f32, NativePowerError> {
    let value = required_float(attributes, level, name)?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err(NativePowerError::InvalidData(name))
    }
}

fn nonnegative(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<f32, NativePowerError> {
    let value = required_float(attributes, level, name)?;
    if value >= 0.0 {
        Ok(value)
    } else {
        Err(NativePowerError::InvalidData(name))
    }
}
