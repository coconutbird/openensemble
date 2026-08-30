//! Persistent ambient-life state owned by the deterministic simulation.

use super::Squad;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

pub(crate) const DEVOUR_DURATION_MS: u32 = 10_000;

/// Current retail ambient-life behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AmbientLifeBehavior {
    /// Waiting to choose another random movement destination.
    Wander = 0,
    /// Periodically selecting a prey member to attack.
    Hunt = 1,
    /// Preparing one movement away from a dangerous squad.
    Flee = 2,
    /// Waiting for a movement child action to finish.
    Idle = 3,
    /// Pausing for ten seconds after killing prey.
    Devour = 4,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum AmbientLifePhase {
    #[default]
    Disconnected = 0,
    Starting = 1,
    Working = 2,
    Moving = 3,
    Attacking = 4,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SquadAmbientLife {
    pub(crate) phase: AmbientLifePhase,
    pub(crate) behavior: AmbientLifeBehavior,
    pub(crate) wander_timer_ms: u32,
    pub(crate) predator_timer_ms: u32,
    pub(crate) devour_timer_ms: u32,
    pub(crate) prey_timer_ms: u32,
    pub(crate) dangerous_squad: Option<EntityId>,
    pub(crate) prey_squad: Option<EntityId>,
    pub(crate) current_prey_unit: Option<EntityId>,
    pub(crate) target_position: Option<Vec3>,
    pub(crate) owned_move_target: Option<Vec3>,
    pub(crate) movement_modifier: f32,
    pub(crate) fleeing: bool,
    pub(crate) leaving_map: bool,
}

impl Default for SquadAmbientLife {
    fn default() -> Self {
        Self {
            phase: AmbientLifePhase::Disconnected,
            behavior: AmbientLifeBehavior::Wander,
            wander_timer_ms: 0,
            predator_timer_ms: 0,
            devour_timer_ms: DEVOUR_DURATION_MS,
            prey_timer_ms: 0,
            dangerous_squad: None,
            prey_squad: None,
            current_prey_unit: None,
            target_position: None,
            owned_move_target: None,
            movement_modifier: 1.0,
            fleeing: false,
            leaving_map: false,
        }
    }
}

impl SquadAmbientLife {
    pub(crate) const fn is_initialized(self) -> bool {
        !matches!(self.phase, AmbientLifePhase::Disconnected)
    }

    pub(crate) fn initialize(
        &mut self,
        wander_timer_ms: u32,
        predator_timer_ms: u32,
        prey_timer_ms: u32,
    ) {
        if self.is_initialized() {
            return;
        }
        self.phase = AmbientLifePhase::Starting;
        self.behavior = AmbientLifeBehavior::Wander;
        self.wander_timer_ms = wander_timer_ms;
        self.predator_timer_ms = predator_timer_ms;
        self.devour_timer_ms = DEVOUR_DURATION_MS;
        self.prey_timer_ms = prey_timer_ms;
        self.movement_modifier = 1.0;
    }

    pub(crate) fn disconnect(&mut self) -> (Option<Vec3>, Option<EntityId>) {
        let owned = (self.owned_move_target, self.current_prey_unit);
        *self = Self::default();
        owned
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.behavior as u32);
        checksum.hash_u32(self.wander_timer_ms);
        checksum.hash_u32(self.predator_timer_ms);
        checksum.hash_u32(self.devour_timer_ms);
        checksum.hash_u32(self.prey_timer_ms);
        hash_optional_entity(checksum, self.dangerous_squad);
        hash_optional_entity(checksum, self.prey_squad);
        hash_optional_entity(checksum, self.current_prey_unit);
        hash_optional_vec3(checksum, self.target_position);
        hash_optional_vec3(checksum, self.owned_move_target);
        checksum.hash_f32(self.movement_modifier);
        checksum.hash_u32(u32::from(self.fleeing));
        checksum.hash_u32(u32::from(self.leaving_map));
    }
}

impl Squad {
    /// Whether a persistent ambient-life action is connected to this squad.
    #[must_use]
    pub const fn has_ambient_life(&self) -> bool {
        self.ambient_life.is_initialized()
    }

    /// Current ambient behavior, or `None` when the action is disconnected.
    #[must_use]
    pub const fn ambient_life_behavior(&self) -> Option<AmbientLifeBehavior> {
        if self.ambient_life.is_initialized() {
            Some(self.ambient_life.behavior)
        } else {
            None
        }
    }

    /// Latest source-selected ambient movement target.
    #[must_use]
    pub const fn ambient_life_target(&self) -> Option<Vec3> {
        self.ambient_life.target_position
    }

    /// Squad currently treated as the ambient creature's danger source.
    #[must_use]
    pub const fn ambient_life_dangerous_squad(&self) -> Option<EntityId> {
        self.ambient_life.dangerous_squad
    }

    /// Squad currently treated as prey by the ambient creature.
    #[must_use]
    pub const fn ambient_life_prey_squad(&self) -> Option<EntityId> {
        self.ambient_life.prey_squad
    }

    /// Whether the flee child action currently owns its speed modifier.
    #[must_use]
    pub const fn is_ambient_life_fleeing(&self) -> bool {
        self.ambient_life.fleeing
    }

    pub(crate) const fn ambient_life_movement_modifier(&self) -> f32 {
        self.ambient_life.movement_modifier
    }
}

pub(crate) fn countdown_due(timer_ms: &mut u32, elapsed_ms: u32) -> bool {
    if *timer_ms <= elapsed_ms {
        true
    } else {
        *timer_ms -= elapsed_ms;
        false
    }
}

fn hash_optional_entity(checksum: &mut SyncChecksum, value: Option<EntityId>) {
    checksum.hash_u32(value.map_or(u32::MAX, EntityId::as_u32));
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_vec3(value.x, value.y, value.z);
    } else {
        checksum.hash_u32(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn countdown_uses_retail_less_than_or_equal_boundary() {
        let mut timer = 100;
        assert!(!countdown_due(&mut timer, 50));
        assert_eq!(timer, 50);
        assert!(countdown_due(&mut timer, 50));
        assert_eq!(timer, 50);
    }

    #[test]
    fn disconnect_returns_child_targets_and_resets_speed() {
        let mut state = SquadAmbientLife::default();
        state.initialize(10, 20, 30);
        state.phase = AmbientLifePhase::Moving;
        state.owned_move_target = Some(Vec3::X);
        state.current_prey_unit = Some(EntityId::new(EntityClass::Unit, 4));
        state.movement_modifier = 1.5;

        let owned = state.disconnect();
        assert_eq!(owned.0, Some(Vec3::X));
        assert!(owned.1.is_some());
        assert!(!state.is_initialized());
        assert_eq!(state.movement_modifier.to_bits(), 1.0_f32.to_bits());
    }
}
