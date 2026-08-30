//! Air-unit approach state for retail trained-squad `FlyIn` births.

use crate::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Sim-owned vertical approach performed by an aircraft trained with `Birth=FlyIn`.
#[derive(Debug, Clone, PartialEq)]
pub struct SquadTrainedAirBirth {
    leader_unit_id: EntityId,
    current_position: Vec3,
    landing_position: Vec3,
    rally_point: Option<Vec3>,
    speed: f32,
}

impl SquadTrainedAirBirth {
    pub(crate) fn new(
        leader_unit_id: EntityId,
        landing_position: Vec3,
        rally_point: Option<Vec3>,
        speed: f32,
    ) -> Self {
        Self {
            leader_unit_id,
            current_position: landing_position + Vec3::Y * 100.0,
            landing_position,
            rally_point: rally_point.filter(|point| point.is_finite()),
            speed: valid_speed(speed),
        }
    }

    /// Aircraft unit whose position is controlled by this approach.
    #[must_use]
    pub const fn leader_unit_id(&self) -> EntityId {
        self.leader_unit_id
    }

    /// Current authoritative aircraft position.
    #[must_use]
    pub const fn current_position(&self) -> Vec3 {
        self.current_position
    }

    /// Ground/flight-plane position at which normal squad control resumes.
    #[must_use]
    pub const fn landing_position(&self) -> Vec3 {
        self.landing_position
    }

    pub(crate) const fn rally_point(&self) -> Option<Vec3> {
        self.rally_point
    }

    pub(crate) fn advance(&mut self, dt: f32) -> bool {
        let delta = self.landing_position - self.current_position;
        let distance = delta.length();
        if !distance.is_finite() || distance <= self.speed * dt {
            self.current_position = self.landing_position;
            return true;
        }
        self.current_position += delta / distance * (self.speed * dt);
        false
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.leader_unit_id.as_u32());
        hash_vec3(checksum, self.current_position);
        hash_vec3(checksum, self.landing_position);
        checksum.hash_u32(u32::from(self.rally_point.is_some()));
        if let Some(rally_point) = self.rally_point {
            hash_vec3(checksum, rally_point);
        }
        checksum.hash_f32(self.speed);
    }
}

fn valid_speed(speed: f32) -> f32 {
    if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        30.0
    }
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn approach_starts_one_hundred_units_above_landing_and_advances() {
        let leader = EntityId::new(EntityClass::Unit, 3);
        let mut action = SquadTrainedAirBirth::new(leader, Vec3::X, Some(Vec3::Z), 25.0);

        assert_eq!(action.current_position(), Vec3::new(1.0, 100.0, 0.0));
        assert!(!action.advance(2.0));
        assert_eq!(action.current_position(), Vec3::new(1.0, 50.0, 0.0));
        assert!(action.advance(2.0));
        assert_eq!(action.current_position(), Vec3::X);
    }
}
