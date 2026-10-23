//! The weapon effect families of a tactic weapon: damage over time, daze and stasis.
//!
//! `BWeapon_loadWeapon` (`0x1403FFE52`) reads `DOTrate`, `DOTduration`, `DOTEffect`, `Stasis`,
//! `StasisDrain`, `StasisBomb`, `StasisHealToDrainRatio` and `Daze` (2008 `tactic.cpp:301-327`
//! and `640-695`).

use super::{AreaDamageProfile, GameplayCatalog};
use pipeline::database::hw1::Database;
use pipeline::database::hw1::tactics::{Action, Weapon};
use std::collections::BTreeMap;

/// The database values that the weapon effects read at run time.
#[derive(Clone, Debug, Default)]
pub(super) struct WeaponEffectCatalog {
    dot_effect_sizes: DotEffectSizes,
    /// `NumStasisFieldsToStop` by lowercase proto-object name, for the objects that set it.
    stasis_fields_to_stop: BTreeMap<String, i32>,
    /// The code proto objects `Stun` and `EMP` (`BDatabase::mOTIDStun`, `mOTIDEmp`).
    daze_effects: [Option<String>; 2],
}

impl WeaponEffectCatalog {
    pub(super) fn from_database(database: &Database) -> Self {
        Self {
            dot_effect_sizes: DotEffectSizes::from_database(database),
            stasis_fields_to_stop: database
                .objects
                .iter()
                .filter_map(|object| {
                    object
                        .num_stasis_fields_to_stop
                        .map(|count| (object.name.to_ascii_lowercase(), count))
                })
                .collect(),
            daze_effects: [
                super::code_objects::named_proto(database, "Stun"),
                super::code_objects::named_proto(database, "EMP"),
            ],
        }
    }
}

/// The area damage of a stasis bomb: the linked action of a `StasisBomb` weapon.
#[derive(Clone, Debug, PartialEq)]
pub struct StasisBombProfile {
    /// The `DamagePerSecond` of the linked weapon (`doAreaEffectDamage` uses it as damage).
    pub damage: f32,
    /// The weapon type of the linked weapon.
    pub weapon_type: Option<String>,
    /// The area of the linked weapon.
    pub area_damage: AreaDamageProfile,
}

impl GameplayCatalog {
    /// The obstruction-radius limits of the `DOTEffect` sizes.
    #[must_use]
    pub const fn dot_effect_sizes(&self) -> DotEffectSizes {
        self.weapon_effects.dot_effect_sizes
    }

    /// `BProtoObject::getNumStasisFieldsToStop`, raised to 1 as `BUnitActionStasis` does.
    #[must_use]
    pub fn stasis_fields_to_stop(&self, proto_object_name: &str) -> i32 {
        self.weapon_effects
            .stasis_fields_to_stop
            .get(&proto_object_name.to_ascii_lowercase())
            .copied()
            .unwrap_or(1)
            .max(1)
    }

    /// The daze effect object of a squad: `Stun` for an infantry squad, else `EMP`.
    #[must_use]
    pub fn daze_effect(&self, infantry: bool) -> Option<&str> {
        self.weapon_effects.daze_effects[usize::from(!infantry)].as_deref()
    }

    /// The stasis bomb of an attack action of a proto object (its `LinkedAction`).
    #[must_use]
    pub fn stasis_bomb(&self, proto_object_name: &str, action_name: &str) -> Option<StasisBombProfile> {
        let ranged = self
            .object(proto_object_name)?
            .ranged_actions()
            .find(|ranged| ranged.action.name.eq_ignore_ascii_case(action_name))?;
        Some(StasisBombProfile {
            damage: ranged
                .weapon
                .damage_per_second
                .filter(|damage| damage.is_finite())
                .unwrap_or(0.0),
            weapon_type: ranged.weapon.weapon_type.clone(),
            area_damage: super::timing::area_damage_profile(ranged.weapon)?,
        })
    }
}

/// The smart-target counter that a weapon uses (`BWeaponStatic::mSmartTargetType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SmartTargetType {
    /// `BSquad::cSmartTargetStasis`.
    Stasis = 0,
    /// `BSquad::cSmartTargetDaze`.
    Daze = 1,
}

/// The damage over time of a weapon with a positive `DOTrate`.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageOverTimeProfile {
    /// `DOTrate`: the damage per second of one stack.
    pub rate: f32,
    /// `DOTduration`: the life of one stack in seconds.
    pub duration: f32,
    /// `DOTEffect`: the effect objects by target size.
    pub effect: DotEffectProfile,
}

/// The `DOTEffect` object names. The element text sets all three sizes, and the `small`,
/// `medium` and `large` attributes replace one size each.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DotEffectProfile {
    /// The effect of a target with an obstruction radius up to the small size.
    pub small: Option<String>,
    /// The effect of a target with an obstruction radius up to the medium size.
    pub medium: Option<String>,
    /// The effect of a larger target.
    pub large: Option<String>,
}

impl DotEffectProfile {
    /// `BProtoAction::getDOTEffect`: the effect for a target obstruction radius.
    #[must_use]
    pub fn for_radius(&self, radius: f32, sizes: DotEffectSizes) -> Option<&str> {
        let effect = if radius <= sizes.small {
            &self.small
        } else if radius <= sizes.medium {
            &self.medium
        } else {
            &self.large
        };
        effect.as_deref()
    }
}

/// The obstruction-radius limits of the `DOTEffect` sizes (game data `Dot`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DotEffectSizes {
    /// `Dot@small`.
    pub small: f32,
    /// `Dot@medium`.
    pub medium: f32,
}

impl DotEffectSizes {
    pub(super) fn from_database(database: &Database) -> Self {
        let dot = database
            .game_data
            .as_ref()
            .and_then(|data| data.dot.as_ref());
        let size = |value: Option<f32>| value.filter(|value| value.is_finite()).unwrap_or(0.0);
        Self {
            small: size(dot.and_then(|dot| dot.small)),
            medium: size(dot.and_then(|dot| dot.medium)),
        }
    }
}

/// The daze of a weapon with a `Daze` element.
#[derive(Clone, Debug, PartialEq)]
pub struct DazeProfile {
    /// The element text times 1000: the daze duration in milliseconds.
    pub duration_ms: f32,
    /// `TargetType`: the object type that a hit dazes. `None` never matches.
    pub target_type: Option<String>,
    /// `MovementModifier` (default 0). A value under `1e-6` immobilizes the squad.
    pub movement_modifier: f32,
    /// `AOEDaze` (default true). Without it, a projectile dazes only one squad.
    pub aoe: bool,
}

/// The stasis flags of a weapon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StasisProfile {
    /// `Stasis` with `SmartTargeting="true"`. Without that attribute, retail sets no flag.
    pub stasis: bool,
    /// `StasisDrain`.
    pub drain: bool,
    /// `StasisBomb`.
    pub bomb: bool,
    /// `StasisHealToDrainRatio`: the self heal of a stasis beam attack per damage per second.
    pub heal_to_drain_ratio: f32,
    /// The `LinkedAction` of the attack action: the area damage of a stasis bomb.
    pub linked_action: Option<String>,
}

impl StasisProfile {
    /// Return whether a fired attack applies the stasis effect
    /// (`BUnitActionRangedAttack_fireWeapon` `0x140556850`).
    #[must_use]
    pub const fn applies(&self) -> bool {
        self.stasis || self.drain || self.bomb
    }
}

/// The effect families of one tactic weapon.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponEffectProfile {
    /// The damage over time, or `None` for a weapon without a positive `DOTrate`.
    pub damage_over_time: Option<DamageOverTimeProfile>,
    /// The daze, or `None` for a weapon without `Daze`.
    pub daze: Option<DazeProfile>,
    /// The stasis flags.
    pub stasis: StasisProfile,
    /// The smart-target counter of the weapon.
    pub smart_target: Option<SmartTargetType>,
    /// The authored `DamagePerSecond`, the base of the stasis self heal.
    pub damage_per_second: f32,
}

impl WeaponEffectProfile {
    /// The effect families of a weapon, or `None` if the weapon has none.
    #[must_use]
    pub fn from_action(action: &Action, weapon: &Weapon) -> Option<Self> {
        let finite = |value: Option<f32>| value.filter(|value| value.is_finite()).unwrap_or(0.0);
        let damage_over_time = (finite(weapon.dot_rate) > 0.0).then(|| DamageOverTimeProfile {
            rate: finite(weapon.dot_rate),
            duration: finite(weapon.dot_duration),
            effect: weapon.dot_effect.as_ref().map_or_else(
                DotEffectProfile::default,
                |effect| {
                    let text = non_empty(Some(effect.default_effect.as_str()));
                    let size = |value: Option<&String>| {
                        value.map_or_else(|| text.clone(), |name| non_empty(Some(name)))
                    };
                    DotEffectProfile {
                        small: size(effect.small.as_ref()),
                        medium: size(effect.medium.as_ref()),
                        large: size(effect.large.as_ref()),
                    }
                },
            ),
        });
        let stasis_smart = weapon
            .stasis
            .as_ref()
            .is_some_and(|stasis| stasis.smart_targeting == Some(true));
        let daze = weapon.daze.as_ref().map(|daze| DazeProfile {
            duration_ms: finite(Some(daze.duration)) * 1_000.0,
            target_type: non_empty(daze.target_type.as_deref()),
            movement_modifier: finite(daze.movement_modifier),
            aoe: daze.aoe_daze != Some(false),
        });
        let daze_smart = weapon
            .daze
            .as_ref()
            .is_some_and(|daze| daze.smart_targeting == Some(true));
        let stasis = StasisProfile {
            stasis: stasis_smart,
            drain: weapon.stasis_drain == Some(true),
            bomb: weapon.stasis_bomb == Some(true),
            heal_to_drain_ratio: finite(weapon.stasis_heal_to_drain_ratio),
            linked_action: non_empty(action.linked_action.as_deref()),
        };
        // The later of `Stasis` and `Daze` wins the smart-target type; shipped weapons use one.
        let smart_target = if daze_smart {
            Some(SmartTargetType::Daze)
        } else {
            stasis_smart.then_some(SmartTargetType::Stasis)
        };
        (damage_over_time.is_some() || daze.is_some() || stasis.applies() || smart_target.is_some())
            .then(|| Self {
                damage_over_time,
                daze,
                stasis,
                smart_target,
                damage_per_second: finite(weapon.damage_per_second),
            })
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
