//! Impact-effect identity retained across projectile flight.

use super::Projectile;
use crate::gameplay::ImpactEffectProfile;
use crate::sync::SyncChecksum;

impl Projectile {
    /// Return the tactic impact effect that this projectile will present.
    #[must_use]
    pub fn impact_effect(&self) -> Option<&ImpactEffectProfile> {
        self.impact_effect.as_ref()
    }

    pub(super) fn hash_impact_effect(&self, checksum: &mut SyncChecksum) {
        let Some(effect) = &self.impact_effect else {
            checksum.hash_u32(0);
            return;
        };
        checksum.hash_u32(1);
        checksum.hash_u32(u32::try_from(effect.name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(effect.name.as_bytes());
        checksum.hash_u32(effect.size.checksum_value());
        checksum.hash_u32(u32::from(effect.do_shockwave_action));
    }
}
