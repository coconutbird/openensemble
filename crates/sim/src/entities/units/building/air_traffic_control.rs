//! Persistent landing-pad reservations owned by an air-base unit.

use crate::entities::squads::formation_offset_to_world;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Fixed retail landing-pad capacity of one air-traffic-control action.
pub const AIR_TRAFFIC_LANDING_SPOT_COUNT: usize = 8;

/// One world-space landing pad and its current aircraft reservation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirTrafficLandingSpot {
    position: Vec3,
    forward: Vec3,
    aircraft_id: Option<EntityId>,
}

impl AirTrafficLandingSpot {
    const fn new(position: Vec3, forward: Vec3) -> Self {
        Self {
            position,
            forward,
            aircraft_id: None,
        }
    }

    /// World-space center of this landing pad.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// World-space aircraft facing assigned to this pad.
    #[must_use]
    pub const fn forward(self) -> Vec3 {
        self.forward
    }

    /// Aircraft currently holding this pad, if any.
    #[must_use]
    pub const fn aircraft_id(self) -> Option<EntityId> {
        self.aircraft_id
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_vec3(self.position.x, self.position.y, self.position.z);
        checksum.hash_vec3(self.forward.x, self.forward.y, self.forward.z);
        checksum.hash_u32(self.aircraft_id.map_or(u32::MAX, EntityId::as_u32));
    }
}

/// Authoritative state of one persistent retail `AirTrafficControl` action.
#[derive(Debug, Clone, PartialEq)]
pub struct AirTrafficControl {
    action_name: String,
    landing_spots: [AirTrafficLandingSpot; AIR_TRAFFIC_LANDING_SPOT_COUNT],
}

impl AirTrafficControl {
    pub(crate) fn new(
        action_name: &str,
        owner_position: Vec3,
        owner_forward: Vec3,
        unsc_layout: bool,
    ) -> Self {
        let landing_spots = if unsc_layout {
            unsc_landing_spots(owner_position, owner_forward)
        } else {
            radial_landing_spots(owner_position, owner_forward)
        };
        Self {
            action_name: action_name.to_owned(),
            landing_spots,
        }
    }

    /// Authored persistent action represented by this state.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// All eight pads in retail request order.
    #[must_use]
    pub const fn landing_spots(&self) -> &[AirTrafficLandingSpot; AIR_TRAFFIC_LANDING_SPOT_COUNT] {
        &self.landing_spots
    }

    /// Pad already held by one aircraft.
    #[must_use]
    pub fn assignment(&self, aircraft_id: EntityId) -> Option<AirTrafficLandingSpot> {
        self.landing_spots
            .iter()
            .copied()
            .find(|spot| spot.aircraft_id == Some(aircraft_id))
    }

    pub(crate) fn request_landing_spot(
        &mut self,
        aircraft_id: EntityId,
    ) -> Option<AirTrafficLandingSpot> {
        let spot = self
            .landing_spots
            .iter_mut()
            .find(|spot| spot.aircraft_id.is_none())?;
        spot.aircraft_id = Some(aircraft_id);
        Some(*spot)
    }

    pub(crate) fn release_aircraft(&mut self, aircraft_id: EntityId) -> bool {
        let Some(spot) = self
            .landing_spots
            .iter_mut()
            .find(|spot| spot.aircraft_id == Some(aircraft_id))
        else {
            return false;
        };
        spot.aircraft_id = None;
        true
    }

    pub(crate) fn retain_live_aircraft(&mut self, mut is_live: impl FnMut(EntityId) -> bool) {
        for spot in &mut self.landing_spots {
            if spot
                .aircraft_id
                .is_some_and(|aircraft_id| !is_live(aircraft_id))
            {
                spot.aircraft_id = None;
            }
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.action_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.action_name.as_bytes());
        for spot in self.landing_spots {
            spot.hash_state(checksum);
        }
    }
}

fn unsc_landing_spots(
    owner_position: Vec3,
    owner_forward: Vec3,
) -> [AirTrafficLandingSpot; AIR_TRAFFIC_LANDING_SPOT_COUNT] {
    const OFFSETS: [Vec3; AIR_TRAFFIC_LANDING_SPOT_COUNT] = [
        Vec3::new(3.0, 3.0, 0.0),
        Vec3::new(3.0, 3.0, -7.0),
        Vec3::new(3.0, 3.0, -14.0),
        Vec3::new(3.0, 3.0, -21.0),
        Vec3::new(-6.0, 3.0, -10.0),
        Vec3::new(-6.0, 3.0, -17.0),
        Vec3::new(-6.0, 3.0, -24.0),
        Vec3::new(-6.0, 3.0, -31.0),
    ];
    std::array::from_fn(|index| {
        let forward = if index < 4 {
            Vec3::new(1.0, 0.0, -1.0).normalize()
        } else {
            Vec3::new(-1.0, 0.0, -1.0).normalize()
        };
        AirTrafficLandingSpot::new(
            owner_position + formation_offset_to_world(owner_forward, OFFSETS[index]),
            forward,
        )
    })
}

fn radial_landing_spots(
    owner_position: Vec3,
    owner_forward: Vec3,
) -> [AirTrafficLandingSpot; AIR_TRAFFIC_LANDING_SPOT_COUNT] {
    const OFFSETS: [Vec3; AIR_TRAFFIC_LANDING_SPOT_COUNT] = [
        Vec3::new(0.0, 3.0, 20.0),
        Vec3::new(14.14, 3.0, 14.14),
        Vec3::new(20.0, 3.0, 0.0),
        Vec3::new(14.14, 3.0, -14.14),
        Vec3::new(0.0, 3.0, -24.0),
        Vec3::new(-14.14, 3.0, -14.14),
        Vec3::new(-20.0, 3.0, 0.0),
        Vec3::new(-14.14, 3.0, 14.14),
    ];
    OFFSETS.map(|offset| {
        let position = owner_position + formation_offset_to_world(owner_forward, offset);
        let radial = position - owner_position;
        let forward = Vec3::new(radial.x, 0.0, radial.z).normalize_or(Vec3::X);
        AirTrafficLandingSpot::new(position, forward)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn layouts_match_source_offsets_and_facing_rules() {
        let owner = Vec3::new(10.0, 2.0, 20.0);
        let unsc = AirTrafficControl::new("Controller", owner, Vec3::X, true);
        assert_eq!(unsc.landing_spots[0].position, Vec3::new(10.0, 5.0, 17.0));
        assert_eq!(unsc.landing_spots[1].position, Vec3::new(3.0, 5.0, 17.0));
        assert!(
            unsc.landing_spots[0]
                .forward
                .abs_diff_eq(Vec3::new(1.0, 0.0, -1.0).normalize(), 1.0e-6)
        );

        let covenant = AirTrafficControl::new("Controller", owner, Vec3::X, false);
        assert_eq!(
            covenant.landing_spots[0].position,
            Vec3::new(30.0, 5.0, 20.0)
        );
        assert_eq!(
            covenant.landing_spots[2].position,
            Vec3::new(10.0, 5.0, 0.0)
        );
        assert!(
            covenant.landing_spots[0]
                .forward
                .abs_diff_eq(Vec3::X, 1.0e-6)
        );
        assert!(
            covenant.landing_spots[2]
                .forward
                .abs_diff_eq(Vec3::NEG_Z, 1.0e-6)
        );
    }

    #[test]
    fn first_free_reservations_release_and_checksum() {
        let mut control = AirTrafficControl::new("Controller", Vec3::ZERO, Vec3::Z, true);
        let initial_checksum = checksum(&control);
        let aircraft = EntityId::new(EntityClass::Unit, 4);
        let assigned = control.request_landing_spot(aircraft).unwrap();
        assert_eq!(assigned.position(), control.landing_spots[0].position());
        assert_eq!(control.assignment(aircraft), Some(assigned));
        assert_ne!(checksum(&control), initial_checksum);
        assert!(control.release_aircraft(aircraft));
        assert_eq!(checksum(&control), initial_checksum);
    }

    fn checksum(control: &AirTrafficControl) -> u32 {
        let mut checksum = SyncChecksum::new();
        control.hash_state(&mut checksum);
        checksum.value()
    }
}
