//! Persistent projectile Dodge and Deflect actions from layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use crate::entities::SquadMode;
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

/// Immutable values used by one persistent retail `Dodge` action.
#[derive(Debug, Clone, PartialEq)]
pub struct DodgeActionProfile {
    action_name: String,
    starts_disabled: bool,
    chance_max: f32,
    chance_min: f32,
    max_angle: f32,
    cooldown: f32,
    physics_impulse: f32,
    wait_for_deflect_cooldown: bool,
}

/// Immutable values used by one persistent retail `Deflect` action.
#[derive(Debug, Clone, PartialEq)]
pub struct DeflectActionProfile {
    action_name: String,
    chance_max: f32,
    chance_min: f32,
    max_angle: f32,
    cooldown: f32,
    max_damage: f32,
    squad_mode: Option<SquadMode>,
    flags: DeflectFlags,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DeflectFlags(u8);

impl DeflectFlags {
    const STARTS_DISABLED: Self = Self(1 << 0);
    const WAIT_FOR_DODGE_COOLDOWN: Self = Self(1 << 1);
    const SMALL_ARMS: Self = Self(1 << 2);
    const MULTI_DEFLECT: Self = Self(1 << 3);
    const SHIELD_VISUAL: Self = Self(1 << 4);

    fn from_action(action: &Action) -> Self {
        let mut flags = Self::default();
        flags.set(Self::STARTS_DISABLED, action.start_disabled == Some(true));
        flags.set(
            Self::WAIT_FOR_DODGE_COOLDOWN,
            action.wait_for_dodge_cooldown == Some(true),
        );
        flags.set(Self::SMALL_ARMS, action.small_arms == Some(true));
        flags.set(Self::MULTI_DEFLECT, action.multi_deflect == Some(true));
        flags.set(
            Self::SHIELD_VISUAL,
            action
                .proto_object
                .as_ref()
                .is_some_and(|reference| !reference.name.trim().is_empty()),
        );
        flags
    }

    const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    fn set(&mut self, flag: Self, enabled: bool) {
        if enabled {
            self.0 |= flag.0;
        }
    }
}

/// Persistent projectile reactions available to one unit prototype.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectileDefenseProfile {
    dodge: Option<DodgeActionProfile>,
    deflect: Option<DeflectActionProfile>,
}

impl DodgeActionProfile {
    /// Authored action name used by live unit and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Whether the action begins disabled before live overrides.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    /// Dodge chance for a projectile arriving directly from the front.
    #[must_use]
    pub const fn chance_max(&self) -> f32 {
        self.chance_max
    }

    /// Dodge chance at the outer authored angle.
    #[must_use]
    pub const fn chance_min(&self) -> f32 {
        self.chance_min
    }

    /// Maximum incoming angle after retail's degree-to-radian conversion.
    #[must_use]
    pub const fn max_angle(&self) -> f32 {
        self.max_angle
    }

    /// Cooldown after a successful dodge, in seconds.
    #[must_use]
    pub const fn cooldown(&self) -> f32 {
        self.cooldown
    }

    /// Optional physics impulse scalar used instead of an evade animation.
    #[must_use]
    pub const fn physics_impulse(&self) -> f32 {
        self.physics_impulse
    }

    /// Whether an active Deflect cooldown blocks this action.
    #[must_use]
    pub const fn waits_for_deflect_cooldown(&self) -> bool {
        self.wait_for_deflect_cooldown
    }
}

impl DeflectActionProfile {
    /// Authored action name used by live unit and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Whether the action begins disabled before live overrides.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.flags.contains(DeflectFlags::STARTS_DISABLED)
    }

    /// Deflect chance for a projectile arriving directly from the front.
    #[must_use]
    pub const fn chance_max(&self) -> f32 {
        self.chance_max
    }

    /// Deflect chance at the outer authored angle.
    #[must_use]
    pub const fn chance_min(&self) -> f32 {
        self.chance_min
    }

    /// Maximum incoming angle after retail's degree-to-radian conversion.
    #[must_use]
    pub const fn max_angle(&self) -> f32 {
        self.max_angle
    }

    /// Cooldown after a successful deflection, in seconds.
    #[must_use]
    pub const fn cooldown(&self) -> f32 {
        self.cooldown
    }

    /// Accumulated incoming damage that lowers the authored shield visual.
    #[must_use]
    pub const fn max_damage(&self) -> f32 {
        self.max_damage
    }

    /// Whether an active Dodge cooldown blocks this action.
    #[must_use]
    pub const fn waits_for_dodge_cooldown(&self) -> bool {
        self.flags.contains(DeflectFlags::WAIT_FOR_DODGE_COOLDOWN)
    }

    /// Whether only weapons marked `SmallArmsDeflectable` may use the action.
    #[must_use]
    pub const fn small_arms(&self) -> bool {
        self.flags.contains(DeflectFlags::SMALL_ARMS)
    }

    /// Whether the action permits simultaneous projectile deflections.
    #[must_use]
    pub const fn multi_deflect(&self) -> bool {
        self.flags.contains(DeflectFlags::MULTI_DEFLECT)
    }

    /// Optional squad mode required by the authored action.
    #[must_use]
    pub const fn squad_mode(&self) -> Option<SquadMode> {
        self.squad_mode
    }

    pub(crate) const fn has_shield_visual(&self) -> bool {
        self.flags.contains(DeflectFlags::SHIELD_VISUAL)
    }
}

impl ProjectileDefenseProfile {
    /// Persistent Dodge action, when one survived layered tactic loading.
    #[must_use]
    pub const fn dodge(&self) -> Option<&DodgeActionProfile> {
        self.dodge.as_ref()
    }

    /// Persistent Deflect action, when one survived layered tactic loading.
    #[must_use]
    pub const fn deflect(&self) -> Option<&DeflectActionProfile> {
        self.deflect.as_ref()
    }
}

impl GameplayCatalog {
    /// Resolve persistent projectile reactions for one unit prototype.
    #[must_use]
    pub fn projectile_defense(&self, proto_object_name: &str) -> Option<&ProjectileDefenseProfile> {
        self.projectile_defenses
            .get(&proto_object_name.to_ascii_lowercase())
    }
}

pub(super) fn collect_projectile_defenses(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, ProjectileDefenseProfile> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let profile = ProjectileDefenseProfile {
                dodge: persistent_action(gameplay, "Dodge").map(dodge_profile),
                deflect: persistent_action(gameplay, "Deflect").map(deflect_profile),
            };
            (profile.dodge.is_some() || profile.deflect.is_some()).then(|| (key.clone(), profile))
        })
        .collect()
}

fn dodge_profile(action: &Action) -> DodgeActionProfile {
    DodgeActionProfile {
        action_name: action.name.clone(),
        starts_disabled: action.start_disabled == Some(true),
        chance_max: finite_or_zero(action.dodge_chance_max),
        chance_min: finite_or_zero(action.dodge_chance_min),
        max_angle: authored_angle(action.dodge_max_angle),
        cooldown: finite_nonnegative(action.dodge_cooldown),
        physics_impulse: finite_nonnegative(action.dodge_physics_impulse),
        wait_for_deflect_cooldown: action.wait_for_deflect_cooldown == Some(true),
    }
}

fn deflect_profile(action: &Action) -> DeflectActionProfile {
    DeflectActionProfile {
        action_name: action.name.clone(),
        chance_max: finite_or_zero(action.deflect_chance_max),
        chance_min: finite_or_zero(action.deflect_chance_min),
        max_angle: authored_angle(action.deflect_max_angle),
        cooldown: finite_nonnegative(action.deflect_cooldown),
        max_damage: finite_nonnegative(action.deflect_max_damage),
        squad_mode: action
            .squad_mode
            .as_deref()
            .and_then(SquadMode::from_authored),
        flags: DeflectFlags::from_action(action),
    }
}

fn persistent_action<'a>(gameplay: &'a ObjectGameplay, expected: &str) -> Option<&'a Action> {
    let tactics = &gameplay.tactics;
    let named = tactics
        .tactic
        .as_ref()
        .into_iter()
        .flat_map(|rules| &rules.persistent_actions)
        .filter_map(|name| {
            tactics
                .actions
                .iter()
                .find(|action| action.name.eq_ignore_ascii_case(name))
        });
    let typed = tactics
        .actions
        .iter()
        .filter(|action| action.persistent_action_type.is_some());
    named.chain(typed).find(|action| {
        action
            .persistent_action_type
            .as_deref()
            .or(action.action_type.as_deref())
            .is_some_and(|kind| kind.eq_ignore_ascii_case(expected))
    })
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    finite_or_zero(value).max(0.0)
}

fn authored_angle(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite())
        .unwrap_or(180.0)
        .clamp(0.0, 180.0)
        .to_radians()
}

#[cfg(test)]
mod tests;
