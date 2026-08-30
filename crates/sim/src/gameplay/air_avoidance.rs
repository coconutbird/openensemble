//! Persistent aircraft collision avoidance and crash profiles.

use super::{AreaDamageProfile, GameplayCatalog, ImpactEffectProfile, ObjectGameplay};
use pipeline::database::hw1::tactics::{Action, Weapon};
use std::collections::BTreeMap;

const DEFAULT_MAX_TARGET_DEPRESSION_ANGLE: f32 = 50.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AirAvoidanceProfileFlags(u8);

impl AirAvoidanceProfileFlags {
    const STARTS_DISABLED: u8 = 1 << 0;
    const STATIONARY: u8 = 1 << 1;
    const AVOID_ONLY: u8 = 1 << 2;
    const DETONATE_ON_DEATH: u8 = 1 << 3;

    fn from_action(action: &Action) -> Self {
        let mut flags = Self::default();
        flags.set(Self::STARTS_DISABLED, action.start_disabled == Some(true));
        flags.set(Self::STATIONARY, action.stationary == Some(true));
        flags.set(Self::AVOID_ONLY, action.avoid_only == Some(true));
        flags.set(
            Self::DETONATE_ON_DEATH,
            action.detonate_on_death == Some(true),
        );
        flags
    }

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

/// Immutable weapon values used when a dying aircraft crashes.
#[derive(Debug, Clone, PartialEq)]
pub struct KamikazeWeaponProfile {
    damage: f32,
    max_range: f32,
    weapon_type: Option<String>,
    area_damage: Option<AreaDamageProfile>,
    impact_effect: Option<ImpactEffectProfile>,
}

impl KamikazeWeaponProfile {
    /// Damage applied by the crash action before live modifiers.
    #[must_use]
    pub const fn damage(&self) -> f32 {
        self.damage
    }

    /// Radius used to choose a ground kamikaze target in front of the aircraft.
    #[must_use]
    pub const fn max_range(&self) -> f32 {
        self.max_range
    }

    /// Authored weapon type used by target armor modifiers.
    #[must_use]
    pub fn weapon_type(&self) -> Option<&str> {
        self.weapon_type.as_deref()
    }

    /// Authored crash splash distribution.
    #[must_use]
    pub const fn area_damage(&self) -> Option<AreaDamageProfile> {
        self.area_damage
    }

    /// Renderer-facing impact effect emitted by the authoritative crash.
    #[must_use]
    pub const fn impact_effect(&self) -> Option<&ImpactEffectProfile> {
        self.impact_effect.as_ref()
    }
}

/// Immutable inputs for one persistent retail `AvoidCollisionAir` action.
#[derive(Debug, Clone, PartialEq)]
pub struct AirAvoidanceActionProfile {
    action_name: String,
    flags: AirAvoidanceProfileFlags,
    hover_altitude_offset: f32,
    max_target_depression_angle: f32,
    kamikaze_weapon: Option<KamikazeWeaponProfile>,
}

impl AirAvoidanceActionProfile {
    /// Authored action name used by technology and live enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Whether the action begins disabled.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.flags
            .contains(AirAvoidanceProfileFlags::STARTS_DISABLED)
    }

    /// Whether nearby flying units ignore this stationary obstruction.
    #[must_use]
    pub const fn stationary(&self) -> bool {
        self.flags.contains(AirAvoidanceProfileFlags::STATIONARY)
    }

    /// Whether this action avoids other spots without claiming its own.
    #[must_use]
    pub const fn avoid_only(&self) -> bool {
        self.flags.contains(AirAvoidanceProfileFlags::AVOID_ONLY)
    }

    /// Whether lethal damage skips the aircraft crash lifecycle.
    #[must_use]
    pub const fn detonate_on_death(&self) -> bool {
        self.flags
            .contains(AirAvoidanceProfileFlags::DETONATE_ON_DEATH)
    }

    /// Vertical offset consumed by hover-flight movement.
    #[must_use]
    pub const fn hover_altitude_offset(&self) -> f32 {
        self.hover_altitude_offset
    }

    /// Maximum attack depression angle in authored degrees.
    #[must_use]
    pub const fn max_target_depression_angle(&self) -> f32 {
        self.max_target_depression_angle
    }

    /// Optional weapon that turns a generic crash into a targeted kamikaze.
    #[must_use]
    pub const fn kamikaze_weapon(&self) -> Option<&KamikazeWeaponProfile> {
        self.kamikaze_weapon.as_ref()
    }
}

impl GameplayCatalog {
    /// Iterate persistent air-avoidance actions in authored tactic order.
    pub fn air_avoidance_actions(&self, proto_object_name: &str) -> &[AirAvoidanceActionProfile] {
        self.air_avoidance_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }
}

pub(super) fn collect_air_avoidance_actions(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<AirAvoidanceActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let profiles = rules
                .persistent_actions
                .iter()
                .filter_map(|name| {
                    gameplay.tactics.actions.iter().find(|action| {
                        action.name.eq_ignore_ascii_case(name) && is_air_avoidance(action)
                    })
                })
                .map(|action| profile_from_action(action, &gameplay.tactics.weapons))
                .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(action: &Action, weapons: &[Weapon]) -> AirAvoidanceActionProfile {
    let kamikaze_weapon = action.weapon.as_deref().and_then(|name| {
        weapons
            .iter()
            .find(|weapon| weapon.name.eq_ignore_ascii_case(name))
            .map(kamikaze_profile)
    });
    AirAvoidanceActionProfile {
        action_name: action.name.clone(),
        flags: AirAvoidanceProfileFlags::from_action(action),
        hover_altitude_offset: finite_or(action.hover_altitude_offset, 0.0),
        max_target_depression_angle: finite_or(
            action.max_target_depression_angle,
            DEFAULT_MAX_TARGET_DEPRESSION_ANGLE,
        )
        .clamp(0.0, 180.0),
        kamikaze_weapon,
    }
}

fn kamikaze_profile(weapon: &Weapon) -> KamikazeWeaponProfile {
    KamikazeWeaponProfile {
        damage: finite_nonnegative(weapon.damage_per_second),
        max_range: finite_nonnegative(weapon.max_range),
        weapon_type: trimmed(weapon.weapon_type.as_deref()),
        area_damage: area_damage_profile(weapon),
        impact_effect: ImpactEffectProfile::from_weapon(weapon),
    }
}

fn area_damage_profile(weapon: &Weapon) -> Option<AreaDamageProfile> {
    let radius = finite_nonnegative(weapon.aoe_radius);
    (radius > 0.0).then(|| AreaDamageProfile {
        radius,
        primary_target_factor: finite_or(weapon.aoe_primary_target_factor, 0.0),
        distance_factor: finite_or(weapon.aoe_distance_factor, 0.0),
        damage_factor: finite_or(weapon.aoe_damage_factor, 0.0),
        linear_damage: weapon.aoe_linear_damage == Some(true),
        ignores_y_axis: weapon.aoe_ignores_y_axis == Some(true),
        friendly_fire: weapon.allow_friendly_fire == Some(true),
    })
}

fn is_air_avoidance(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .or(action.persistent_action_type.as_deref())
        .is_some_and(|kind| kind.eq_ignore_ascii_case("AvoidCollisionAir"))
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    finite_or(value, 0.0).max(0.0)
}

fn finite_or(value: Option<f32>, fallback: f32) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or(fallback)
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::tactics::{ImpactEffect, TacticData, TacticRules};
    use pipeline::database::hw1::{Database, ProtoObject};

    #[test]
    fn compiles_authored_order_and_kamikaze_weapon() {
        let actions = vec![
            Action {
                name: "AvoidCollisionAir".to_owned(),
                action_type: Some("AvoidCollisionAir".to_owned()),
                max_target_depression_angle: Some(60.0),
                ..Action::default()
            },
            Action {
                name: "KamikazeOnDeath".to_owned(),
                action_type: Some("AvoidCollisionAir".to_owned()),
                start_disabled: Some(true),
                weapon: Some("KamikazeDive".to_owned()),
                ..Action::default()
            },
        ];
        let weapon = Weapon {
            name: "KamikazeDive".to_owned(),
            damage_per_second: Some(1500.0),
            max_range: Some(65.0),
            weapon_type: Some(" Basic ".to_owned()),
            aoe_radius: Some(6.0),
            aoe_primary_target_factor: Some(0.5),
            impact_effect: Some(ImpactEffect {
                name: "Tankshell".to_owned(),
                size: Some("Medium".to_owned()),
                ..ImpactEffect::default()
            }),
            ..Weapon::default()
        };
        let database = Database {
            objects: vec![ProtoObject {
                name: "banshee".to_owned(),
                tactics: Some("banshee.tactics".to_owned()),
                ..ProtoObject::default()
            }],
            ..Database::default()
        };
        let catalog = GameplayCatalog::from_tactics(
            &database,
            [(
                "banshee".to_owned(),
                TacticData {
                    actions,
                    weapons: vec![weapon],
                    tactic: Some(TacticRules {
                        persistent_actions: vec![
                            "AvoidCollisionAir".to_owned(),
                            "KamikazeOnDeath".to_owned(),
                        ],
                        ..TacticRules::default()
                    }),
                    ..TacticData::default()
                },
            )],
        );

        let profiles = catalog.air_avoidance_actions("BANSHEE");
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].action_name(), "AvoidCollisionAir");
        assert!((profiles[0].max_target_depression_angle() - 60.0).abs() < f32::EPSILON);
        assert!(profiles[1].starts_disabled());
        let weapon = profiles[1].kamikaze_weapon().expect("compiled weapon");
        assert!((weapon.damage() - 1500.0).abs() < f32::EPSILON);
        assert!((weapon.max_range() - 65.0).abs() < f32::EPSILON);
        assert_eq!(weapon.weapon_type(), Some("Basic"));
        assert!((weapon.area_damage().unwrap().radius - 6.0).abs() < f32::EPSILON);
        assert_eq!(weapon.impact_effect().unwrap().name, "Tankshell");
    }
}
