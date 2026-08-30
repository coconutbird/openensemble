use super::MAX_AUTHORED_SHOTS;
use crate::world::powers::NativePowerError;
use crate::world::powers::common::{
    optional_bool, optional_float, required_int, required_prototype, seconds_to_milliseconds,
    validate_level,
};
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

#[derive(Debug, Clone)]
pub(super) struct OrbitalProfile {
    pub target_beam_prototype: String,
    pub target_beam_speed: f32,
    pub projectile_prototype: String,
    pub effect_prototype: String,
    pub rock_small_prototype: String,
    pub rock_medium_prototype: String,
    pub rock_large_prototype: String,
    pub shots: u32,
    pub targeting_delay_ms: u32,
    pub auto_shot_delay_ms: u32,
    pub auto_shot_inner_radius: f32,
    pub auto_shot_outer_radius: f32,
    pub launch_offset: Vec3,
    pub requires_los: bool,
}

impl OrbitalProfile {
    pub(super) fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, NativePowerError> {
        validate_level(attributes, level)?;
        let target_beam_prototype = required_prototype(database, attributes, level, "TargetBeam")?;
        let target_beam_speed = prototype_speed(database, &target_beam_prototype);
        let shots = required_int(attributes, level, "NumShots")?;
        let shots = u32::try_from(shots)
            .ok()
            .filter(|shots| (1..=MAX_AUTHORED_SHOTS).contains(shots))
            .ok_or(NativePowerError::InvalidData("NumShots"))?;
        let targeting_delay_ms =
            optional_float(attributes, level, "TargetingDelay")?.map_or(0, seconds_to_milliseconds);
        let auto_shot_delay_ms =
            optional_float(attributes, level, "AutoShotDelay")?.map_or(0, seconds_to_milliseconds);
        Ok(Self {
            target_beam_prototype,
            target_beam_speed,
            projectile_prototype: required_prototype(database, attributes, level, "Projectile")?,
            effect_prototype: required_prototype(database, attributes, level, "Effect")?,
            rock_small_prototype: required_prototype(database, attributes, level, "RockSmall")?,
            rock_medium_prototype: required_prototype(database, attributes, level, "RockMedium")?,
            rock_large_prototype: required_prototype(database, attributes, level, "RockLarge")?,
            shots,
            targeting_delay_ms,
            auto_shot_delay_ms,
            auto_shot_inner_radius: optional_float(attributes, level, "AutoShotInnerRadius")?
                .unwrap_or_default(),
            auto_shot_outer_radius: optional_float(attributes, level, "AutoShotOuterRadius")?
                .unwrap_or_default(),
            launch_offset: Vec3::new(
                optional_float(attributes, level, "XOffset")?.unwrap_or_default(),
                optional_float(attributes, level, "YOffset")?.unwrap_or_default(),
                optional_float(attributes, level, "ZOffset")?.unwrap_or_default(),
            ),
            requires_los: optional_bool(attributes, level, "RequiresLOS").unwrap_or(true),
        })
    }
}

fn prototype_speed(database: &Database, prototype_name: &str) -> f32 {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(prototype_name))
        .and_then(|prototype| prototype.max_velocity.or(prototype.velocity))
        .filter(|speed| speed.is_finite() && *speed >= 0.0)
        .unwrap_or_default()
}
