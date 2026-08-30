//! Runtime state for persistent energy-shield presentation actions.

use super::UnitShields;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// Broad presentation mechanism used by a persistent shield action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EnergyShieldPresentationKind {
    /// A separate class-zero visual attachment.
    Attachment = 0,
    /// A named component inside the unit visual.
    InfantryComponent = 1,
}

/// Authoritative lifecycle phase projected by presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EnergyShieldPhase {
    /// No external shield visual or active infantry hit timer.
    Down = 0,
    /// External shield attachment is raised and idle.
    Up = 1,
    /// A shield-hit animation or infantry hit timer is active.
    Hit = 2,
    /// External shield death animation is completing before removal.
    Lowering = 3,
}

/// One unit's live persistent shield-action state.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitEnergyShieldAction {
    pub(crate) action_name: String,
    pub(crate) kind: EnergyShieldPresentationKind,
    pub(crate) phase: EnergyShieldPhase,
    pub(crate) attachment_entity_id: Option<EntityId>,
    pub(crate) transition_seconds_remaining: Option<f32>,
    pub(crate) prototype_name: Option<String>,
    pub(crate) prototype_id: Option<i32>,
    pub(crate) bone_name: Option<String>,
    pub(crate) component_name: Option<String>,
    pub(crate) hit_duration_ms: u32,
}

impl UnitEnergyShieldAction {
    /// Authored persistent action name.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Presentation mechanism configured by the action type.
    #[must_use]
    pub const fn kind(&self) -> EnergyShieldPresentationKind {
        self.kind
    }

    /// Current authoritative shield visual phase.
    #[must_use]
    pub const fn phase(&self) -> EnergyShieldPhase {
        self.phase
    }

    /// Class-zero shield attachment, when the external visual is raised.
    #[must_use]
    pub const fn attachment_entity_id(&self) -> Option<EntityId> {
        self.attachment_entity_id
    }

    /// Remaining time for the current hit or lowering phase.
    #[must_use]
    pub const fn transition_seconds_remaining(&self) -> Option<f32> {
        self.transition_seconds_remaining
    }

    /// External shield prototype name, when this is an attachment action.
    #[must_use]
    pub fn prototype_name(&self) -> Option<&str> {
        self.prototype_name.as_deref()
    }

    /// Bone authored for the external attachment.
    #[must_use]
    pub fn bone_name(&self) -> Option<&str> {
        self.bone_name.as_deref()
    }

    /// Named unit-visual component owned by the infantry action.
    #[must_use]
    pub fn component_name(&self) -> Option<&str> {
        self.component_name.as_deref()
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_text(checksum, &self.action_name);
        checksum.hash_u32(self.kind as u32);
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.attachment_entity_id.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_f32(self.transition_seconds_remaining.unwrap_or(-1.0));
        hash_optional_text(checksum, self.prototype_name.as_deref());
        checksum.hash_i32(self.prototype_id.unwrap_or(i32::MIN));
        hash_optional_text(checksum, self.bone_name.as_deref());
        hash_optional_text(checksum, self.component_name.as_deref());
        checksum.hash_u32(self.hit_duration_ms);
    }
}

impl UnitShields {
    /// Live persistent energy-shield actions owned by this unit.
    #[must_use]
    pub fn energy_shield_actions(&self) -> &[UnitEnergyShieldAction] {
        &self.visual_actions
    }

    pub(crate) fn take_energy_shield_actions(&mut self) -> Vec<UnitEnergyShieldAction> {
        std::mem::take(&mut self.visual_actions)
    }

    pub(crate) fn replace_energy_shield_actions(&mut self, actions: Vec<UnitEnergyShieldAction>) {
        self.visual_actions = actions;
    }

    pub(crate) fn notify_energy_shield_actions_damaged(&mut self, shieldpoints: f32, alive: bool) {
        for action in &mut self.visual_actions {
            match action.kind {
                EnergyShieldPresentationKind::Attachment
                    if action.phase != EnergyShieldPhase::Down =>
                {
                    action.phase = if alive && shieldpoints > 0.0 {
                        EnergyShieldPhase::Hit
                    } else {
                        EnergyShieldPhase::Lowering
                    };
                    action.transition_seconds_remaining = None;
                }
                EnergyShieldPresentationKind::InfantryComponent if shieldpoints > 0.0 => {
                    action.phase = EnergyShieldPhase::Hit;
                    action.transition_seconds_remaining = None;
                }
                _ => {}
            }
        }
    }

    pub(super) fn hash_energy_shield_actions(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.visual_actions.len()).unwrap_or(u32::MAX));
        for action in &self.visual_actions {
            action.hash_state(checksum);
        }
    }
}

fn hash_optional_text(checksum: &mut SyncChecksum, value: Option<&str>) {
    checksum.hash_u32(u32::from(value.is_some()));
    if let Some(value) = value {
        hash_text(checksum, value);
    }
}

fn hash_text(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}
