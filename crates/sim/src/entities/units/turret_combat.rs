//! Independent secondary- and slave-turret attack state.

use std::collections::{BTreeMap, BTreeSet};

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

use super::UnitCombat;

/// Retail executor that owns one auxiliary attack track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurretAttackKind {
    /// Persistent or opportunity-driven turret with its own target scan.
    Secondary,
    /// Turret created and targeted by a parent ranged attack.
    Slave,
}

/// Authoritative executor phase for one auxiliary turret action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TurretAttackPhase {
    #[default]
    None,
    Working,
    Blocked,
    Paused,
}

/// Runtime state for one authored auxiliary turret action.
#[derive(Debug, Clone)]
pub struct TurretAttackState {
    action_name: String,
    kind: TurretAttackKind,
    phase: TurretAttackPhase,
    target: Option<EntityId>,
    attack_wait_remaining: f32,
    pub(crate) timing: UnitCombat,
}

impl TurretAttackState {
    fn new(action_name: &str, kind: TurretAttackKind) -> Self {
        Self {
            action_name: action_name.to_owned(),
            kind,
            phase: TurretAttackPhase::None,
            target: None,
            attack_wait_remaining: 0.0,
            timing: UnitCombat::default(),
        }
    }

    /// Authored tactic action name.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Retail executor that owns this track.
    #[must_use]
    pub const fn kind(&self) -> TurretAttackKind {
        self.kind
    }

    /// Concrete unit currently tracked by this turret.
    #[must_use]
    pub const fn target(&self) -> Option<EntityId> {
        self.target
    }

    pub(crate) const fn phase(&self) -> TurretAttackPhase {
        self.phase
    }

    pub(crate) fn attack_wait_on(&self) -> bool {
        self.attack_wait_remaining > 0.0
    }

    pub(crate) const fn is_attacking(&self) -> bool {
        self.timing.is_animating()
    }

    /// Selected attachment-animation variant, when a cycle exists.
    #[must_use]
    pub const fn animation_index(&self) -> Option<usize> {
        self.timing.animation_index()
    }

    /// Project the active attachment clip's normalized position.
    #[must_use]
    pub fn animation_position(&self, duration_seconds: f32) -> Option<f32> {
        self.timing.animation_position(duration_seconds)
    }

    pub(crate) fn set_target(&mut self, target: EntityId) {
        self.phase = TurretAttackPhase::Working;
        self.target = Some(target);
    }

    pub(crate) fn stop(&mut self) {
        self.set_phase(TurretAttackPhase::None);
    }

    pub(crate) fn set_phase(&mut self, phase: TurretAttackPhase) {
        self.phase = phase;
        self.target = None;
        self.timing.stop_firing();
    }

    pub(crate) fn set_attack_wait(&mut self, duration: f32) {
        self.attack_wait_remaining = if duration.is_finite() && duration > 0.0 {
            duration
        } else {
            0.0
        };
    }

    fn advance_attack_wait(&mut self, elapsed: f32) {
        if elapsed.is_finite() && elapsed > 0.0 {
            self.attack_wait_remaining = (self.attack_wait_remaining - elapsed).max(0.0);
        }
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_string(&self.action_name);
        checksum.hash_u32(match self.kind {
            TurretAttackKind::Secondary => 0,
            TurretAttackKind::Slave => 1,
        });
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(
            self.target
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_f32(self.attack_wait_remaining);
        self.timing.hash_state(checksum);
    }
}

/// Per-unit collection of concurrently updating auxiliary turrets.
#[derive(Debug, Clone, Default)]
pub struct UnitTurretCombat {
    attacks: BTreeMap<String, TurretAttackState>,
    secondary_scan_tokens: BTreeSet<String>,
}

impl UnitTurretCombat {
    /// Iterate tracks in deterministic normalized action-name order.
    pub fn attacks(&self) -> impl Iterator<Item = &TurretAttackState> {
        self.attacks.values()
    }

    pub(crate) fn attack_mut(
        &mut self,
        action_name: &str,
        kind: TurretAttackKind,
    ) -> &mut TurretAttackState {
        self.attacks
            .entry(canonical_name(action_name))
            .and_modify(|attack| attack.kind = kind)
            .or_insert_with(|| TurretAttackState::new(action_name, kind))
    }

    pub(crate) fn get(&self, action_name: &str) -> Option<&TurretAttackState> {
        self.attacks.get(&canonical_name(action_name))
    }

    pub(crate) fn stop(&mut self, action_name: &str) {
        if let Some(attack) = self.attacks.get_mut(&canonical_name(action_name)) {
            attack.stop();
        }
    }

    pub(crate) fn set_phase(&mut self, action_name: &str, phase: TurretAttackPhase) {
        if let Some(attack) = self.attacks.get_mut(&canonical_name(action_name)) {
            attack.set_phase(phase);
        }
    }

    pub(crate) fn stop_except(&mut self, active_actions: &[String]) {
        for (name, attack) in &mut self.attacks {
            if !active_actions
                .iter()
                .any(|active| name.eq_ignore_ascii_case(active))
            {
                attack.stop();
            }
        }
    }

    pub(crate) fn advance_attack_wait_timers(&mut self, elapsed: f32) {
        for attack in self.attacks.values_mut() {
            attack.advance_attack_wait(elapsed);
        }
    }

    pub(crate) fn grant_secondary_scan_token(&mut self, hardpoint_name: &str) {
        self.secondary_scan_tokens
            .insert(canonical_name(hardpoint_name));
    }

    pub(crate) fn consume_secondary_scan_token(&mut self, hardpoint_name: &str) -> bool {
        self.secondary_scan_tokens
            .remove(&canonical_name(hardpoint_name))
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.attacks.len()).unwrap_or(u32::MAX));
        for attack in self.attacks.values() {
            attack.hash_state(checksum);
        }
        checksum.hash_u32(u32::try_from(self.secondary_scan_tokens.len()).unwrap_or(u32::MAX));
        for hardpoint in &self.secondary_scan_tokens {
            checksum.hash_string(hardpoint);
        }
    }
}

fn canonical_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_are_case_insensitive_and_stop_when_no_longer_active() {
        let mut combat = UnitTurretCombat::default();
        let target = EntityId::new(crate::entity_id::EntityClass::Unit, 7);
        combat
            .attack_mut("LeftGun", TurretAttackKind::Secondary)
            .set_target(target);

        assert_eq!(
            combat
                .attack_mut("leftgun", TurretAttackKind::Secondary)
                .target(),
            Some(target)
        );
        combat.stop_except(&[]);
        assert_eq!(combat.get("LEFTGUN").unwrap().target(), None);
    }

    #[test]
    fn stopping_a_track_does_not_change_its_executor_kind() {
        let mut combat = UnitTurretCombat::default();
        combat.attack_mut("Cannon", TurretAttackKind::Slave);

        combat.stop("cannon");

        assert_eq!(
            combat.get("CANNON").unwrap().kind(),
            TurretAttackKind::Slave
        );
    }

    #[test]
    fn secondary_scan_tokens_persist_and_are_shared_by_hardpoint() {
        let mut combat = UnitTurretCombat::default();
        combat.grant_secondary_scan_token("PassengerTurret");
        combat.grant_secondary_scan_token("passengerturret");

        assert!(combat.consume_secondary_scan_token("PASSENGERTURRET"));
        assert!(!combat.consume_secondary_scan_token("PassengerTurret"));
    }
}
