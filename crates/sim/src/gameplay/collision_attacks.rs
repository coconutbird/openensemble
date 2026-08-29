//! Immutable collision-attack definitions and per-armor Ram behavior.

use super::{GameplayCatalog, ObjectGameplay, TacticStateId};
use crate::entities::SquadMode;
use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, Weapon};

/// One persistent `CollisionAttack` action joined to its authored Ram weapon.
#[derive(Debug, Clone, Copy)]
pub struct CollisionAttackProfile<'gameplay> {
    /// Tactic action name.
    pub action_name: &'gameplay str,
    /// Tactic weapon name used by technology effects.
    pub weapon_name: &'gameplay str,
    /// Weapon-type table used for armor, bowl, ram, and reflection rules.
    pub weapon_type: Option<&'gameplay str>,
    /// Radius around the ordered target in which collateral infantry may be bowled.
    pub area_radius: f32,
    /// Authored per-impact damage cap after scenario database layering.
    pub max_damage_per_ram: f32,
    /// Weapon-level reflection scalar before target-armor reflection.
    pub reflect_damage_factor: f32,
    /// Authored state entered while the persistent collision action is active.
    pub new_tactic_state: Option<TacticStateId>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CollisionTargetProfile {
    pub reflect_damage_factor: f32,
    pub bowlable: bool,
    pub rammable: bool,
}

impl GameplayCatalog {
    /// Resolve a prototype's first authored collision attack.
    #[must_use]
    pub fn collision_attack(&self, proto_object_name: &str) -> Option<CollisionAttackProfile<'_>> {
        let object = self.object(proto_object_name)?;
        let (action, weapon) = collision_action(object)?;
        Some(CollisionAttackProfile {
            action_name: &action.name,
            weapon_name: &weapon.name,
            weapon_type: weapon.weapon_type.as_deref(),
            area_radius: finite_nonnegative(weapon.aoe_radius),
            max_damage_per_ram: finite_nonnegative(weapon.max_damage_per_ram),
            reflect_damage_factor: finite_nonnegative(weapon.reflect_damage_factor),
            new_tactic_state: action
                .new_tactic_state
                .as_deref()
                .and_then(|name| object.tactic_state_id(name)),
        })
    }

    pub(crate) fn collision_target_profile(
        &self,
        weapon_type: Option<&str>,
        target_proto: &str,
        direction: Vec3,
        target_forward: Vec3,
        target_mode: SquadMode,
    ) -> CollisionTargetProfile {
        let Some(weapon_type) = weapon_type else {
            return CollisionTargetProfile::default();
        };
        let Some(damage_type) =
            self.directional_damage_type(target_proto, direction, target_forward, target_mode)
        else {
            return CollisionTargetProfile::default();
        };
        self.weapon_damage_modifiers
            .get(&weapon_type.to_ascii_lowercase())
            .and_then(|modifiers| modifiers.get(&damage_type.to_ascii_lowercase()))
            .map_or_else(CollisionTargetProfile::default, |modifier| {
                CollisionTargetProfile {
                    reflect_damage_factor: finite_nonnegative(Some(modifier.reflect_damage_factor)),
                    bowlable: modifier.bowlable,
                    rammable: modifier.rammable,
                }
            })
    }
}

fn collision_action(object: &ObjectGameplay) -> Option<(&Action, &Weapon)> {
    let action = object.tactics.actions.iter().find(|action| {
        action
            .action_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("CollisionAttack"))
    })?;
    let weapon_name = action.weapon.as_deref()?;
    let weapon = object
        .tactics
        .weapons
        .iter()
        .find(|weapon| weapon.name.eq_ignore_ascii_case(weapon_name))?;
    Some((action, weapon))
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::tactics::TacticData;
    use pipeline::database::hw1::{Database, ProtoObject};

    #[test]
    fn absent_ram_cap_keeps_retail_zero_default_and_authored_metadata() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "rammer".to_owned(),
            tactics: Some("rammer.tactics".to_owned()),
            ..ProtoObject::default()
        });
        let tactics = TacticData {
            weapons: vec![Weapon {
                name: "Ram".to_owned(),
                weapon_type: Some("WarthogRam".to_owned()),
                aoe_radius: Some(40.0),
                ..Weapon::default()
            }],
            actions: vec![Action {
                name: "PersistentCollisionAttack".to_owned(),
                action_type: Some("CollisionAttack".to_owned()),
                weapon: Some("Ram".to_owned()),
                ..Action::default()
            }],
            ..TacticData::default()
        };
        let catalog = GameplayCatalog::from_tactics(&database, [("rammer".to_owned(), tactics)]);

        let profile = catalog.collision_attack("RAMMER").unwrap();
        assert_eq!(profile.action_name, "PersistentCollisionAttack");
        assert_eq!(profile.weapon_name, "Ram");
        assert_eq!(profile.weapon_type, Some("WarthogRam"));
        assert_close(profile.area_radius, 40.0);
        assert_close(profile.max_damage_per_ram, 0.0);
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
    }
}
