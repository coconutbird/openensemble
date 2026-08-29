//! Scenario-layered prototype values used by retail squad veterancy.

use crate::entities::UnitScalarModifiers;
use num_traits::ToPrimitive;
use pipeline::database::hw1::objects::VeterancyLevel;
use pipeline::database::hw1::{Database, ProtoObject, Squad};
use std::collections::BTreeMap;

use super::GameplayCatalog;

#[derive(Debug, Clone, Default)]
pub(crate) struct VeterancyCatalog {
    objects: BTreeMap<String, ObjectVeterancyProfile>,
    squads: BTreeMap<String, SquadVeterancyProfile>,
}

#[derive(Debug, Clone, Default)]
struct ObjectVeterancyProfile {
    hitpoints: f32,
    bounty: f32,
    combat_value: f32,
    levels: Vec<VeterancyLevel>,
}

#[derive(Debug, Clone, Default)]
struct SquadVeterancyProfile {
    combat_value: f32,
    level_xp: Vec<f32>,
}

impl VeterancyCatalog {
    pub(crate) fn from_database(database: &Database) -> Self {
        let objects = database
            .objects
            .iter()
            .map(|object| {
                (
                    key(&object.name),
                    ObjectVeterancyProfile::from_object(object),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let squads = database
            .squads
            .iter()
            .map(|squad| (key(&squad.name), squad_profile(squad, &objects)))
            .collect();
        Self { objects, squads }
    }

    pub(crate) fn object_modifiers(
        &self,
        proto_object: &str,
        start_level: i32,
        target_level: i32,
    ) -> UnitScalarModifiers {
        self.objects
            .get(&key(proto_object))
            .map_or_else(UnitScalarModifiers::default, |profile| {
                UnitScalarModifiers::from_veterancy_levels(
                    &profile.levels,
                    start_level,
                    target_level,
                )
            })
    }

    pub(crate) fn join_modifiers(
        &self,
        joining_proto_object: &str,
        target_proto_squad: &str,
        damage_factor: f32,
        damage_taken_factor: f32,
    ) -> (f32, f32) {
        let joining = self
            .objects
            .get(&key(joining_proto_object))
            .map_or(0.0, |profile| profile.combat_value);
        let target = self.squad_combat_value(target_proto_squad);
        if joining <= 0.0 || target <= 0.0 {
            return (1.0, 1.0);
        }
        let ratio = joining / target;
        let damage = 1.0 + ratio * damage_factor;
        let damage_taken = if damage_taken_factor > 0.0 {
            1.0 / (1.0 + ratio / damage_taken_factor)
        } else {
            1.0
        };
        (damage, damage_taken)
    }

    pub(crate) fn squad_combat_value(&self, proto_squad: &str) -> f32 {
        self.squads
            .get(&key(proto_squad))
            .map_or(0.0, |profile| profile.combat_value)
    }

    pub(crate) fn squad_level_for_experience(
        &self,
        proto_squad: &str,
        current_level: i32,
        experience: f32,
    ) -> i32 {
        let Some(profile) = self.squads.get(&key(proto_squad)) else {
            return current_level.max(0);
        };
        let mut level = current_level.max(0);
        for &required in profile
            .level_xp
            .iter()
            .skip(usize::try_from(level).unwrap_or(usize::MAX))
        {
            if required == 0.0 || experience <= required {
                break;
            }
            level = level.saturating_add(1);
        }
        level
    }

    pub(crate) fn squad_level_thresholds(&self, proto_squad: &str) -> Option<&[f32]> {
        self.squads
            .get(&key(proto_squad))
            .map(|profile| profile.level_xp.as_slice())
    }

    pub(crate) fn bounty_experience(&self, proto_object: &str, hp_damage: f32) -> f32 {
        if !hp_damage.is_finite() || hp_damage <= 0.0 {
            return 0.0;
        }
        let Some(profile) = self.objects.get(&key(proto_object)) else {
            return 0.0;
        };
        if profile.hitpoints <= 0.0 || profile.bounty <= 0.0 {
            return 0.0;
        }
        (hp_damage / profile.hitpoints).min(1.0) * profile.bounty
    }
}

impl GameplayCatalog {
    /// Return the scenario-layered retail XP threshold for each squad level.
    #[must_use]
    pub fn squad_veterancy_thresholds(&self, proto_squad: &str) -> Option<&[f32]> {
        self.veterancy.squad_level_thresholds(proto_squad)
    }

    /// Return the summed combat value of the squad's authored members.
    #[must_use]
    pub fn squad_combat_value(&self, proto_squad: &str) -> f32 {
        self.veterancy.squad_combat_value(proto_squad)
    }

    pub(crate) fn squad_veterancy_level_for_experience(
        &self,
        proto_squad: &str,
        current_level: i32,
        experience: f32,
    ) -> i32 {
        self.veterancy
            .squad_level_for_experience(proto_squad, current_level, experience)
    }

    pub(crate) fn object_veterancy_modifiers(
        &self,
        proto_object: &str,
        start_level: i32,
        target_level: i32,
    ) -> UnitScalarModifiers {
        self.veterancy
            .object_modifiers(proto_object, start_level, target_level)
    }

    pub(crate) fn bounty_experience(&self, proto_object: &str, hp_damage: f32) -> f32 {
        self.veterancy.bounty_experience(proto_object, hp_damage)
    }
}

impl ObjectVeterancyProfile {
    fn from_object(object: &ProtoObject) -> Self {
        Self {
            hitpoints: finite_or_zero(object.hitpoints),
            bounty: finite_or_zero(object.bounty),
            combat_value: finite_or_zero(object.combat_value),
            levels: object.veterancy.clone(),
        }
    }
}

fn squad_profile(
    squad: &Squad,
    objects: &BTreeMap<String, ObjectVeterancyProfile>,
) -> SquadVeterancyProfile {
    let mut profile = SquadVeterancyProfile::default();
    let Some(units) = &squad.units else {
        return profile;
    };
    for member in &units.entries {
        let Some(object) = objects.get(&key(&member.proto_object)) else {
            continue;
        };
        let count = member.count.max(0).to_f32().unwrap_or_default();
        profile.combat_value += object.combat_value * count;
        add_object_level_thresholds(&mut profile.level_xp, &object.levels, count);
    }
    profile
}

fn add_object_level_thresholds(thresholds: &mut Vec<f32>, levels: &[VeterancyLevel], count: f32) {
    for level in levels {
        let xp = finite_or_zero(level.xp);
        if xp == 0.0 {
            break;
        }
        let Some(index) = level
            .level
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
        else {
            continue;
        };
        if thresholds.len() <= index {
            thresholds.resize(index + 1, 0.0);
        }
        thresholds[index] += xp * count;
    }
}

fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

    #[test]
    fn squad_thresholds_sum_each_member_count_and_keep_retail_strict_comparison() {
        let database = database();
        let catalog = VeterancyCatalog::from_database(&database);

        assert_eq!(
            catalog.squad_level_thresholds("TARGET_SQUAD"),
            Some(&[30.0, 60.0][..])
        );
        assert_eq!(
            catalog.squad_level_for_experience("target_squad", 0, 30.0),
            0
        );
        assert_eq!(
            catalog.squad_level_for_experience("target_squad", 0, 30.01),
            1
        );
        assert_eq!(
            catalog.squad_level_for_experience("target_squad", 0, 60.01),
            2
        );
    }

    #[test]
    fn combat_value_join_and_bounty_math_use_prototype_values() {
        let catalog = VeterancyCatalog::from_database(&database());
        let (damage, damage_taken) = catalog.join_modifiers("joiner", "target_squad", 0.8, 2.0);

        assert!((damage - 1.4).abs() < f32::EPSILON);
        assert!((damage_taken - 0.8).abs() < f32::EPSILON);
        assert!((catalog.bounty_experience("target", 25.0) - 20.0).abs() < f32::EPSILON);
        assert!((catalog.bounty_experience("target", 500.0) - 40.0).abs() < f32::EPSILON);
    }

    fn database() -> Database {
        Database {
            objects: vec![
                ProtoObject {
                    name: "target".to_owned(),
                    hitpoints: Some(50.0),
                    bounty: Some(40.0),
                    combat_value: Some(20.0),
                    veterancy: vec![
                        VeterancyLevel {
                            level: 1,
                            xp: Some(10.0),
                            ..VeterancyLevel::default()
                        },
                        VeterancyLevel {
                            level: 2,
                            xp: Some(20.0),
                            ..VeterancyLevel::default()
                        },
                    ],
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "joiner".to_owned(),
                    combat_value: Some(30.0),
                    ..ProtoObject::default()
                },
            ],
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
        }
    }
}
