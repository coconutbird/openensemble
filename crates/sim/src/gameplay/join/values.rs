//! Scenario-layered database values consumed by retail Join actions.

use super::JoinActionProfile;
use crate::entities::UnitScalarModifiers;
use num_traits::ToPrimitive;
use pipeline::database::hw1::tactics::ProtoObjectRef;
use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct JoinAttachmentProfile {
    proto_object_id: i32,
    proto_object_name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct JoinDatabaseProfiles {
    object_combat_values: BTreeMap<String, f32>,
    squad_combat_values: BTreeMap<String, f32>,
    attachment_objects: BTreeMap<String, JoinAttachmentProfile>,
    veterancy_levels: BTreeMap<String, Vec<pipeline::database::hw1::objects::VeterancyLevel>>,
}

impl JoinAttachmentProfile {
    pub(crate) const fn proto_object_id(&self) -> i32 {
        self.proto_object_id
    }

    pub(crate) fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }
}

impl JoinDatabaseProfiles {
    pub(crate) fn from_database(database: &Database) -> Self {
        let object_combat_values = database
            .objects
            .iter()
            .map(|object| {
                (
                    object.name.to_ascii_lowercase(),
                    finite_or_zero(object.combat_value),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let squad_combat_values = database
            .squads
            .iter()
            .map(|squad| {
                let combat_value = squad.units.as_ref().map_or(0.0, |units| {
                    units.entries.iter().fold(0.0, |total, member| {
                        let unit_value = object_combat_values
                            .get(&member.proto_object.to_ascii_lowercase())
                            .copied()
                            .unwrap_or_default();
                        total + unit_value * member.count.max(0).to_f32().unwrap_or_default()
                    })
                });
                (squad.name.to_ascii_lowercase(), combat_value)
            })
            .collect();
        let attachment_objects = database
            .objects
            .iter()
            .enumerate()
            .filter(|(_, object)| is_attachment_object(object))
            .map(|(index, object)| {
                (
                    object.name.to_ascii_lowercase(),
                    JoinAttachmentProfile {
                        proto_object_id: database_id(object, index),
                        proto_object_name: object.name.clone(),
                    },
                )
            })
            .collect();
        let veterancy_levels = database
            .objects
            .iter()
            .map(|object| (object.name.to_ascii_lowercase(), object.veterancy.clone()))
            .collect();
        Self {
            object_combat_values,
            squad_combat_values,
            attachment_objects,
            veterancy_levels,
        }
    }

    pub(crate) fn resolve_damage_modifiers(
        &self,
        action: &JoinActionProfile,
        joining_proto_object: &str,
        target_proto_squad: &str,
    ) -> (f32, f32) {
        if !action.damage_by_combat_value() {
            return (action.damage_modifier(), action.damage_taken_modifier());
        }
        let joining = self
            .object_combat_values
            .get(&joining_proto_object.to_ascii_lowercase())
            .copied()
            .unwrap_or_default();
        let target = self
            .squad_combat_values
            .get(&target_proto_squad.to_ascii_lowercase())
            .copied()
            .unwrap_or_default();
        if joining <= 0.0 || target <= 0.0 {
            return (1.0, 1.0);
        }
        let ratio = joining / target;
        let damage = 1.0 + ratio * action.damage_modifier();
        let damage_taken = if action.damage_taken_modifier() > 0.0 {
            1.0 / (1.0 + ratio / action.damage_taken_modifier())
        } else {
            1.0
        };
        (damage, damage_taken)
    }

    pub(crate) fn resolve_attachment(
        &self,
        reference: Option<&ProtoObjectRef>,
    ) -> Option<&JoinAttachmentProfile> {
        let reference = reference?;
        if reference.squad.is_some() {
            return None;
        }
        self.attachment_objects
            .get(&reference.name.trim().to_ascii_lowercase())
    }

    pub(crate) fn veterancy_modifiers(
        &self,
        proto_object: &str,
        start_level: i32,
        target_level: i32,
    ) -> UnitScalarModifiers {
        self.veterancy_levels
            .get(&proto_object.to_ascii_lowercase())
            .map_or_else(UnitScalarModifiers::default, |levels| {
                UnitScalarModifiers::from_veterancy_levels(levels, start_level, target_level)
            })
    }
}

fn is_attachment_object(object: &ProtoObject) -> bool {
    object
        .object_class
        .as_deref()
        .is_none_or(|class| class.trim().eq_ignore_ascii_case("Object"))
}

fn database_id(object: &ProtoObject, index: usize) -> i32 {
    object
        .dbid
        .unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::Squad;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
    use pipeline::database::hw1::tactics::DamageModifiers;

    #[test]
    fn combat_value_modifiers_follow_retail_ratio_math() {
        let database = Database {
            objects: vec![object("joiner", 30.0), object("target", 20.0)],
            squads: vec![Squad {
                name: "target_squad".to_owned(),
                units: Some(UnitsWrapper {
                    entries: vec![UnitEntry {
                        proto_object: "target".to_owned(),
                        count: 3,
                        ..UnitEntry::default()
                    }],
                }),
                ..Squad::default()
            }],
            ..Database::default()
        };
        let action = JoinActionProfile::from_action(&pipeline::database::hw1::tactics::Action {
            damage_modifiers: Some(DamageModifiers {
                damage: Some(0.8),
                damage_taken: Some(2.0),
                by_combat_value: Some(true),
            }),
            ..Default::default()
        });
        let profiles = JoinDatabaseProfiles::from_database(&database);

        let (damage, damage_taken) =
            profiles.resolve_damage_modifiers(&action, "joiner", "target_squad");
        assert!((damage - 1.4).abs() < f32::EPSILON);
        assert!((damage_taken - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn combat_value_mode_keeps_identity_when_either_value_is_zero() {
        let action = JoinActionProfile::from_action(&pipeline::database::hw1::tactics::Action {
            damage_modifiers: Some(DamageModifiers {
                damage: Some(4.0),
                damage_taken: Some(0.2),
                by_combat_value: Some(true),
            }),
            ..Default::default()
        });
        assert_eq!(
            JoinDatabaseProfiles::default().resolve_damage_modifiers(&action, "missing", "missing"),
            (1.0, 1.0)
        );
    }

    #[test]
    fn omitted_object_class_uses_retail_class_zero_attachment_default() {
        let database = Database {
            objects: vec![ProtoObject {
                name: "fx_hijacked".to_owned(),
                dbid: Some(3883),
                ..ProtoObject::default()
            }],
            ..Database::default()
        };
        let action = JoinActionProfile::from_action(&pipeline::database::hw1::tactics::Action {
            proto_object: Some(ProtoObjectRef {
                name: "FX_HIJACKED".to_owned(),
                ..ProtoObjectRef::default()
            }),
            ..Default::default()
        });
        let profiles = JoinDatabaseProfiles::from_database(&database);
        let attachment = profiles
            .resolve_attachment(action.attachment())
            .expect("an omitted ObjectClass retains retail's class-zero default");

        assert_eq!(attachment.proto_object_id(), 3883);
        assert_eq!(attachment.proto_object_name(), "fx_hijacked");
    }

    fn object(name: &str, combat_value: f32) -> ProtoObject {
        ProtoObject {
            name: name.to_owned(),
            combat_value: Some(combat_value),
            ..ProtoObject::default()
        }
    }
}
