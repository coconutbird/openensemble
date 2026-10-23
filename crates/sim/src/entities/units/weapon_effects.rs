//! Unit state of the weapon effect families: the damage over time actions (`BUnitActionDOT`),
//! the stasis action (`BUnitActionStasis`) and the stasis hold of a ranged attack.

use crate::entity_id::EntityId;
use crate::player::{PlayerId, TeamId};
use crate::sync::SyncChecksum;

/// One stack of a damage over time action (`BUnitActionDOT::DotInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DotStack {
    /// `mSource`: the attacker unit.
    pub source_id: Option<EntityId>,
    /// The player of the attacker, for the kill credit.
    pub source_player_id: PlayerId,
    /// `mSourceTeamID`.
    pub source_team_id: TeamId,
    /// `mEndTime`: the game time in milliseconds at which the stack ends.
    pub end_time_ms: u32,
}

/// One `BUnitActionDOT` (`mFlagCanHaveMultipleOfType`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DotAction {
    /// `cStateWorking`. The first update only creates the effect and starts the work.
    pub working: bool,
    /// `mDOTEffect`: the effect object name.
    pub effect: Option<String>,
    /// `mAttachID`: the effect object attached to the unit.
    pub effect_attachment: Option<EntityId>,
    /// `mDamageMultiplier`: the weapon-type modifier of the hit that created the action.
    pub damage_multiplier: f32,
    /// `mDOTrate`.
    pub rate: f32,
    /// `mDotStacks`.
    pub stacks: Vec<DotStack>,
}

impl DotAction {
    /// `BUnitActionDOT::operator==(BDamage)`: the same effect, rate and multiplier.
    pub fn matches(&self, effect: Option<&str>, rate: f32, damage_multiplier: f32) -> bool {
        self.effect.as_deref() == effect
            && self.rate.to_bits() == rate.to_bits()
            && self.damage_multiplier.to_bits() == damage_multiplier.to_bits()
    }

    /// `BUnitActionDOT::hasSource`.
    pub fn has_source(&self, source_id: Option<EntityId>) -> bool {
        self.stacks.iter().any(|stack| stack.source_id == source_id)
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.working));
        checksum.hash_string(self.effect.as_deref().unwrap_or_default());
        hash_optional_entity(checksum, self.effect_attachment);
        checksum.hash_f32(self.damage_multiplier);
        checksum.hash_f32(self.rate);
        checksum.hash_u32(u32::try_from(self.stacks.len()).unwrap_or(u32::MAX));
        for stack in &self.stacks {
            hash_optional_entity(checksum, stack.source_id);
            checksum.hash_u32(u32::from(stack.source_player_id));
            checksum.hash_u32(u32::from(stack.source_team_id));
            checksum.hash_u32(stack.end_time_ms);
        }
    }
}

/// The stasis that a ranged attack holds on its target (`mFlagStasisApplied`,
/// `mFlagStasisDrainApplied`, `mFlagStasisBombApplied`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StasisHold {
    /// The target unit of the ranged attack.
    pub target_id: EntityId,
    /// The attack action that applied the stasis.
    pub action_name: String,
    /// `mFlagStasisDrainApplied`.
    pub drain: bool,
    /// `mFlagStasisBombApplied`.
    pub bomb: bool,
}

/// The weapon effect state of one unit.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct UnitWeaponEffects {
    /// The damage over time actions in creation order.
    pub dot_actions: Vec<DotAction>,
    /// Whether the unit has a `BUnitActionStasis`.
    pub stasis_action: bool,
    /// The stasis that the ranged attack of this unit holds on its target.
    pub stasis_hold: Option<StasisHold>,
}

impl UnitWeaponEffects {
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.dot_actions.len()).unwrap_or(u32::MAX));
        for action in &self.dot_actions {
            action.hash_state(checksum);
        }
        checksum.hash_u32(u32::from(self.stasis_action));
        if let Some(hold) = &self.stasis_hold {
            checksum.hash_u32(1);
            checksum.hash_u32(hold.target_id.as_u32());
            checksum.hash_string(&hold.action_name);
            checksum.hash_u32(u32::from(hold.drain));
            checksum.hash_u32(u32::from(hold.bomb));
        } else {
            checksum.hash_u32(0);
        }
    }
}

impl super::Unit {
    /// The damage over time stacks on the unit, over every `BUnitActionDOT`.
    #[must_use]
    pub fn damage_over_time_stack_count(&self) -> usize {
        self.weapon_effects
            .dot_actions
            .iter()
            .map(|action| action.stacks.len())
            .sum()
    }

    /// Whether the unit has a `BUnitActionStasis`.
    #[must_use]
    pub const fn has_stasis_action(&self) -> bool {
        self.weapon_effects.stasis_action
    }
}

fn hash_optional_entity(checksum: &mut SyncChecksum, value: Option<EntityId>) {
    checksum.hash_u32(value.map_or(u32::MAX, EntityId::as_u32));
}
