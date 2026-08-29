//! Database resolution for retail's required Rage profile contract.

use super::super::NativePowerError;
use super::super::common::{
    optional_float, required_float, required_object_type, required_prototype, resource_id,
    validate_level,
};
use crate::spawn::object_prototype_id;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::powers::PowerAttributes;

#[derive(Debug)]
pub(super) struct RageProfile {
    pub tick_length: f32,
    pub supplies_per_tick: f32,
    pub supplies_per_tick_attacking: f32,
    pub supplies_per_jump: f32,
    pub supplies_resource_id: usize,
    pub damage_multiplier: f32,
    pub damage_taken_multiplier: f32,
    pub speed_multiplier: f32,
    pub nudge_multiplier: f32,
    pub scan_radius: f32,
    pub teleport_time: f32,
    pub teleport_lateral_distance: f32,
    pub teleport_jump_distance: f32,
    pub time_between_retarget: f32,
    pub distance_vs_angle_weight: f32,
    pub projectile_prototype: String,
    pub hand_attachment_prototype: String,
    pub hand_attachment_prototype_id: i32,
    pub teleport_attachment_prototype: String,
    pub teleport_attachment_prototype_id: i32,
    pub aura_attachment_prototypes: [String; 3],
    pub aura_attachment_prototype_ids: [i32; 3],
    pub heal_attachment_prototype: String,
    pub heal_attachment_prototype_id: i32,
    pub aura_filter_type: String,
    pub heal_per_kill_combat_value: f32,
    pub aura_radius: f32,
    pub aura_damage_bonus: f32,
}

impl RageProfile {
    pub(super) fn resolve(
        database: &Database,
        attributes: &PowerAttributes,
        level: u32,
    ) -> Result<Self, NativePowerError> {
        validate_level(attributes, level)?;
        let costs = resolve_costs(attributes, level)?;
        let movement = resolve_movement(attributes, level)?;
        validate_presentation_fields(attributes, level)?;
        let prototypes = resolve_prototypes(database, attributes, level)?;
        let aura = resolve_aura(database, attributes, level)?;
        let supplies_resource_id = if costs.iter().any(|cost| *cost > 0.0) {
            resource_id(database, "Supplies").ok_or(NativePowerError::InvalidData("Supplies"))?
        } else {
            0
        };
        Ok(Self {
            tick_length: positive(attributes, level, "TickLength")?,
            supplies_per_tick: costs[0],
            supplies_per_tick_attacking: costs[1],
            supplies_per_jump: costs[2],
            supplies_resource_id,
            damage_multiplier: positive(attributes, level, "DamageMultiplier")?,
            damage_taken_multiplier: positive(attributes, level, "DamageTakenMultiplier")?,
            speed_multiplier: positive(attributes, level, "SpeedMultiplier")?,
            nudge_multiplier: nonnegative(attributes, level, "NudgeMultiplier")?,
            scan_radius: positive(attributes, level, "ScanRadius")?,
            teleport_time: movement[0],
            teleport_lateral_distance: movement[1],
            teleport_jump_distance: movement[2],
            time_between_retarget: movement[3],
            distance_vs_angle_weight: movement[4],
            projectile_prototype: prototypes.0,
            hand_attachment_prototype: prototypes.1.0,
            hand_attachment_prototype_id: prototypes.1.1,
            teleport_attachment_prototype: prototypes.2.0,
            teleport_attachment_prototype_id: prototypes.2.1,
            aura_attachment_prototypes: aura.0,
            aura_attachment_prototype_ids: aura.1,
            heal_attachment_prototype: aura.2.0,
            heal_attachment_prototype_id: aura.2.1,
            aura_filter_type: aura.3,
            heal_per_kill_combat_value: optional_nonnegative(
                attributes,
                level,
                "HealPerKillCombatValue",
            )?,
            aura_radius: optional_nonnegative(attributes, level, "AuraRadius")?,
            aura_damage_bonus: optional_nonnegative(attributes, level, "AuraDamageBonus")?,
        })
    }
}

fn resolve_costs(attributes: &PowerAttributes, level: u32) -> Result<[f32; 3], NativePowerError> {
    Ok([
        nonnegative(attributes, level, "SuppliesPerTick")?,
        nonnegative(attributes, level, "SuppliesPerTickAttacking")?,
        nonnegative(attributes, level, "SuppliesPerJump")?,
    ])
}

fn resolve_movement(
    attributes: &PowerAttributes,
    level: u32,
) -> Result<[f32; 5], NativePowerError> {
    let weight = required_float(attributes, level, "DistanceVsAngleWeight")?;
    if !(0.0..1.0).contains(&weight) || weight == 0.0 {
        return Err(NativePowerError::InvalidData("DistanceVsAngleWeight"));
    }
    Ok([
        positive(attributes, level, "TeleportTime")?,
        nonnegative(attributes, level, "TeleportLateralDistance")?,
        nonnegative(attributes, level, "TeleportJumpDistance")?,
        nonnegative(attributes, level, "TimeBetweenRetarget")?,
        weight,
    ])
}

fn validate_presentation_fields(
    attributes: &PowerAttributes,
    level: u32,
) -> Result<(), NativePowerError> {
    for name in ["MotionBlurAmount", "MotionBlurDistance", "MotionBlurTime"] {
        let _value = nonnegative(attributes, level, name)?;
    }
    Ok(())
}

type NamedPrototype = (String, i32);
type CorePrototypes = (String, NamedPrototype, NamedPrototype);

fn resolve_prototypes(
    database: &Database,
    attributes: &PowerAttributes,
    level: u32,
) -> Result<CorePrototypes, NativePowerError> {
    let projectile = required_prototype(database, attributes, level, "Projectile")?;
    let hand = named_prototype(database, attributes, level, "HandAttachObject")?;
    let teleport = named_prototype(database, attributes, level, "TeleportAttachObject")?;
    Ok((projectile, hand, teleport))
}

type AuraProfile = ([String; 3], [i32; 3], NamedPrototype, String);

fn resolve_aura(
    database: &Database,
    attributes: &PowerAttributes,
    level: u32,
) -> Result<AuraProfile, NativePowerError> {
    let small = named_prototype(database, attributes, level, "AuraAttachFxSmall")?;
    let medium = named_prototype(database, attributes, level, "AuraAttachFxMedium")?;
    let large = named_prototype(database, attributes, level, "AuraAttachFxLarge")?;
    let heal = named_prototype(database, attributes, level, "HealAttachFx")?;
    Ok((
        [small.0, medium.0, large.0],
        [small.1, medium.1, large.1],
        heal,
        required_object_type(database, attributes, level, "AuraFilterType")?,
    ))
}

fn named_prototype(
    database: &Database,
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<NamedPrototype, NativePowerError> {
    let prototype = required_prototype(database, attributes, level, name)?;
    let id = object_prototype_id(database, &prototype)
        .ok_or_else(|| NativePowerError::UnknownPrototype(prototype.clone()))?;
    Ok((prototype, id))
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

fn optional_nonnegative(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<f32, NativePowerError> {
    let value = optional_float(attributes, level, name)?.unwrap_or_default();
    (value >= 0.0)
        .then_some(value)
        .ok_or(NativePowerError::InvalidData(name))
}
