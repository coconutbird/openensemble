//! Persistent player-owned prototype changes authored by trigger effects.

use crate::sync::SyncChecksum;

/// Campaign-authored `BTechEffect::DataType` values used by `ModifyProtoData`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ProtoDataType {
    Hitpoints = 1,
    Shieldpoints = 2,
    LineOfSight = 4,
    MaximumVelocity = 5,
    MaximumRange = 6,
    Damage = 14,
    AoePrimaryTargetFactor = 17,
    Accuracy = 20,
    MovingAccuracy = 21,
    MaxDeviation = 22,
    MovingMaxDeviation = 23,
    AccuracyDistanceFactor = 24,
    AccuracyDeviationFactor = 25,
    MaxVelocityLead = 26,
    BuildPoints = 28,
    CommandEnable = 34,
    ShieldRegenDelay = 38,
    Level = 48,
    CommandSelectable = 59,
}

impl ProtoDataType {
    pub(crate) fn from_trigger_value(value: &str) -> Option<Self> {
        if let Ok(ordinal) = value.trim().parse() {
            return Self::from_ordinal(ordinal);
        }
        [
            ("Hitpoints", Self::Hitpoints),
            ("Shieldpoints", Self::Shieldpoints),
            ("LOS", Self::LineOfSight),
            ("MaximumVelocity", Self::MaximumVelocity),
            ("MaximumRange", Self::MaximumRange),
            ("Damage", Self::Damage),
            ("AOEPrimaryTargetFactor", Self::AoePrimaryTargetFactor),
            ("Accuracy", Self::Accuracy),
            ("MovingAccuracy", Self::MovingAccuracy),
            ("MaxDeviation", Self::MaxDeviation),
            ("MovingMaxDeviation", Self::MovingMaxDeviation),
            ("AccuracyDistanceFactor", Self::AccuracyDistanceFactor),
            ("AccuracyDeviationFactor", Self::AccuracyDeviationFactor),
            ("MaxVelocityLead", Self::MaxVelocityLead),
            ("BuildPoints", Self::BuildPoints),
            ("CommandEnable", Self::CommandEnable),
            ("ShieldRegenDelay", Self::ShieldRegenDelay),
            ("Level", Self::Level),
            ("CommandSelectable", Self::CommandSelectable),
        ]
        .into_iter()
        .find_map(|(name, data_type)| name.eq_ignore_ascii_case(value.trim()).then_some(data_type))
    }

    fn from_ordinal(value: i32) -> Option<Self> {
        Some(match value {
            1 => Self::Hitpoints,
            2 => Self::Shieldpoints,
            4 => Self::LineOfSight,
            5 => Self::MaximumVelocity,
            6 => Self::MaximumRange,
            14 => Self::Damage,
            17 => Self::AoePrimaryTargetFactor,
            20 => Self::Accuracy,
            21 => Self::MovingAccuracy,
            22 => Self::MaxDeviation,
            23 => Self::MovingMaxDeviation,
            24 => Self::AccuracyDistanceFactor,
            25 => Self::AccuracyDeviationFactor,
            26 => Self::MaxVelocityLead,
            28 => Self::BuildPoints,
            34 => Self::CommandEnable,
            38 => Self::ShieldRegenDelay,
            48 => Self::Level,
            59 => Self::CommandSelectable,
            _ => return None,
        })
    }

    fn is_weapon_data(self) -> bool {
        matches!(
            self,
            Self::MaximumRange
                | Self::Damage
                | Self::AoePrimaryTargetFactor
                | Self::Accuracy
                | Self::MovingAccuracy
                | Self::MaxDeviation
                | Self::MovingMaxDeviation
                | Self::AccuracyDistanceFactor
                | Self::AccuracyDeviationFactor
                | Self::MaxVelocityLead
        )
    }
}

/// Retail `BTechEffect::Relativity` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ProtoDataRelativity {
    Absolute = 0,
    BasePercent = 1,
    Percent = 2,
    Assign = 3,
    BasePercentAssign = 4,
}

impl ProtoDataRelativity {
    pub(crate) fn from_trigger_value(value: &str) -> Option<Self> {
        if let Ok(ordinal) = value.trim().parse::<i32>() {
            return Some(match ordinal {
                0 => Self::Absolute,
                1 => Self::BasePercent,
                2 => Self::Percent,
                3 => Self::Assign,
                4 => Self::BasePercentAssign,
                _ => return None,
            });
        }
        [
            ("Absolute", Self::Absolute),
            ("BasePercent", Self::BasePercent),
            ("Percent", Self::Percent),
            ("Assign", Self::Assign),
            ("BasePercentAssign", Self::BasePercentAssign),
        ]
        .into_iter()
        .find_map(|(name, relativity)| {
            name.eq_ignore_ascii_case(value.trim())
                .then_some(relativity)
        })
    }
}

/// One resolved V4/V5 `ModifyProtoData` operation for one concrete prototype.
#[derive(Debug, Clone)]
pub(crate) struct ProtoDataModification {
    pub(crate) data_type: ProtoDataType,
    pub(crate) amount: f32,
    pub(crate) relativity: ProtoDataRelativity,
    pub(crate) all_actions: bool,
    pub(crate) name: Option<String>,
    pub(crate) invert: bool,
    pub(crate) command_type: Option<String>,
    pub(crate) command_data: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct RuntimeProtoData {
    scalar_effects: Vec<RuntimeScalarEffect>,
    command_effects: Vec<RuntimeCommandEffect>,
}

#[derive(Debug, Clone)]
struct RuntimeScalarEffect {
    proto_object: String,
    data_type: ProtoDataType,
    weapon: Option<String>,
    operation: RuntimeScalarOperation,
}

#[derive(Debug, Clone)]
struct RuntimeCommandEffect {
    proto_object: String,
    data_type: ProtoDataType,
    command_type: String,
    command_data: String,
    enabled: bool,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeScalarOperation {
    amount: f32,
    relativity: ProtoDataRelativity,
    invert: bool,
}

impl RuntimeProtoData {
    pub(super) fn record(&mut self, proto_object: &str, modification: &ProtoDataModification) {
        if matches!(
            modification.data_type,
            ProtoDataType::CommandEnable | ProtoDataType::CommandSelectable
        ) {
            self.record_command(proto_object, modification);
            return;
        }
        let weapon = if modification.data_type.is_weapon_data() && !modification.all_actions {
            let Some(name) = modification.name.as_deref().and_then(nonempty) else {
                return;
            };
            Some(normalize(name))
        } else {
            None
        };
        self.scalar_effects.push(RuntimeScalarEffect {
            proto_object: normalize(proto_object),
            data_type: modification.data_type,
            weapon,
            operation: RuntimeScalarOperation {
                amount: modification.amount,
                relativity: modification.relativity,
                invert: modification.invert,
            },
        });
    }

    fn record_command(&mut self, proto_object: &str, modification: &ProtoDataModification) {
        let (Some(command_type), Some(command_data)) = (
            modification.command_type.as_deref().and_then(nonempty),
            modification.command_data.as_deref().and_then(nonempty),
        ) else {
            return;
        };
        self.command_effects.push(RuntimeCommandEffect {
            proto_object: normalize(proto_object),
            data_type: modification.data_type,
            command_type: normalize(command_type),
            command_data: normalize(command_data),
            enabled: if modification.invert {
                modification.amount <= 0.0
            } else {
                modification.amount > 0.0
            },
        });
    }

    pub(super) fn scalar(
        &self,
        data_type: ProtoDataType,
        proto_object: &str,
        weapon: Option<&str>,
        base: f32,
        current: f32,
    ) -> f32 {
        self.scalar_effects
            .iter()
            .filter(|effect| {
                effect.data_type == data_type
                    && effect.proto_object.eq_ignore_ascii_case(proto_object)
                    && effect.weapon.as_deref().is_none_or(|required| {
                        weapon.is_some_and(|actual| required.eq_ignore_ascii_case(actual))
                    })
            })
            .fold(current, |value, effect| effect.operation.apply(base, value))
    }

    pub(super) fn command_state(
        &self,
        data_type: ProtoDataType,
        proto_object: &str,
        command_type: &str,
        command_data: &str,
        authored: bool,
    ) -> bool {
        self.command_effects
            .iter()
            .filter(|effect| {
                effect.data_type == data_type
                    && effect.proto_object.eq_ignore_ascii_case(proto_object)
                    && effect.command_type.eq_ignore_ascii_case(command_type)
                    && effect.command_data.eq_ignore_ascii_case(command_data)
            })
            .fold(authored, |_, effect| effect.enabled)
    }

    pub(super) fn modification_count(&self) -> usize {
        self.scalar_effects.len() + self.command_effects.len()
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.scalar_effects.len()).unwrap_or(u32::MAX));
        for effect in &self.scalar_effects {
            hash_string(checksum, &effect.proto_object);
            checksum.hash_u32(u32::from(effect.data_type as u8));
            hash_optional_string(checksum, effect.weapon.as_deref());
            checksum.hash_f32(effect.operation.amount);
            checksum.hash_u32(u32::from(effect.operation.relativity as u8));
            checksum.hash_u32(u32::from(effect.operation.invert));
        }
        checksum.hash_u32(u32::try_from(self.command_effects.len()).unwrap_or(u32::MAX));
        for effect in &self.command_effects {
            hash_string(checksum, &effect.proto_object);
            checksum.hash_u32(u32::from(effect.data_type as u8));
            hash_string(checksum, &effect.command_type);
            hash_string(checksum, &effect.command_data);
            checksum.hash_u32(u32::from(effect.enabled));
        }
    }
}

impl RuntimeScalarOperation {
    fn apply(self, base: f32, current: f32) -> f32 {
        match (self.relativity, self.invert) {
            (ProtoDataRelativity::Absolute, false) => current + self.amount,
            (ProtoDataRelativity::Absolute, true) => current - self.amount,
            (ProtoDataRelativity::BasePercent, false) => current + base.mul_add(self.amount, -base),
            (ProtoDataRelativity::BasePercent, true) => current - base.mul_add(self.amount, -base),
            (ProtoDataRelativity::Percent, false) => current * self.amount,
            (ProtoDataRelativity::Percent, true) => current / self.amount,
            (ProtoDataRelativity::Assign, _) => self.amount,
            (ProtoDataRelativity::BasePercentAssign, false) => base * self.amount,
            (ProtoDataRelativity::BasePercentAssign, true) => base / self.amount,
        }
    }
}

fn nonempty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value.trim())
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn hash_optional_string(checksum: &mut SyncChecksum, value: Option<&str>) {
    checksum.hash_u32(u32::from(value.is_some()));
    if let Some(value) = value {
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

    #[test]
    fn projectile_accuracy_types_keep_retail_ordinals_and_names() {
        for (ordinal, name, expected) in [
            (20, "Accuracy", ProtoDataType::Accuracy),
            (21, "MovingAccuracy", ProtoDataType::MovingAccuracy),
            (22, "MaxDeviation", ProtoDataType::MaxDeviation),
            (23, "MovingMaxDeviation", ProtoDataType::MovingMaxDeviation),
            (
                24,
                "AccuracyDistanceFactor",
                ProtoDataType::AccuracyDistanceFactor,
            ),
            (
                25,
                "AccuracyDeviationFactor",
                ProtoDataType::AccuracyDeviationFactor,
            ),
            (26, "MaxVelocityLead", ProtoDataType::MaxVelocityLead),
        ] {
            assert_eq!(ProtoDataType::from_ordinal(ordinal), Some(expected));
            assert_eq!(ProtoDataType::from_trigger_value(name), Some(expected));
        }
    }

    #[test]
    fn inverse_operations_match_retail_calc_amount() {
        let operation = |relativity, invert| RuntimeScalarOperation {
            amount: 1.5,
            relativity,
            invert,
        };
        assert_close(
            operation(ProtoDataRelativity::Absolute, true).apply(10.0, 20.0),
            18.5,
        );
        assert_close(
            operation(ProtoDataRelativity::BasePercent, true).apply(10.0, 20.0),
            15.0,
        );
        assert_close(
            operation(ProtoDataRelativity::Percent, true).apply(10.0, 20.0),
            20.0 / 1.5,
        );
        assert_close(
            operation(ProtoDataRelativity::Assign, true).apply(10.0, 20.0),
            1.5,
        );
        assert_close(
            operation(ProtoDataRelativity::BasePercentAssign, true).apply(10.0, 20.0),
            10.0 / 1.5,
        );
    }

    #[test]
    fn all_actions_and_named_weapons_keep_retail_order() {
        let mut state = RuntimeProtoData::default();
        state.record(
            "marine",
            &ProtoDataModification {
                data_type: ProtoDataType::Damage,
                amount: 2.0,
                relativity: ProtoDataRelativity::Percent,
                all_actions: true,
                name: None,
                invert: false,
                command_type: None,
                command_data: None,
            },
        );
        state.record(
            "marine",
            &ProtoDataModification {
                data_type: ProtoDataType::Damage,
                amount: 3.0,
                relativity: ProtoDataRelativity::Absolute,
                all_actions: false,
                name: Some("Rifle".to_owned()),
                invert: false,
                command_type: None,
                command_data: None,
            },
        );

        assert_close(
            state.scalar(ProtoDataType::Damage, "marine", Some("Rifle"), 5.0, 5.0),
            13.0,
        );
        assert_close(
            state.scalar(ProtoDataType::Damage, "marine", Some("Pistol"), 5.0, 5.0),
            10.0,
        );
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
    }
}
