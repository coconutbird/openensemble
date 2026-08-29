//! Deterministic player-specific technology effects.
//!
//! Retail keeps mutable proto objects, tactics, and ability recovery times per
//! player. This compact representation preserves that ownership without
//! cloning the complete database into every [`Player`](super::Player).

mod proto_data;

pub(crate) use proto_data::{ProtoDataModification, ProtoDataRelativity, ProtoDataType};

use crate::sync::SyncChecksum;
use pipeline::database::hw1::techs::TechEffect;
use pipeline::database::hw1::{Database, Tech};
use proto_data::RuntimeProtoData;
use std::collections::BTreeMap;

/// The technology state owned by one player.
#[derive(Debug, Clone, Default)]
pub struct PlayerTechState {
    active_technologies: Vec<String>,
    action_effects: Vec<ActionEffect>,
    command_effects: Vec<CommandEffect>,
    weapon_effects: Vec<WeaponScalarEffect>,
    hitpoint_effects: Vec<ProtoScalarEffect>,
    shieldpoint_effects: Vec<ProtoScalarEffect>,
    player_shield_regen_rate_effects: Vec<ScalarOperation>,
    player_shield_regen_delay_effects: Vec<ScalarOperation>,
    unit_shield_regen_rate_effects: Vec<ProtoScalarEffect>,
    unit_shield_regen_delay_effects: Vec<ProtoScalarEffect>,
    ability_recovery_effects: Vec<AbilityScalarEffect>,
    squad_transforms: BTreeMap<String, String>,
    runtime_proto_data: RuntimeProtoData,
}

#[derive(Debug, Clone)]
struct ActionEffect {
    proto_object: String,
    action: Option<String>,
    enabled: bool,
}

#[derive(Debug, Clone)]
struct CommandEffect {
    proto_object: String,
    command_type: String,
    command_data: String,
    enabled: bool,
}

#[derive(Debug, Clone)]
struct ProtoScalarEffect {
    proto_object: String,
    action: Option<String>,
    operation: ScalarOperation,
}

#[derive(Debug, Clone)]
struct WeaponScalarEffect {
    data_type: ProtoDataType,
    scalar: ProtoScalarEffect,
}

#[derive(Debug, Clone)]
struct AbilityScalarEffect {
    ability: String,
    operation: ScalarOperation,
}

#[derive(Debug, Clone, Copy)]
struct ScalarOperation {
    amount: f32,
    relativity: ScalarRelativity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum ScalarRelativity {
    Absolute,
    BasePercent,
    Percent,
    Assign,
    BasePercentAssign,
}

/// One persistent proto-squad transformation applied by a newly active tech.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedSquadTransform {
    /// Logical player proto that remains the squad's type.
    pub from: String,
    /// Definition installed in that logical proto before this effect.
    pub previous_definition: String,
    /// Definition installed by this effect.
    pub new_definition: String,
}

impl PlayerTechState {
    /// Return whether a named technology is active for this player.
    #[must_use]
    pub fn is_active(&self, technology: &str) -> bool {
        self.active_technologies
            .iter()
            .any(|active| active.eq_ignore_ascii_case(technology.trim()))
    }

    /// Iterate active technology names in activation order.
    pub fn active_technologies(&self) -> impl Iterator<Item = &str> {
        self.active_technologies.iter().map(String::as_str)
    }

    /// Resolve a player-specific tactic action against its authored state.
    #[must_use]
    pub fn action_enabled(&self, proto_object: &str, action: &str, authored_enabled: bool) -> bool {
        self.action_effects
            .iter()
            .filter(|effect| {
                effect.proto_object.eq_ignore_ascii_case(proto_object)
                    && effect
                        .action
                        .as_deref()
                        .is_none_or(|name| name.eq_ignore_ascii_case(action))
            })
            .fold(authored_enabled, |_, effect| effect.enabled)
    }

    /// Resolve one building command against authored state and active techs.
    #[must_use]
    pub fn command_enabled(
        &self,
        proto_object: &str,
        command_type: &str,
        command_data: &str,
        authored_enabled: bool,
    ) -> bool {
        let enabled = self
            .command_effects
            .iter()
            .filter(|effect| {
                effect.proto_object.eq_ignore_ascii_case(proto_object)
                    && effect.command_type.eq_ignore_ascii_case(command_type)
                    && effect.command_data.eq_ignore_ascii_case(command_data)
            })
            .fold(authored_enabled, |_, effect| effect.enabled);
        self.runtime_proto_data.command_state(
            ProtoDataType::CommandEnable,
            proto_object,
            command_type,
            command_data,
            enabled,
        )
    }

    /// Resolve whether a player-specific command remains selectable in the UI.
    #[must_use]
    pub fn command_selectable(
        &self,
        proto_object: &str,
        command_type: &str,
        command_data: &str,
        authored_selectable: bool,
    ) -> bool {
        self.runtime_proto_data.command_state(
            ProtoDataType::CommandSelectable,
            proto_object,
            command_type,
            command_data,
            authored_selectable,
        )
    }

    /// Apply player technology effects to one authored weapon damage value.
    #[must_use]
    pub fn weapon_damage(&self, proto_object: &str, weapon: &str, base: f32) -> f32 {
        self.weapon_scalar(ProtoDataType::Damage, proto_object, weapon, base)
    }

    /// Apply player-owned prototype changes to an authored weapon range.
    #[must_use]
    pub fn weapon_range(&self, proto_object: &str, weapon: &str, base: f32) -> f32 {
        self.weapon_scalar(ProtoDataType::MaximumRange, proto_object, weapon, base)
    }

    /// Apply player-owned prototype changes to authored weapon accuracy.
    #[must_use]
    pub fn weapon_accuracy(&self, proto_object: &str, weapon: &str, base: f32) -> f32 {
        self.weapon_scalar(ProtoDataType::Accuracy, proto_object, weapon, base)
    }

    /// Apply player-owned prototype changes to authored moving accuracy.
    #[must_use]
    pub fn weapon_moving_accuracy(&self, proto_object: &str, weapon: &str, base: f32) -> f32 {
        self.weapon_scalar(ProtoDataType::MovingAccuracy, proto_object, weapon, base)
    }

    /// Apply player-owned prototype changes to authored weapon deviation.
    #[must_use]
    pub fn weapon_max_deviation(&self, proto_object: &str, weapon: &str, base: f32) -> f32 {
        self.weapon_scalar(ProtoDataType::MaxDeviation, proto_object, weapon, base)
    }

    /// Apply player-owned prototype changes to authored moving deviation.
    #[must_use]
    pub fn weapon_moving_max_deviation(&self, proto_object: &str, weapon: &str, base: f32) -> f32 {
        self.weapon_scalar(
            ProtoDataType::MovingMaxDeviation,
            proto_object,
            weapon,
            base,
        )
    }

    /// Apply player-owned changes to the accuracy distribution split point.
    #[must_use]
    pub fn weapon_accuracy_distance_factor(
        &self,
        proto_object: &str,
        weapon: &str,
        base: f32,
    ) -> f32 {
        self.weapon_scalar(
            ProtoDataType::AccuracyDistanceFactor,
            proto_object,
            weapon,
            base,
        )
    }

    /// Apply player-owned changes to deviation at the distribution split.
    #[must_use]
    pub fn weapon_accuracy_deviation_factor(
        &self,
        proto_object: &str,
        weapon: &str,
        base: f32,
    ) -> f32 {
        self.weapon_scalar(
            ProtoDataType::AccuracyDeviationFactor,
            proto_object,
            weapon,
            base,
        )
    }

    /// Apply player-owned prototype changes to launch-time velocity leading.
    #[must_use]
    pub fn weapon_max_velocity_lead(&self, proto_object: &str, weapon: &str, base: f32) -> f32 {
        self.weapon_scalar(ProtoDataType::MaxVelocityLead, proto_object, weapon, base)
    }

    /// Apply player-owned prototype changes to primary-target AOE damage.
    #[must_use]
    pub fn weapon_aoe_primary_target_factor(
        &self,
        proto_object: &str,
        weapon: &str,
        base: f32,
    ) -> f32 {
        self.weapon_scalar(
            ProtoDataType::AoePrimaryTargetFactor,
            proto_object,
            weapon,
            base,
        )
    }

    fn weapon_scalar(
        &self,
        data_type: ProtoDataType,
        proto_object: &str,
        weapon: &str,
        base: f32,
    ) -> f32 {
        let current = apply_weapon_scalar_effects(
            &self.weapon_effects,
            data_type,
            proto_object,
            weapon,
            base,
        );
        self.runtime_proto_data
            .scalar(data_type, proto_object, Some(weapon), base, current)
    }

    /// Apply player technology effects to one authored maximum hitpoint value.
    #[must_use]
    pub fn hitpoints(&self, proto_object: &str, base: f32) -> f32 {
        let current = apply_proto_scalar_effects(&self.hitpoint_effects, proto_object, None, base);
        self.runtime_proto_data
            .scalar(ProtoDataType::Hitpoints, proto_object, None, base, current)
    }

    /// Apply player technology effects to a proto object's shield maximum.
    #[must_use]
    pub fn shieldpoints(&self, proto_object: &str, base: f32) -> f32 {
        let current =
            apply_proto_scalar_effects(&self.shieldpoint_effects, proto_object, None, base);
        self.runtime_proto_data.scalar(
            ProtoDataType::Shieldpoints,
            proto_object,
            None,
            base,
            current,
        )
    }

    /// Apply player-owned prototype changes to authored line of sight.
    #[must_use]
    pub fn line_of_sight(&self, proto_object: &str, base: f32) -> f32 {
        self.runtime_proto_data
            .scalar(ProtoDataType::LineOfSight, proto_object, None, base, base)
    }

    /// Apply player-owned prototype changes to authored desired velocity.
    #[must_use]
    pub fn maximum_velocity(&self, proto_object: &str, base: f32) -> f32 {
        self.runtime_proto_data.scalar(
            ProtoDataType::MaximumVelocity,
            proto_object,
            None,
            base,
            base,
        )
    }

    /// Apply player-owned prototype changes to authored construction work.
    #[must_use]
    pub fn build_points(&self, proto_object: &str, base: f32) -> f32 {
        self.runtime_proto_data
            .scalar(ProtoDataType::BuildPoints, proto_object, None, base, base)
    }

    /// Apply player-level technology effects to the base shield recharge rate.
    #[must_use]
    pub fn shield_regen_rate(&self, base: f32) -> f32 {
        apply_scalar_operations(&self.player_shield_regen_rate_effects, base)
    }

    /// Apply player-level technology effects to the base post-damage delay.
    #[must_use]
    pub fn shield_regen_delay(&self, base: f32) -> f32 {
        apply_scalar_operations(&self.player_shield_regen_delay_effects, base)
    }

    /// Resolve the per-unit recharge-rate scalar assigned by player tech.
    #[must_use]
    pub fn unit_shield_regen_rate(&self, proto_object: &str) -> f32 {
        assigned_unit_shield_scalar(&self.unit_shield_regen_rate_effects, proto_object)
    }

    /// Resolve the per-unit recharge-delay scalar assigned by player tech.
    #[must_use]
    pub fn unit_shield_regen_delay(&self, proto_object: &str) -> f32 {
        assigned_unit_shield_scalar(&self.unit_shield_regen_delay_effects, proto_object)
    }

    /// Apply player technology effects to an authored ability recovery time.
    #[must_use]
    pub fn ability_recovery_time(&self, ability: &str, base: f32) -> f32 {
        let current = self
            .ability_recovery_effects
            .iter()
            .filter(|effect| effect.ability.eq_ignore_ascii_case(ability))
            .fold(base, |current, effect| {
                effect.operation.apply(base, current)
            });
        valid_scalar_result(current, base)
    }

    /// Resolve the effective definition of a logical player proto squad.
    #[must_use]
    pub fn resolved_squad_prototype<'name>(&'name self, logical_name: &'name str) -> &'name str {
        self.squad_transforms
            .get(&normalize(logical_name))
            .map_or(logical_name, String::as_str)
    }

    pub(crate) fn modify_proto_data(
        &mut self,
        proto_object: &str,
        modification: &ProtoDataModification,
    ) {
        self.runtime_proto_data.record(proto_object, modification);
    }

    /// Number of persistent trigger-authored prototype changes owned by this player.
    #[must_use]
    pub fn runtime_proto_modification_count(&self) -> usize {
        self.runtime_proto_data.modification_count()
    }

    pub(crate) fn activate(
        &mut self,
        database: &Database,
        technology: &Tech,
    ) -> Vec<AppliedSquadTransform> {
        if self.is_active(&technology.name) {
            return Vec::new();
        }

        let transforms = technology
            .effects
            .as_ref()
            .map_or(&[][..], |effects| effects.entries.as_slice())
            .iter()
            .filter_map(|effect| self.apply_squad_transform(effect))
            .collect();
        self.active_technologies.push(technology.name.clone());
        self.rebuild(database);
        transforms
    }

    pub(crate) fn deactivate(&mut self, database: &Database, technology: &str) -> bool {
        let Some(index) = self
            .active_technologies
            .iter()
            .position(|active| active.eq_ignore_ascii_case(technology.trim()))
        else {
            return false;
        };
        self.active_technologies.remove(index);
        self.rebuild(database);
        true
    }

    fn apply_squad_transform(&mut self, effect: &TechEffect) -> Option<AppliedSquadTransform> {
        if !effect
            .effect_type
            .eq_ignore_ascii_case("TransformProtoSquad")
        {
            return None;
        }
        let from = nonempty(effect.from_type.as_deref())?;
        let new_definition = nonempty(effect.to_type.as_deref())?;
        let previous_definition = self.resolved_squad_prototype(from).to_owned();
        self.squad_transforms
            .insert(normalize(from), new_definition.to_owned());
        Some(AppliedSquadTransform {
            from: from.to_owned(),
            previous_definition,
            new_definition: new_definition.to_owned(),
        })
    }

    fn rebuild(&mut self, database: &Database) {
        self.action_effects.clear();
        self.command_effects.clear();
        self.weapon_effects.clear();
        self.hitpoint_effects.clear();
        self.shieldpoint_effects.clear();
        self.player_shield_regen_rate_effects.clear();
        self.player_shield_regen_delay_effects.clear();
        self.unit_shield_regen_rate_effects.clear();
        self.unit_shield_regen_delay_effects.clear();
        self.ability_recovery_effects.clear();

        let active = self.active_technologies.clone();
        for active_name in active {
            let Some(technology) = database
                .techs
                .iter()
                .find(|technology| technology.name.eq_ignore_ascii_case(&active_name))
            else {
                continue;
            };
            for effect in technology
                .effects
                .as_ref()
                .map_or(&[][..], |effects| effects.entries.as_slice())
            {
                self.apply_data_effect(effect);
            }
        }
    }

    fn apply_data_effect(&mut self, effect: &TechEffect) {
        if !effect.effect_type.eq_ignore_ascii_case("Data") {
            return;
        }
        let Some(subtype) = nonempty(effect.subtype.as_deref()) else {
            return;
        };
        if subtype.eq_ignore_ascii_case("ActionEnable") {
            self.collect_action_effect(effect);
        } else if subtype.eq_ignore_ascii_case("CommandEnable") {
            self.collect_command_effect(effect);
        } else if let Some(data_type) = weapon_data_type(subtype) {
            if let Some(scalar) = proto_scalar_effect(effect, true) {
                self.weapon_effects
                    .push(WeaponScalarEffect { data_type, scalar });
            }
        } else if subtype.eq_ignore_ascii_case("Hitpoints") {
            if let Some(effect) = proto_scalar_effect(effect, false) {
                self.hitpoint_effects.push(effect);
            }
        } else if subtype.eq_ignore_ascii_case("Shieldpoints") {
            if let Some(effect) = proto_scalar_effect(effect, false) {
                self.shieldpoint_effects.push(effect);
            }
        } else if subtype.eq_ignore_ascii_case("ShieldRegenRate") {
            collect_shield_regen_effect(
                effect,
                &mut self.player_shield_regen_rate_effects,
                &mut self.unit_shield_regen_rate_effects,
            );
        } else if subtype.eq_ignore_ascii_case("ShieldRegenDelay") {
            collect_shield_regen_effect(
                effect,
                &mut self.player_shield_regen_delay_effects,
                &mut self.unit_shield_regen_delay_effects,
            );
        } else if subtype.eq_ignore_ascii_case("AbilityRecoverTime") {
            self.collect_ability_recovery_effect(effect);
        }
    }

    fn collect_action_effect(&mut self, effect: &TechEffect) {
        let Some(proto_object) = effect_target(effect, "ProtoUnit") else {
            return;
        };
        let action = if effect.allactions == Some(true) {
            None
        } else {
            nonempty(effect.action.as_deref()).map(normalize)
        };
        if action.is_none() && effect.allactions != Some(true) {
            return;
        }
        let Some(amount) = effect.amount.filter(|amount| amount.is_finite()) else {
            return;
        };
        self.action_effects.push(ActionEffect {
            proto_object: normalize(proto_object),
            action,
            enabled: amount != 0.0,
        });
    }

    fn collect_command_effect(&mut self, effect: &TechEffect) {
        let (Some(proto_object), Some(command_type), Some(command_data), Some(amount)) = (
            effect_target(effect, "ProtoUnit"),
            nonempty(effect.command_type.as_deref()),
            nonempty(effect.command_data.as_deref()),
            effect.amount.filter(|amount| amount.is_finite()),
        ) else {
            return;
        };
        self.command_effects.push(CommandEffect {
            proto_object: normalize(proto_object),
            command_type: normalize(command_type),
            command_data: normalize(command_data),
            enabled: amount != 0.0,
        });
    }

    fn collect_ability_recovery_effect(&mut self, effect: &TechEffect) {
        if !effect_target_is(effect, "Player") {
            return;
        }
        let (Some(ability), Some(operation)) = (
            nonempty(effect.ability.as_deref()),
            ScalarOperation::from_effect(effect),
        ) else {
            return;
        };
        self.ability_recovery_effects.push(AbilityScalarEffect {
            ability: normalize(ability),
            operation,
        });
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_strings(checksum, &self.active_technologies);
        checksum.hash_u32(u32::try_from(self.squad_transforms.len()).unwrap_or(u32::MAX));
        for (from, to) in &self.squad_transforms {
            hash_string(checksum, from);
            hash_string(checksum, to);
        }
        self.runtime_proto_data.hash_state(checksum);
    }
}

impl ScalarOperation {
    fn from_effect(effect: &TechEffect) -> Option<Self> {
        let amount = effect.amount.filter(|amount| amount.is_finite())?;
        let relativity = ScalarRelativity::from_authored(effect.relativity.as_deref()?)?;
        Some(Self { amount, relativity })
    }

    fn apply(self, base: f32, current: f32) -> f32 {
        match self.relativity {
            ScalarRelativity::Absolute => current + self.amount,
            ScalarRelativity::BasePercent => current + base.mul_add(self.amount, -base),
            ScalarRelativity::Percent => current * self.amount,
            ScalarRelativity::Assign => self.amount,
            ScalarRelativity::BasePercentAssign => base * self.amount,
        }
    }
}

impl ScalarRelativity {
    fn from_authored(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("Absolute") {
            Some(Self::Absolute)
        } else if value.eq_ignore_ascii_case("BasePercent") {
            Some(Self::BasePercent)
        } else if value.eq_ignore_ascii_case("Percent") {
            Some(Self::Percent)
        } else if value.eq_ignore_ascii_case("Assign") {
            Some(Self::Assign)
        } else if value.eq_ignore_ascii_case("BasePercentAssign") {
            Some(Self::BasePercentAssign)
        } else {
            None
        }
    }
}

fn proto_scalar_effect(effect: &TechEffect, action_sensitive: bool) -> Option<ProtoScalarEffect> {
    let proto_object = effect_target(effect, "ProtoUnit")?;
    let action = if action_sensitive && effect.allactions != Some(true) {
        Some(normalize(nonempty(effect.action.as_deref())?))
    } else {
        None
    };
    Some(ProtoScalarEffect {
        proto_object: normalize(proto_object),
        action,
        operation: ScalarOperation::from_effect(effect)?,
    })
}

fn apply_proto_scalar_effects(
    effects: &[ProtoScalarEffect],
    proto_object: &str,
    action: Option<&str>,
    base: f32,
) -> f32 {
    let current = effects
        .iter()
        .filter(|effect| {
            effect.proto_object.eq_ignore_ascii_case(proto_object)
                && effect.action.as_deref().is_none_or(|required| {
                    action.is_some_and(|actual| required.eq_ignore_ascii_case(actual))
                })
        })
        .fold(base, |current, effect| {
            effect.operation.apply(base, current)
        });
    valid_scalar_result(current, base)
}

fn apply_weapon_scalar_effects(
    effects: &[WeaponScalarEffect],
    data_type: ProtoDataType,
    proto_object: &str,
    weapon: &str,
    base: f32,
) -> f32 {
    let current = effects
        .iter()
        .filter(|effect| {
            effect.data_type == data_type
                && effect
                    .scalar
                    .proto_object
                    .eq_ignore_ascii_case(proto_object)
                && effect
                    .scalar
                    .action
                    .as_deref()
                    .is_none_or(|required| required.eq_ignore_ascii_case(weapon))
        })
        .fold(base, |current, effect| {
            effect.scalar.operation.apply(base, current)
        });
    valid_scalar_result(current, base)
}

fn weapon_data_type(subtype: &str) -> Option<ProtoDataType> {
    [
        ("MaximumRange", ProtoDataType::MaximumRange),
        ("Damage", ProtoDataType::Damage),
        (
            "AOEPrimaryTargetFactor",
            ProtoDataType::AoePrimaryTargetFactor,
        ),
        ("Accuracy", ProtoDataType::Accuracy),
        ("MovingAccuracy", ProtoDataType::MovingAccuracy),
        ("MaxDeviation", ProtoDataType::MaxDeviation),
        ("MovingMaxDeviation", ProtoDataType::MovingMaxDeviation),
        (
            "AccuracyDistanceFactor",
            ProtoDataType::AccuracyDistanceFactor,
        ),
        (
            "AccuracyDeviationFactor",
            ProtoDataType::AccuracyDeviationFactor,
        ),
        ("MaxVelocityLead", ProtoDataType::MaxVelocityLead),
    ]
    .into_iter()
    .find_map(|(name, data_type)| subtype.eq_ignore_ascii_case(name).then_some(data_type))
}

fn apply_scalar_operations(effects: &[ScalarOperation], base: f32) -> f32 {
    let current = effects
        .iter()
        .fold(base, |current, effect| effect.apply(base, current));
    valid_scalar_result(current, base)
}

fn assigned_unit_shield_scalar(effects: &[ProtoScalarEffect], proto_object: &str) -> f32 {
    let value = effects
        .iter()
        .filter(|effect| effect.proto_object.eq_ignore_ascii_case(proto_object))
        .fold(1.0, |_, effect| effect.operation.amount);
    valid_scalar_result(value, 1.0)
}

fn collect_shield_regen_effect(
    effect: &TechEffect,
    player_effects: &mut Vec<ScalarOperation>,
    unit_effects: &mut Vec<ProtoScalarEffect>,
) {
    if effect_target_is(effect, "Player") {
        if let Some(operation) = ScalarOperation::from_effect(effect) {
            player_effects.push(operation);
        }
    } else if let Some(effect) = proto_scalar_effect(effect, false) {
        unit_effects.push(effect);
    }
}

fn valid_scalar_result(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        fallback
    }
}

fn effect_target<'effect>(
    effect: &'effect TechEffect,
    expected_type: &str,
) -> Option<&'effect str> {
    let target = effect.target.as_ref()?;
    target
        .target_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case(expected_type))
        .then_some(target.value.as_deref())
        .flatten()
        .and_then(|value| nonempty(Some(value)))
}

fn effect_target_is(effect: &TechEffect, expected_type: &str) -> bool {
    effect
        .target
        .as_ref()
        .and_then(|target| target.target_type.as_deref())
        .is_some_and(|kind| kind.eq_ignore_ascii_case(expected_type))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn hash_strings(checksum: &mut SyncChecksum, values: &[String]) {
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        hash_string(checksum, value);
    }
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper};

    #[test]
    fn static_and_runtime_weapon_accuracy_effects_layer_in_retail_order() {
        let technology = Tech {
            name: "Sharpshooter".to_owned(),
            effects: Some(EffectsWrapper {
                entries: vec![
                    weapon_effect("Accuracy", 0.5, "Percent", Some("Rifle"), false),
                    weapon_effect("MovingMaxDeviation", 2.0, "Percent", None, true),
                    weapon_effect("MaxVelocityLead", 5.0, "Assign", None, true),
                ],
            }),
            ..Tech::default()
        };
        let mut database = Database::new();
        database.techs.push(technology.clone());
        let mut state = PlayerTechState::default();
        let _transforms = state.activate(&database, &technology);

        assert!((state.weapon_accuracy("Marine", "Rifle", 0.8) - 0.4).abs() < f32::EPSILON);
        assert!((state.weapon_accuracy("Marine", "Pistol", 0.8) - 0.8).abs() < f32::EPSILON);
        assert!(
            (state.weapon_moving_max_deviation("Marine", "Rifle", 3.0) - 6.0).abs() < f32::EPSILON
        );
        assert!(
            (state.weapon_max_velocity_lead("Marine", "Rifle", 0.0) - 5.0).abs() < f32::EPSILON
        );

        state.modify_proto_data(
            "Marine",
            &ProtoDataModification {
                data_type: ProtoDataType::Accuracy,
                amount: 2.0,
                relativity: ProtoDataRelativity::Percent,
                all_actions: false,
                name: Some("Rifle".to_owned()),
                invert: false,
                command_type: None,
                command_data: None,
            },
        );
        assert!((state.weapon_accuracy("Marine", "Rifle", 0.8) - 0.8).abs() < f32::EPSILON);
    }

    fn weapon_effect(
        subtype: &str,
        amount: f32,
        relativity: &str,
        action: Option<&str>,
        all_actions: bool,
    ) -> TechEffect {
        TechEffect {
            effect_type: "Data".to_owned(),
            subtype: Some(subtype.to_owned()),
            amount: Some(amount),
            relativity: Some(relativity.to_owned()),
            action: action.map(str::to_owned),
            allactions: Some(all_actions),
            target: Some(EffectTarget {
                target_type: Some("ProtoUnit".to_owned()),
                value: Some("Marine".to_owned()),
            }),
            ..TechEffect::default()
        }
    }
}
