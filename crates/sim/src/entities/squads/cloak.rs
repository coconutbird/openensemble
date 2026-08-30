//! Persistent squad cloak state owned by the deterministic simulation.

use super::Squad;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CloakModifiers {
    pub ability_id: Option<u8>,
    pub damage_taken: f32,
    pub dodge: f32,
}

impl Default for CloakModifiers {
    fn default() -> Self {
        Self {
            ability_id: None,
            damage_taken: 0.0,
            dodge: 0.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SquadCloak {
    phase: CloakPhase,
    detected: bool,
    permanent: bool,
    activation_delay: f32,
    activation_remaining: f32,
    recloak_delay: f32,
    detection_remaining: f32,
    duration: f32,
    duration_remaining: f32,
    requested_ability_id: Option<u8>,
    modifiers: CloakModifiers,
    modified_unit_ids: Vec<EntityId>,
    effect_attachments: Vec<(EntityId, EntityId)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CloakPhase {
    #[default]
    Uninitialized,
    UninitializedRequested,
    Ready,
    Requested,
    Cloaked,
}

impl SquadCloak {
    pub(crate) const fn is_initialized(&self) -> bool {
        !matches!(self.phase, CloakPhase::Uninitialized)
    }

    pub(crate) const fn is_cloaked(&self) -> bool {
        matches!(self.phase, CloakPhase::Cloaked)
    }

    pub(crate) const fn is_detected(&self) -> bool {
        self.detected
    }

    pub(crate) const fn wants_to_cloak(&self) -> bool {
        matches!(
            self.phase,
            CloakPhase::UninitializedRequested | CloakPhase::Requested
        )
    }

    pub(crate) const fn is_permanent(&self) -> bool {
        self.permanent
    }

    pub(crate) const fn duration_remaining(&self) -> f32 {
        self.duration_remaining
    }

    pub(crate) const fn modifiers(&self) -> CloakModifiers {
        self.modifiers
    }

    pub(crate) fn modified_unit_ids(&self) -> &[EntityId] {
        &self.modified_unit_ids
    }

    pub(crate) fn effect_attachments(&self) -> &[(EntityId, EntityId)] {
        &self.effect_attachments
    }

    pub(crate) fn initialize(
        &mut self,
        permanent: bool,
        activation_delay: f32,
        recloak_delay: f32,
    ) {
        self.phase = match self.phase {
            CloakPhase::Uninitialized => CloakPhase::Ready,
            CloakPhase::UninitializedRequested => CloakPhase::Requested,
            _ => return,
        };
        self.permanent = permanent;
        self.activation_delay = nonnegative(activation_delay);
        self.activation_remaining = self.activation_delay;
        self.recloak_delay = nonnegative(recloak_delay);
        self.detection_remaining = self.recloak_delay;
    }

    pub(crate) fn request(&mut self, ability_id: Option<u8>) -> bool {
        self.phase = match self.phase {
            CloakPhase::Cloaked => return false,
            CloakPhase::Uninitialized | CloakPhase::UninitializedRequested => {
                CloakPhase::UninitializedRequested
            }
            CloakPhase::Ready | CloakPhase::Requested => CloakPhase::Requested,
        };
        self.requested_ability_id = ability_id;
        true
    }

    pub(crate) fn activation_ready(&mut self, dt: f32) -> bool {
        if !self.wants_to_cloak() || !valid_elapsed(dt) {
            return false;
        }
        advance_countdown(&mut self.activation_remaining, dt, self.activation_delay)
    }

    pub(crate) fn activate(
        &mut self,
        duration: f32,
        modifiers: CloakModifiers,
        modified_unit_ids: Vec<EntityId>,
        effect_attachments: Vec<(EntityId, EntityId)>,
    ) {
        self.phase = CloakPhase::Cloaked;
        self.duration = if self.permanent {
            0.0
        } else {
            nonnegative(duration)
        };
        self.duration_remaining = self.duration;
        self.modifiers = modifiers;
        self.modified_unit_ids = modified_unit_ids;
        self.effect_attachments = effect_attachments;
    }

    pub(crate) fn duration_expired(&mut self, dt: f32) -> bool {
        if !self.is_cloaked() || self.permanent || !valid_elapsed(dt) {
            return false;
        }
        advance_countdown(&mut self.duration_remaining, dt, self.duration)
    }

    pub(crate) fn detect(&mut self) {
        self.detected = true;
        self.detection_remaining = self.recloak_delay;
    }

    pub(crate) fn advance_detection(&mut self, dt: f32) {
        if !self.detected || !valid_elapsed(dt) {
            return;
        }
        if advance_countdown(&mut self.detection_remaining, dt, self.recloak_delay) {
            self.detected = false;
        }
    }

    pub(crate) fn finish(&mut self) {
        self.phase = if self.is_initialized() {
            CloakPhase::Ready
        } else {
            CloakPhase::Uninitialized
        };
        self.detected = false;
        self.activation_remaining = self.activation_delay;
        self.detection_remaining = self.recloak_delay;
        self.duration = 0.0;
        self.duration_remaining = 0.0;
        self.requested_ability_id = None;
        self.modifiers = CloakModifiers::default();
        self.modified_unit_ids.clear();
        self.effect_attachments.clear();
    }

    pub(crate) fn disconnect(&mut self) {
        self.finish();
        self.phase = CloakPhase::Uninitialized;
        self.permanent = false;
        self.activation_delay = 0.0;
        self.activation_remaining = 0.0;
        self.recloak_delay = 0.0;
        self.detection_remaining = 0.0;
        self.duration = 0.0;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.is_initialized()));
        checksum.hash_u32(u32::from(self.wants_to_cloak()));
        checksum.hash_u32(u32::from(self.is_cloaked()));
        checksum.hash_u32(u32::from(self.detected));
        checksum.hash_u32(u32::from(self.permanent));
        checksum.hash_f32(self.activation_delay);
        checksum.hash_f32(self.activation_remaining);
        checksum.hash_f32(self.recloak_delay);
        checksum.hash_f32(self.detection_remaining);
        checksum.hash_f32(self.duration);
        checksum.hash_f32(self.duration_remaining);
        checksum.hash_u32(self.requested_ability_id.map_or(u32::MAX, u32::from));
        checksum.hash_u32(self.modifiers.ability_id.map_or(u32::MAX, u32::from));
        checksum.hash_f32(self.modifiers.damage_taken);
        checksum.hash_f32(self.modifiers.dodge);
        hash_ids(checksum, &self.modified_unit_ids);
        checksum.hash_u32(u32::try_from(self.effect_attachments.len()).unwrap_or(u32::MAX));
        for (unit_id, effect_id) in &self.effect_attachments {
            checksum.hash_u32(unit_id.as_u32());
            checksum.hash_u32(effect_id.as_u32());
        }
    }
}

impl Squad {
    /// Whether the squad currently has retail cloak gameplay active.
    #[must_use]
    pub const fn is_cloaked(&self) -> bool {
        self.cloak.is_cloaked()
    }

    /// Whether enemies may currently see and target this cloaked squad.
    #[must_use]
    pub const fn is_cloak_detected(&self) -> bool {
        self.cloak.is_detected()
    }

    /// Whether a manual cloak request is waiting for its activation delay.
    #[must_use]
    pub const fn wants_to_cloak(&self) -> bool {
        self.cloak.wants_to_cloak()
    }

    /// Whether the active profile uses retail's permanent-cloak path.
    #[must_use]
    pub const fn is_permanently_cloaked(&self) -> bool {
        self.cloak.is_cloaked() && self.cloak.is_permanent()
    }

    /// Seconds remaining on a non-permanent cloak ability.
    #[must_use]
    pub const fn cloak_duration_remaining(&self) -> f32 {
        self.cloak.duration_remaining()
    }
}

fn valid_elapsed(dt: f32) -> bool {
    dt.is_finite() && dt > 0.0
}

fn nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn advance_countdown(remaining: &mut f32, dt: f32, authored: f32) -> bool {
    if *remaining <= dt {
        *remaining = 0.0;
        return true;
    }
    *remaining -= dt;
    let tolerance = f32::EPSILON * authored.max(1.0) * 256.0;
    if *remaining <= tolerance {
        *remaining = 0.0;
        true
    } else {
        false
    }
}

fn hash_ids(checksum: &mut SyncChecksum, ids: &[EntityId]) {
    checksum.hash_u32(u32::try_from(ids.len()).unwrap_or(u32::MAX));
    for id in ids {
        checksum.hash_u32(id.as_u32());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_detection_and_duration_are_distinct_clocks() {
        let mut cloak = SquadCloak::default();
        cloak.initialize(false, 1.0, 5.0);
        assert!(cloak.request(Some(3)));
        assert!(!cloak.activation_ready(0.75));
        assert!(cloak.activation_ready(0.25));
        cloak.activate(2.0, CloakModifiers::default(), Vec::new(), Vec::new());
        cloak.detect();

        assert!(!cloak.duration_expired(1.0));
        cloak.advance_detection(1.0);
        assert!(cloak.is_cloaked());
        assert!(cloak.is_detected());
        assert!(cloak.duration_expired(1.0));
        cloak.advance_detection(4.0);
        assert!(!cloak.is_detected());
    }
}
