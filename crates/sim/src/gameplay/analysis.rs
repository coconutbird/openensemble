//! Immutable tactic calculations used by retail AI squad analysis.

use super::damage_types::DamageTypeProfiles;
use super::{GameplayCatalog, ObjectGameplay, selection};
use crate::player::PlayerTechState;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::tactics::{Action, TargetRule, Weapon};
use std::collections::BTreeMap;

pub(crate) const AI_DAMAGE_TYPES: [&str; 6] = [
    "Light",
    "LightArmored",
    "Medium",
    "MediumAir",
    "Heavy",
    "Building",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AIActionKind {
    Ranged,
    SecondaryTurret,
    SlaveTurret,
}

impl GameplayCatalog {
    /// Reproduce the tactic attack-rating buckets consumed by retail AI.
    pub(crate) fn ai_attack_ratings(
        &self,
        proto_object_name: &str,
        technologies: &PlayerTechState,
    ) -> [f32; AI_DAMAGE_TYPES.len()] {
        let Some(object) = self.object(proto_object_name) else {
            return [0.0; AI_DAMAGE_TYPES.len()];
        };
        let mut ratings = [0.0; AI_DAMAGE_TYPES.len()];
        for action in &object.tactics.actions {
            let Some(kind) = ai_action_kind(action) else {
                continue;
            };
            let Some(weapon) = resolve_weapon(object, action) else {
                continue;
            };
            add_action_ratings(
                self,
                object,
                action,
                weapon,
                kind,
                technologies,
                &mut ratings,
            );
        }
        ratings
    }
}

pub(super) fn collect_damage_type_exemplars(
    database: &Database,
    profiles: &DamageTypeProfiles,
) -> BTreeMap<String, String> {
    let mut exemplars = BTreeMap::new();
    for damage_type in AI_DAMAGE_TYPES {
        if let Some(object) = database.objects.iter().find(|object| {
            profiles
                .base_damage_type(&object.name)
                .or(object.damage_type.as_deref())
                .is_some_and(|kind| kind.eq_ignore_ascii_case(damage_type))
        }) {
            exemplars.insert(damage_type.to_ascii_lowercase(), object.name.clone());
        }
    }
    if let Some(warthog) = database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case("unsc_veh_warthog_01"))
    {
        exemplars.insert("medium".to_owned(), warthog.name.clone());
    }
    exemplars
}

fn add_action_ratings(
    catalog: &GameplayCatalog,
    object: &ObjectGameplay,
    action: &Action,
    weapon: &Weapon,
    kind: AIActionKind,
    technologies: &PlayerTechState,
    ratings: &mut [f32; AI_DAMAGE_TYPES.len()],
) {
    let authored_enabled = action.start_disabled != Some(true);
    if !technologies.action_enabled(&object.proto_object_name, &action.name, authored_enabled) {
        return;
    }
    let rule = matching_rule(object, action);
    if rule.is_some_and(special_mode_rule) {
        return;
    }
    let mut damage = weapon.damage_per_second.unwrap_or_default();
    damage = technologies.weapon_damage(&object.proto_object_name, &weapon.name, damage);
    if rule.is_some_and(has_ability) || weapon.use_dps_as_dpa == Some(true) {
        damage /= 25.0;
    }
    for (index, damage_type) in AI_DAMAGE_TYPES.into_iter().enumerate() {
        if !action_allows_exemplar(catalog, rule, kind, weapon, damage_type) {
            continue;
        }
        let modifier = weapon_modifier(catalog, weapon, damage_type, technologies);
        ratings[index] += damage * modifier.damage * modifier.rating;
    }
}

fn resolve_weapon<'object>(
    object: &'object ObjectGameplay,
    action: &Action,
) -> Option<&'object Weapon> {
    let weapon_name = action.weapon.as_deref()?;
    object
        .tactics
        .weapons
        .iter()
        .find(|weapon| weapon.name.eq_ignore_ascii_case(weapon_name))
}

fn matching_rule<'object>(
    object: &'object ObjectGameplay,
    action: &Action,
) -> Option<&'object TargetRule> {
    object
        .tactics
        .tactic
        .as_ref()?
        .target_rules
        .iter()
        .find(|rule| {
            rule.action
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&action.name))
        })
}

fn special_mode_rule(rule: &TargetRule) -> bool {
    rule.squad_mode.as_deref().is_some_and(|mode| {
        mode.eq_ignore_ascii_case("Lockdown")
            || mode.eq_ignore_ascii_case("Cover")
            || mode.eq_ignore_ascii_case("CarryingObject")
    })
}

fn has_ability(rule: &TargetRule) -> bool {
    rule.ability.is_some() || rule.optional_ability.is_some()
}

fn action_allows_exemplar(
    catalog: &GameplayCatalog,
    rule: Option<&TargetRule>,
    kind: AIActionKind,
    weapon: &Weapon,
    damage_type: &str,
) -> bool {
    let Some(exemplar) = catalog
        .damage_type_exemplars
        .get(&damage_type.to_ascii_lowercase())
    else {
        return true;
    };
    match kind {
        AIActionKind::Ranged | AIActionKind::SecondaryTurret => rule.is_some_and(|rule| {
            rule.target_types.is_empty()
                || rule.target_types.iter().any(|target_type| {
                    selection::proto_matches_type(catalog, exemplar, target_type)
                })
        }),
        AIActionKind::SlaveTurret => target_priority(catalog, exemplar, weapon) > 0.0,
    }
}

fn target_priority(catalog: &GameplayCatalog, exemplar: &str, weapon: &Weapon) -> f32 {
    weapon
        .target_priorities
        .iter()
        .filter(|priority| selection::proto_matches_type(catalog, exemplar, &priority.target_type))
        .map(|priority| priority.priority)
        .sum()
}

fn weapon_modifier(
    catalog: &GameplayCatalog,
    weapon: &Weapon,
    damage_type: &str,
    technologies: &PlayerTechState,
) -> super::WeaponDamageModifier {
    let Some(weapon_type) = weapon.weapon_type.as_deref() else {
        return default_weapon_modifier();
    };
    let Some(mut modifier) = catalog
        .weapon_damage_modifiers
        .get(&weapon_type.to_ascii_lowercase())
        .and_then(|modifiers| modifiers.get(&damage_type.to_ascii_lowercase()))
        .copied()
    else {
        return default_weapon_modifier();
    };
    modifier.damage =
        technologies.weapon_type_damage_modifier(weapon_type, damage_type, modifier.damage);
    modifier
}

fn default_weapon_modifier() -> super::WeaponDamageModifier {
    super::WeaponDamageModifier {
        damage: 1.0,
        rating: 1.0,
    }
}

fn ai_action_kind(action: &Action) -> Option<AIActionKind> {
    let kind = action.action_type.as_deref()?;
    if kind.eq_ignore_ascii_case("RangedAttack") {
        Some(AIActionKind::Ranged)
    } else if kind.eq_ignore_ascii_case("SecondaryTurretAttack") {
        Some(AIActionKind::SecondaryTurret)
    } else if kind.eq_ignore_ascii_case("SlaveTurretAttack") {
        Some(AIActionKind::SlaveTurret)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::tactics::{TacticData, TacticRules};
    use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper, TechEffect};
    use pipeline::database::hw1::weapontypes::DamageModifier;
    use pipeline::database::hw1::{ProtoObject, Tech, WeaponType};

    #[test]
    fn ratings_use_damage_percentage_rating_and_retail_rule_gating() {
        let mut database = Database::new();
        database.objects.extend([
            ProtoObject {
                name: "attacker".to_owned(),
                tactics: Some("attacker.tactics".to_owned()),
                damage_type: Some("Light".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "medium_target".to_owned(),
                damage_type: Some("Medium".to_owned()),
                object_types: vec!["Vehicle".to_owned()],
                ..ProtoObject::default()
            },
        ]);
        database.weapon_types.push(WeaponType {
            name: "AntiVehicle".to_owned(),
            damage_modifiers: vec![DamageModifier {
                damage_type: "Medium".to_owned(),
                rating: Some(2.0),
                modifier: 1.5,
                ..DamageModifier::default()
            }],
            ..WeaponType::default()
        });
        let technology = Tech {
            name: "AntiVehicleDamage".to_owned(),
            effects: Some(EffectsWrapper {
                entries: vec![TechEffect {
                    effect_type: "Data".to_owned(),
                    subtype: Some("DamageModifier".to_owned()),
                    amount: Some(2.0),
                    relativity: Some("Percent".to_owned()),
                    weapon_type: Some("AntiVehicle".to_owned()),
                    damage_type: Some("Medium".to_owned()),
                    target: Some(EffectTarget {
                        target_type: Some("Player".to_owned()),
                        value: Some("Player".to_owned()),
                    }),
                    ..TechEffect::default()
                }],
            }),
            ..Tech::default()
        };
        database.techs.push(technology.clone());
        let tactics = TacticData {
            weapons: vec![Weapon {
                name: "Gun".to_owned(),
                damage_per_second: Some(10.0),
                weapon_type: Some("AntiVehicle".to_owned()),
                ..Weapon::default()
            }],
            actions: vec![Action {
                name: "Attack".to_owned(),
                action_type: Some("RangedAttack".to_owned()),
                weapon: Some("Gun".to_owned()),
                ..Action::default()
            }],
            tactic: Some(TacticRules {
                target_rules: vec![TargetRule {
                    action: Some("Attack".to_owned()),
                    target_types: vec!["Vehicle".to_owned()],
                    ..TargetRule::default()
                }],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        let catalog = GameplayCatalog::from_tactics(&database, [("attacker".to_owned(), tactics)]);

        let ratings = catalog.ai_attack_ratings("attacker", &PlayerTechState::default());
        assert!(ratings[0].abs() < f32::EPSILON);
        assert!((ratings[2] - 30.0).abs() < f32::EPSILON);

        let mut technologies = PlayerTechState::default();
        let _transforms = technologies.activate(&database, &technology);
        let ratings = catalog.ai_attack_ratings("attacker", &technologies);
        assert!((ratings[2] - 60.0).abs() < f32::EPSILON);
    }
}
