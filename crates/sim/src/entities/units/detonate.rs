//! Checksummed per-unit `BUnitActionDetonate` state.

mod bomb;

pub use bomb::BombPhase;

use super::Unit;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// Observable phase of retail's short-lived `BUnitActionDetonate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum UnitDetonatePhase {
    /// No child Detonate action exists, though a squad may have armed one.
    #[default]
    Inactive = 0,
    /// An opportunity created the action during the current update.
    Pending = 1,
    /// The action is evaluating its authored triggers.
    Working = 2,
}

/// Trigger values captured when one action instance is created.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct UnitDetonateTriggerConfig {
    pub immediate: bool,
    pub on_death: bool,
    pub countdown_ms: Option<u32>,
    pub proximity_radius: Option<f32>,
    pub physics_threshold: Option<f32>,
    pub instigator_player_id: Option<PlayerId>,
}

/// Immutable data retained after an action decides to explode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnitDetonation {
    pub action_name: String,
    pub instigator_player_id: Option<PlayerId>,
}

/// Per-unit state retained while its squad is glowing or an action is active.
#[derive(Debug, Clone)]
pub(crate) struct UnitDetonate {
    bomb: bomb::UnitBomb,
    action_name: Option<String>,
    phase: UnitDetonatePhase,
    velocity_scalar: f32,
    immediate_trigger: bool,
    death_trigger: bool,
    countdown_remaining_ms: Option<u32>,
    proximity_radius: Option<f32>,
    physics_trigger_threshold: Option<f32>,
    physics_trigger_activated: bool,
    detonation_instigator: Option<PlayerId>,
}

impl Default for UnitDetonate {
    fn default() -> Self {
        Self {
            bomb: bomb::UnitBomb::default(),
            action_name: None,
            phase: UnitDetonatePhase::Inactive,
            velocity_scalar: 1.0,
            immediate_trigger: false,
            death_trigger: false,
            countdown_remaining_ms: None,
            proximity_radius: None,
            physics_trigger_threshold: None,
            physics_trigger_activated: false,
            detonation_instigator: None,
        }
    }
}

impl UnitDetonate {
    fn arm(&mut self, action_name: &str, velocity_scalar: f32) {
        if self.phase != UnitDetonatePhase::Inactive {
            return;
        }
        self.clear_triggers();
        self.action_name = Some(action_name.to_owned());
        self.velocity_scalar = if velocity_scalar.is_finite() && velocity_scalar >= 0.0 {
            velocity_scalar
        } else {
            1.0
        };
    }

    fn begin(&mut self, config: UnitDetonateTriggerConfig) -> bool {
        if self.action_name.is_none() || self.phase != UnitDetonatePhase::Inactive {
            return false;
        }
        self.phase = UnitDetonatePhase::Pending;
        self.immediate_trigger = config.immediate;
        self.death_trigger = config.on_death;
        self.countdown_remaining_ms = config.countdown_ms;
        self.proximity_radius = finite_nonnegative(config.proximity_radius);
        self.physics_trigger_threshold = finite_nonnegative(config.physics_threshold);
        self.physics_trigger_activated = false;
        self.detonation_instigator = config.instigator_player_id;
        true
    }

    fn advance(
        &mut self,
        elapsed_ms: u32,
        is_dead: bool,
        linear_speed: f32,
        enemy_in_proximity: bool,
    ) -> Option<UnitDetonation> {
        match self.phase {
            UnitDetonatePhase::Inactive => None,
            UnitDetonatePhase::Pending => {
                self.phase = UnitDetonatePhase::Working;
                None
            }
            UnitDetonatePhase::Working => {
                if self.immediate_trigger || (self.death_trigger && is_dead) {
                    return self.take_detonation();
                }
                if self.countdown_finished(elapsed_ms) {
                    return self.take_detonation();
                }
                if self.countdown_remaining_ms.is_none()
                    && self.physics_trigger_activated
                    && linear_speed < 0.1
                {
                    return self.take_detonation();
                }
                enemy_in_proximity.then(|| self.take_detonation()).flatten()
            }
        }
    }

    fn countdown_finished(&mut self, elapsed_ms: u32) -> bool {
        if self.physics_trigger_threshold.is_some() && !self.physics_trigger_activated {
            return false;
        }
        let Some(remaining) = self.countdown_remaining_ms else {
            return false;
        };
        if elapsed_ms > remaining {
            return true;
        }
        self.countdown_remaining_ms = Some(remaining - elapsed_ms);
        false
    }

    fn notify_death(&mut self) -> Option<UnitDetonation> {
        (self.phase != UnitDetonatePhase::Inactive && self.death_trigger)
            .then(|| self.take_detonation())
            .flatten()
    }

    fn force(&mut self) -> Option<UnitDetonation> {
        (self.phase != UnitDetonatePhase::Inactive)
            .then(|| self.take_detonation())
            .flatten()
    }

    fn physics_collision(&mut self, projected_velocity: f32) {
        let activates = self
            .physics_trigger_threshold
            .is_some_and(|threshold| projected_velocity.abs() > threshold);
        self.physics_trigger_activated |= activates;
    }

    fn take_detonation(&mut self) -> Option<UnitDetonation> {
        let detonation = UnitDetonation {
            action_name: self.action_name.clone()?,
            instigator_player_id: self.detonation_instigator,
        };
        self.cancel();
        Some(detonation)
    }

    fn cancel(&mut self) {
        self.action_name = None;
        self.phase = UnitDetonatePhase::Inactive;
        self.velocity_scalar = 1.0;
        self.clear_triggers();
    }

    fn clear_triggers(&mut self) {
        self.immediate_trigger = false;
        self.death_trigger = false;
        self.countdown_remaining_ms = None;
        self.proximity_radius = None;
        self.physics_trigger_threshold = None;
        self.physics_trigger_activated = false;
        self.detonation_instigator = None;
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        self.bomb.hash_state(checksum);
        checksum.hash_u32(self.phase as u32);
        checksum.hash_f32(self.velocity_scalar);
        hash_optional_string(checksum, self.action_name.as_deref());
        checksum.hash_u32(u32::from(self.immediate_trigger));
        checksum.hash_u32(u32::from(self.death_trigger));
        hash_optional_u32(checksum, self.countdown_remaining_ms);
        hash_optional_f32(checksum, self.proximity_radius);
        hash_optional_f32(checksum, self.physics_trigger_threshold);
        checksum.hash_u32(u32::from(self.physics_trigger_activated));
        checksum.hash_u32(self.detonation_instigator.map_or(u32::MAX, u32::from));
    }
}

impl Unit {
    /// Return whether a squad or unit Detonate action has armed this unit.
    #[must_use]
    pub fn is_detonate_armed(&self) -> bool {
        self.detonate.action_name.is_some()
    }

    /// Return the unit Detonate action's current phase.
    #[must_use]
    pub const fn detonate_phase(&self) -> UnitDetonatePhase {
        self.detonate.phase
    }

    /// Return the selected Detonate tactic action while armed or active.
    #[must_use]
    pub fn detonate_action_name(&self) -> Option<&str> {
        self.detonate.action_name.as_deref()
    }

    /// Return the sampled timer remaining on the active action.
    #[must_use]
    pub const fn detonate_countdown_remaining_ms(&self) -> Option<u32> {
        self.detonate.countdown_remaining_ms
    }

    /// Return whether a collision has activated the action's physics trigger.
    #[must_use]
    pub const fn detonate_physics_trigger_activated(&self) -> bool {
        self.detonate.physics_trigger_activated
    }

    pub(crate) fn arm_detonate(&mut self, action_name: &str, velocity_scalar: f32) {
        self.detonate.arm(action_name, velocity_scalar);
    }

    pub(crate) fn begin_detonate_action(&mut self, config: UnitDetonateTriggerConfig) -> bool {
        self.detonate.begin(config)
    }

    pub(crate) fn advance_detonate_action(
        &mut self,
        elapsed_ms: u32,
        enemy_in_proximity: bool,
    ) -> Option<UnitDetonation> {
        let is_dead = self.hitpoints <= 0.0;
        let linear_speed = self.base.velocity.length();
        self.detonate
            .advance(elapsed_ms, is_dead, linear_speed, enemy_in_proximity)
    }

    pub(crate) fn notify_detonate_death(&mut self) -> Option<UnitDetonation> {
        self.detonate.notify_death()
    }

    pub(crate) fn force_detonate_action(&mut self) -> Option<UnitDetonation> {
        self.detonate.force()
    }

    pub(crate) fn notify_detonate_physics_collision(&mut self, projected_velocity: f32) {
        self.detonate.physics_collision(projected_velocity);
    }

    pub(crate) fn detonate_proximity_radius(&self) -> Option<f32> {
        (self.detonate.phase == UnitDetonatePhase::Working)
            .then_some(self.detonate.proximity_radius)
            .flatten()
    }

    pub(crate) fn cancel_detonate_action(&mut self) {
        self.detonate.cancel();
    }

    pub(crate) fn effective_velocity_scalar(&self) -> f32 {
        self.velocity_scalar * self.detonate.velocity_scalar * self.cryo_movement_modifier()
    }

    pub(crate) fn hash_detonate_state(&self, checksum: &mut SyncChecksum) {
        self.detonate.hash_state(checksum);
    }
}

fn finite_nonnegative(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn hash_optional_string(checksum: &mut SyncChecksum, value: Option<&str>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(value.as_bytes());
    } else {
        checksum.hash_u32(0);
    }
}

fn hash_optional_u32(checksum: &mut SyncChecksum, value: Option<u32>) {
    checksum.hash_u32(value.unwrap_or(u32::MAX));
}

fn hash_optional_f32(checksum: &mut SyncChecksum, value: Option<f32>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_f32(value);
    } else {
        checksum.hash_u32(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::{EntityClass, EntityId};

    #[test]
    fn immediate_action_uses_two_updates_and_restores_velocity() {
        let mut unit = Unit::new(EntityId::new(EntityClass::Unit, 1), 1);
        unit.velocity_scalar = 1.25;
        unit.arm_detonate("Bomb", 2.0);
        assert_eq!(
            unit.effective_velocity_scalar().to_bits(),
            2.5_f32.to_bits()
        );
        assert!(unit.begin_detonate_action(UnitDetonateTriggerConfig {
            immediate: true,
            ..UnitDetonateTriggerConfig::default()
        }));
        assert_eq!(unit.detonate_phase(), UnitDetonatePhase::Pending);
        assert_eq!(unit.advance_detonate_action(50, false), None);
        assert_eq!(unit.detonate_phase(), UnitDetonatePhase::Working);
        assert_eq!(
            unit.advance_detonate_action(50, false)
                .map(|detonation| detonation.action_name),
            Some("Bomb".to_owned())
        );
        assert_eq!(unit.detonate_phase(), UnitDetonatePhase::Inactive);
        assert_eq!(
            unit.effective_velocity_scalar().to_bits(),
            unit.velocity_scalar.to_bits()
        );
    }

    #[test]
    fn physics_gated_countdown_uses_retail_strict_comparison() {
        let mut unit = Unit::new(EntityId::new(EntityClass::Unit, 2), 1);
        unit.arm_detonate("Bomb", 1.0);
        assert!(unit.begin_detonate_action(UnitDetonateTriggerConfig {
            countdown_ms: Some(100),
            physics_threshold: Some(5.0),
            ..UnitDetonateTriggerConfig::default()
        }));
        assert_eq!(unit.advance_detonate_action(50, false), None);
        assert_eq!(unit.detonate_countdown_remaining_ms(), Some(100));
        unit.notify_detonate_physics_collision(5.0);
        assert!(!unit.detonate_physics_trigger_activated());
        unit.notify_detonate_physics_collision(-6.0);
        assert!(unit.detonate_physics_trigger_activated());
        assert_eq!(unit.advance_detonate_action(100, false), None);
        assert_eq!(unit.detonate_countdown_remaining_ms(), Some(0));
        assert!(unit.advance_detonate_action(1, false).is_some());
    }

    #[test]
    fn death_notification_can_detonate_pending_action() {
        let mut unit = Unit::new(EntityId::new(EntityClass::Unit, 3), 1);
        unit.arm_detonate("Bomb", 1.0);
        assert!(unit.begin_detonate_action(UnitDetonateTriggerConfig {
            on_death: true,
            instigator_player_id: Some(2),
            ..UnitDetonateTriggerConfig::default()
        }));
        let detonation = unit.notify_detonate_death().unwrap();
        assert_eq!(detonation.action_name, "Bomb");
        assert_eq!(detonation.instigator_player_id, Some(2));
    }
}
