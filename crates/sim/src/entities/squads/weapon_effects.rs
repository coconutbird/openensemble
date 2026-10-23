//! Squad state of the weapon effect families: the daze action (`BSquadActionDaze`), the stasis
//! counters (`mNumStasisEffects`, `mStasisSpeedMult`) and the flags that they set.

use super::Squad;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// `BSquadActionDaze`. `BSquadActionDaze_init` (`0x1403BA2B0`) sets 10000 ms and modifier 1;
/// `BSquad_addDaze` (`0x1403A4260`) then sets the hit values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SquadDaze {
    /// `cStateWorking`. The first update dazes the squad.
    pub working: bool,
    /// `mDazedDuration`: the remaining daze in milliseconds.
    pub remaining_ms: u32,
    /// `mMovementModifier`.
    pub movement_modifier: f32,
    /// `mSmartTarget`.
    pub smart_target: bool,
    /// `mFXAttachment`: the effect object of each child unit.
    pub effects: Vec<Option<EntityId>>,
}

/// The weapon effect state of one squad.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SquadWeaponEffects {
    /// The daze action, if the squad is dazed (`BSquad_isDazed`, `0x1403A4450`).
    pub daze: Option<SquadDaze>,
    /// `mNumStasisEffects` (squad offset `0x290`).
    pub stasis_effects: i8,
    /// `mStasisSpeedMult` (squad offset `0x28C`).
    pub stasis_speed_multiplier: f32,
    /// `mFlagAttackBlocked`: set by a daze and by a full stasis.
    pub attack_blocked: bool,
    /// `mFlagDazeImmobilized`.
    pub daze_immobilized: bool,
    /// `mSmartTargetValues` (squad offset `0x2B0`): stasis, daze.
    pub smart_targets: [u8; 2],
    /// The self heal of stasis beam attacks that waits for the next weapon effect update.
    pub pending_self_heal: f32,
}

impl Default for SquadWeaponEffects {
    fn default() -> Self {
        Self {
            daze: None,
            stasis_effects: 0,
            stasis_speed_multiplier: 1.0,
            attack_blocked: false,
            daze_immobilized: false,
            smart_targets: [0; 2],
            pending_self_heal: 0.0,
        }
    }
}

impl SquadWeaponEffects {
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        if let Some(daze) = &self.daze {
            checksum.hash_u32(1);
            checksum.hash_u32(u32::from(daze.working));
            checksum.hash_u32(daze.remaining_ms);
            checksum.hash_f32(daze.movement_modifier);
            checksum.hash_u32(u32::from(daze.smart_target));
            checksum.hash_u32(u32::try_from(daze.effects.len()).unwrap_or(u32::MAX));
            for effect in &daze.effects {
                checksum.hash_u32(effect.map_or(u32::MAX, EntityId::as_u32));
            }
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_i32(i32::from(self.stasis_effects));
        checksum.hash_f32(self.stasis_speed_multiplier);
        checksum.hash_u32(u32::from(self.attack_blocked));
        checksum.hash_u32(u32::from(self.daze_immobilized));
        checksum.hash_bytes(&self.smart_targets);
        checksum.hash_f32(self.pending_self_heal);
    }
}

impl Squad {
    /// `BSquad::getStasisSpeedMult`: 1 without stasis, 0 under a full stasis.
    #[must_use]
    pub const fn stasis_speed_multiplier(&self) -> f32 {
        self.weapon_effects.stasis_speed_multiplier
    }

    /// `BSquad::getNumStasisEffects`: the stasis fields on the squad.
    #[must_use]
    pub const fn stasis_effect_count(&self) -> i8 {
        self.weapon_effects.stasis_effects
    }

    /// `BSquad::isDazed`.
    #[must_use]
    pub const fn is_dazed(&self) -> bool {
        self.weapon_effects.daze.is_some()
    }

    /// The remaining daze in milliseconds, or `None` without a daze.
    #[must_use]
    pub fn daze_remaining_ms(&self) -> Option<u32> {
        self.weapon_effects
            .daze
            .as_ref()
            .map(|daze| daze.remaining_ms)
    }

    /// `mFlagAttackBlocked`: a daze or a full stasis stops the attacks of the squad.
    #[must_use]
    pub const fn is_attack_blocked(&self) -> bool {
        self.weapon_effects.attack_blocked
    }

    /// `BSquad::canMove` (2008 `squad.cpp:3943-3946`): a full stasis or a daze with a zero
    /// movement modifier holds the squad.
    #[must_use]
    pub fn is_held_by_weapon_effect(&self) -> bool {
        self.weapon_effects.stasis_speed_multiplier == 0.0 || self.weapon_effects.daze_immobilized
    }
}
