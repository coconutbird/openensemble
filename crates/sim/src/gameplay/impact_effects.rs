//! Synchronized tactic identity for projectile impact effects.

use pipeline::database::hw1::tactics::Weapon;

/// TFX size bucket selected by a weapon's tactic action.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImpactEffectSize {
    /// Small authored variant.
    Small,
    /// Medium authored variant.
    Medium,
    /// Large authored variant.
    Large,
    /// Missing or unknown sizes use retail's generic TFX bucket.
    #[default]
    Generic,
}

/// Immutable impact-effect values retained by a launched projectile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactEffectProfile {
    /// Name resolved through the global `impacteffects.xml` prototype table.
    pub name: String,
    /// Requested surface-item size bucket.
    pub size: ImpactEffectSize,
    /// Whether the projectile also starts its prototype's shockwave action.
    pub do_shockwave_action: bool,
}

impl ImpactEffectProfile {
    pub(crate) fn from_weapon(weapon: &Weapon) -> Option<Self> {
        let authored = weapon.impact_effect.as_ref()?;
        let name = authored.name.trim();
        if name.is_empty() {
            return None;
        }
        Some(Self {
            name: name.to_owned(),
            size: ImpactEffectSize::from_authored(authored.size.as_deref()),
            do_shockwave_action: authored.do_shockwave_action == Some(true),
        })
    }
}

impl ImpactEffectSize {
    fn from_authored(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::Generic;
        };
        if value.eq_ignore_ascii_case("small") {
            Self::Small
        } else if value.eq_ignore_ascii_case("medium") {
            Self::Medium
        } else if value.eq_ignore_ascii_case("large") {
            Self::Large
        } else {
            Self::Generic
        }
    }

    pub(crate) const fn checksum_value(self) -> u32 {
        match self {
            Self::Small => 0,
            Self::Medium => 1,
            Self::Large => 2,
            Self::Generic => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use pipeline::database::hw1::tactics::{ImpactEffect, Weapon};

    use super::{ImpactEffectProfile, ImpactEffectSize};

    #[test]
    fn weapon_effect_preserves_sim_flag_and_uses_generic_for_unknown_size() {
        let profile = ImpactEffectProfile::from_weapon(&Weapon {
            impact_effect: Some(ImpactEffect {
                name: " Impact ".to_owned(),
                size: Some("unexpected".to_owned()),
                do_shockwave_action: Some(true),
            }),
            ..Weapon::default()
        })
        .unwrap();

        assert_eq!(profile.name, "Impact");
        assert_eq!(profile.size, ImpactEffectSize::Generic);
        assert!(profile.do_shockwave_action);
    }
}
