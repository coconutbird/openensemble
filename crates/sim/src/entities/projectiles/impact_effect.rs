//! Impact-effect identity and weapon throw values retained across projectile flight.

use super::Projectile;
use crate::gameplay::{DeathThrowProfile, ImpactEffectProfile};
use crate::sync::SyncChecksum;

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

    pub(super) fn hash_impact_effect(&self, checksum: &mut SyncChecksum) {
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
