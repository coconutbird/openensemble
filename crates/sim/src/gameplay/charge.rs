//! Persistent unit `Charge` profiles resolved from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use crate::spawn::object_prototype_id;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

/// Class-zero effect attached while one persistent Charge action is ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeEffectProfile {
    prototype_name: String,
    prototype_id: Option<i32>,
    bone_name: Option<String>,
}

impl ChargeEffectProfile {
    /// Return the authored effect prototype name.
    #[must_use]
    pub fn prototype_name(&self) -> &str {
        &self.prototype_name
    }

    /// Return the effect prototype's active layered-database ID.
    #[must_use]
    pub const fn prototype_id(&self) -> Option<i32> {
        self.prototype_id
    }

    /// Return the authored attachment bone, when one was provided.
    #[must_use]
    pub fn bone_name(&self) -> Option<&str> {
        self.bone_name.as_deref()
    }
}

/// Immutable authored inputs for one retail persistent `Charge` action.
#[derive(Debug, Clone, PartialEq)]
pub struct ChargeActionProfile {
    action_name: String,
    starts_disabled: bool,
    damage_charge: f32,
    animation_type: Option<String>,
    effect: Option<ChargeEffectProfile>,
    charge_on_taken: bool,
    charge_on_dealt: bool,
}

impl ChargeActionProfile {
    /// Return the authored action name used by technology and live enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Return whether the persistent action starts disabled.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    /// Return elapsed seconds required before a pull may replace attack damage.
    #[must_use]
    pub const fn damage_charge(&self) -> f32 {
        self.damage_charge
    }

    /// Return the attacker animation used for a charged pull attack.
    #[must_use]
    pub fn animation_type(&self) -> Option<&str> {
        self.animation_type.as_deref()
    }

    /// Return the ready-state effect attachment contract.
    #[must_use]
    pub const fn effect(&self) -> Option<&ChargeEffectProfile> {
        self.effect.as_ref()
    }

    /// Preserve retail's parsed but currently unused `ChargeOnTaken` flag.
    #[must_use]
    pub const fn charge_on_taken(&self) -> bool {
        self.charge_on_taken
    }

    /// Preserve retail's parsed but currently unused `ChargeOnDealt` flag.
    #[must_use]
    pub const fn charge_on_dealt(&self) -> bool {
        self.charge_on_dealt
    }
}

impl GameplayCatalog {
    /// Iterate persistent Charge actions for one effective object prototype.
    pub fn charge_actions(&self, proto_object_name: &str) -> &[ChargeActionProfile] {
        self.charge_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Return the first persistent Charge action in authored order.
    #[must_use]
    pub fn charge(&self, proto_object_name: &str) -> Option<&ChargeActionProfile> {
        self.charge_actions(proto_object_name).first()
    }
}

pub(super) fn collect_charge_actions(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<ChargeActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let profiles =
                rules
                    .persistent_actions
                    .iter()
                    .filter_map(|name| {
                        gameplay.tactics.actions.iter().find(|action| {
                            action.name.eq_ignore_ascii_case(name) && is_charge(action)
                        })
                    })
                    .map(|action| profile_from_action(database, action))
                    .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(database: &Database, action: &Action) -> ChargeActionProfile {
    ChargeActionProfile {
        action_name: action.name.clone(),
        starts_disabled: action.start_disabled == Some(true),
        damage_charge: action
            .damage_charge
            .filter(|seconds| seconds.is_finite())
            .unwrap_or_default(),
        animation_type: trimmed(
            action
                .anim
                .as_ref()
                .map(|animation| animation.name.as_str()),
        ),
        effect: action.proto_object.as_ref().and_then(|reference| {
            let prototype_name = reference.name.trim();
            (!prototype_name.is_empty()).then(|| ChargeEffectProfile {
                prototype_name: prototype_name.to_owned(),
                prototype_id: object_prototype_id(database, prototype_name),
                bone_name: trimmed(reference.bone.as_deref()),
            })
        }),
        charge_on_taken: action.charge_on_taken == Some(true),
        charge_on_dealt: action.charge_on_dealt == Some(true),
    }
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn is_charge(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Charge"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::ProtoObject;
    use pipeline::database::hw1::tactics::{AnimationRef, ProtoObjectRef, TacticData, TacticRules};

    #[test]
    fn compiles_persistent_charge_and_resolves_its_effect() {
        let database = Database {
            objects: vec![
                ProtoObject {
                    name: "chief".to_owned(),
                    tactics: Some("chief.tactics".to_owned()),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "charged_fx".to_owned(),
                    ..ProtoObject::default()
                },
            ],
            ..Database::default()
        };
        let action = Action {
            name: "ChargeAction".to_owned(),
            action_type: Some("Charge".to_owned()),
            start_disabled: Some(true),
            damage_charge: Some(10.0),
            anim: Some(AnimationRef {
                name: " Pull ".to_owned(),
                ..AnimationRef::default()
            }),
            proto_object: Some(ProtoObjectRef {
                name: " charged_fx ".to_owned(),
                bone: Some(" BoneFX ".to_owned()),
                ..ProtoObjectRef::default()
            }),
            charge_on_taken: Some(true),
            charge_on_dealt: Some(true),
            ..Action::default()
        };
        let catalog = GameplayCatalog::from_tactics(
            &database,
            [(
                "chief".to_owned(),
                TacticData {
                    actions: vec![action],
                    tactic: Some(TacticRules {
                        persistent_actions: vec!["ChargeAction".to_owned()],
                        ..TacticRules::default()
                    }),
                    ..TacticData::default()
                },
            )],
        );

        let profile = catalog.charge("chief").expect("compiled Charge");
        assert_eq!(profile.action_name(), "ChargeAction");
        assert!(profile.starts_disabled());
        assert!((profile.damage_charge() - 10.0).abs() < f32::EPSILON);
        assert_eq!(profile.animation_type(), Some("Pull"));
        assert!(profile.charge_on_taken());
        assert!(profile.charge_on_dealt());
        let effect = profile.effect().expect("ready effect");
        assert_eq!(effect.prototype_name(), "charged_fx");
        assert_eq!(effect.prototype_id(), Some(1));
        assert_eq!(effect.bone_name(), Some("BoneFX"));
    }
}
