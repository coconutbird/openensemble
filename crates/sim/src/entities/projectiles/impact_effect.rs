//! Impact-effect identity, weapon throw values and weapon effects retained across projectile
//! flight.

use super::Projectile;
use crate::gameplay::{DeathThrowProfile, ImpactEffectProfile, WeaponEffectProfile};
use crate::sync::SyncChecksum;
use std::sync::Arc;

/// The weapon effect families of the launching weapon (`mpDamageInfo`) and
/// `BProjectile::mFlagTargetAlreadyDazed` (projectile offset `0x3D6` bit 3).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ProjectileWeaponEffects {
    profile: Option<Arc<WeaponEffectProfile>>,
    target_already_dazed: bool,
}

impl Projectile {
    /// Return the tactic impact effect that this projectile will present.
    #[must_use]
    pub fn impact_effect(&self) -> Option<&ImpactEffectProfile> {
        self.impact_effect.as_ref()
    }

    /// The physics throw values of the weapon that launched the projectile.
    #[must_use]
    pub const fn death_throw(&self) -> Option<DeathThrowProfile> {
        self.death_throw
    }

    /// Set the physics throw values of the weapon that launched the projectile.
    pub(crate) const fn set_death_throw(&mut self, death_throw: Option<DeathThrowProfile>) {
        self.death_throw = death_throw;
    }

    /// The weapon effect families of the weapon that launched the projectile.
    pub(crate) fn weapon_effects(&self) -> Option<&Arc<WeaponEffectProfile>> {
        self.weapon_effects.profile.as_ref()
    }

    /// Set the weapon effect families of the weapon that launched the projectile.
    pub(crate) fn set_weapon_effects(&mut self, profile: Option<Arc<WeaponEffectProfile>>) {
        self.weapon_effects.profile = profile;
    }

    /// `mFlagTargetAlreadyDazed`: a hit of this projectile no longer dazes.
    #[must_use]
    pub const fn target_already_dazed(&self) -> bool {
        self.weapon_effects.target_already_dazed
    }

    /// Set `mFlagTargetAlreadyDazed` (`BProjectile::notify` `cEventTargetDazed`).
    pub(crate) const fn set_target_already_dazed(&mut self) {
        self.weapon_effects.target_already_dazed = true;
    }

    pub(super) fn hash_impact_effect(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.weapon_effects.target_already_dazed));
        let Some(effect) = &self.impact_effect else {
            checksum.hash_u32(0);
            return;
        };
        checksum.hash_u32(1);
        checksum.hash_string(&effect.name);
        checksum.hash_u32(effect.size.checksum_value());
        checksum.hash_u32(u32::from(effect.do_shockwave_action));
    }
}
