//! Checksummed state for retail's persistent `BUnitActionBomb`.

use super::Unit;
use crate::physics::PhysicsBody;
use crate::sync::SyncChecksum;

/// Observable phase of a unit's persistent `Bomb` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum BombPhase {
    /// No enabled persistent `Bomb` action has connected.
    #[default]
    Inactive = 0,
    /// The action owns orientation and is waiting for a terrain impact.
    Working = 1,
    /// A terrain impact released the action back to normal simulation control.
    Complete = 2,
}

#[derive(Debug, Clone, Default)]
pub(super) struct UnitBomb {
    action_name: Option<String>,
    phase: BombPhase,
    rolls: bool,
    collided: bool,
    release_physics_on_completion: bool,
    original_physics: Option<PhysicsBody>,
}

impl UnitBomb {
    fn begin(
        &mut self,
        action_name: &str,
        rolls: bool,
        release_physics_on_completion: bool,
        original_physics: Option<PhysicsBody>,
    ) -> bool {
        if self.phase != BombPhase::Inactive {
            return false;
        }
        self.action_name = Some(action_name.to_owned());
        self.phase = BombPhase::Working;
        self.rolls = rolls;
        self.collided = false;
        self.release_physics_on_completion = release_physics_on_completion;
        self.original_physics = original_physics;
        true
    }

    fn collide(&mut self) -> bool {
        if self.phase != BombPhase::Working {
            return false;
        }
        self.collided = true;
        true
    }

    fn finish(&mut self) -> Option<(bool, Option<PhysicsBody>)> {
        if self.phase != BombPhase::Working || !self.collided {
            return None;
        }
        self.phase = BombPhase::Complete;
        Some((
            self.release_physics_on_completion,
            self.original_physics.take(),
        ))
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        hash_optional_string(checksum, self.action_name.as_deref());
        checksum.hash_u32(u32::from(self.rolls));
        checksum.hash_u32(u32::from(self.collided));
        checksum.hash_u32(u32::from(self.release_physics_on_completion));
        if let Some(physics) = &self.original_physics {
            checksum.hash_u32(1);
            physics.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
    }
}

impl Unit {
    /// Return the persistent `Bomb` action's current phase.
    #[must_use]
    pub const fn bomb_phase(&self) -> BombPhase {
        self.detonate.bomb.phase
    }

    /// Return the connected persistent `Bomb` action name.
    #[must_use]
    pub fn bomb_action_name(&self) -> Option<&str> {
        self.detonate.bomb.action_name.as_deref()
    }

    /// Return whether the action sampled its rolling branch.
    #[must_use]
    pub const fn bomb_rolls(&self) -> bool {
        self.detonate.bomb.rolls
    }

    /// Return whether the body has received a descending terrain contact.
    #[must_use]
    pub const fn bomb_has_collided(&self) -> bool {
        self.detonate.bomb.collided
    }

    pub(crate) fn begin_bomb_action(
        &mut self,
        action_name: &str,
        rolls: bool,
        release_physics_on_completion: bool,
        original_physics: Option<PhysicsBody>,
    ) -> bool {
        self.detonate.bomb.begin(
            action_name,
            rolls,
            release_physics_on_completion,
            original_physics,
        )
    }

    pub(crate) fn notify_bomb_ground_collision(&mut self) -> bool {
        self.detonate.bomb.collide()
    }

    pub(crate) fn finish_bomb_action(&mut self) -> Option<(bool, Option<PhysicsBody>)> {
        self.detonate.bomb.finish()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::{EntityClass, EntityId};

    #[test]
    fn completion_is_sticky_and_returns_action_owned_physics() {
        let mut unit = Unit::new(EntityId::new(EntityClass::Unit, 1), 1);
        let original = PhysicsBody::static_obstruction(crate::physics::BoxCollider::new(
            glam::Vec3::ONE,
            glam::Vec3::ZERO,
        ));
        assert!(unit.begin_bomb_action("Bomb", true, true, Some(original.clone())));
        assert_eq!(unit.bomb_phase(), BombPhase::Working);
        assert!(unit.bomb_rolls());
        assert!(unit.notify_bomb_ground_collision());
        let completion = unit.finish_bomb_action().expect("completed Bomb action");
        assert!(completion.0);
        assert_eq!(completion.1, Some(original));
        assert_eq!(unit.bomb_phase(), BombPhase::Complete);
        assert!(!unit.begin_bomb_action("Bomb", false, false, None));
    }
}
