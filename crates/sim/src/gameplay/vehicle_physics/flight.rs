//! Retail flight-controller selection from layered prototype physics and flags.

use crate::entities::FlightControllerKind;
use crate::sync::SyncChecksum;
use pipeline::database::hw1::ProtoObject;
use pipeline::database::hw1::physics::PhysicsVehicleType;

/// Layered prototype rule used to select a retail flying movement action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlightControllerProfile {
    kind: FlightControllerKind,
    requires_enable_flight: bool,
}

impl FlightControllerProfile {
    const fn physics_hover() -> Self {
        Self {
            kind: FlightControllerKind::PhysicsHover,
            requires_enable_flight: false,
        }
    }

    const fn move_air(requires_enable_flight: bool) -> Self {
        Self {
            kind: FlightControllerKind::MoveAir,
            requires_enable_flight,
        }
    }

    pub(crate) const fn resolve(self, enable_flight: bool) -> FlightControllerKind {
        if self.requires_enable_flight && !enable_flight {
            FlightControllerKind::Direct
        } else {
            self.kind
        }
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.kind as u32);
        checksum.hash_u32(u32::from(self.requires_enable_flight));
    }
}

pub(super) fn controller_profile(
    object: &ProtoObject,
    physics_vehicle_type: Option<PhysicsVehicleType>,
) -> Option<FlightControllerProfile> {
    if let Some(vehicle_type) = physics_vehicle_type {
        return is_physics_hover_vehicle(vehicle_type)
            .then_some(FlightControllerProfile::physics_hover());
    }
    if has_flag(object, "AirMovement") {
        Some(FlightControllerProfile::move_air(false))
    } else if has_flag(object, "Flying") {
        Some(FlightControllerProfile::move_air(true))
    } else {
        None
    }
}

fn is_physics_hover_vehicle(vehicle_type: PhysicsVehicleType) -> bool {
    matches!(
        vehicle_type,
        PhysicsVehicleType::Hawk
            | PhysicsVehicleType::Hornet
            | PhysicsVehicleType::Vulture
            | PhysicsVehicleType::Banshee
            | PhysicsVehicleType::Vampire
            | PhysicsVehicleType::Sentinel
    )
}

fn has_flag(object: &ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physics_aircraft_select_the_hover_controller() {
        let object = ProtoObject::default();
        for vehicle_type in [
            PhysicsVehicleType::Hawk,
            PhysicsVehicleType::Hornet,
            PhysicsVehicleType::Vulture,
            PhysicsVehicleType::Banshee,
            PhysicsVehicleType::Vampire,
            PhysicsVehicleType::Sentinel,
        ] {
            assert_eq!(
                controller_profile(&object, Some(vehicle_type)),
                Some(FlightControllerProfile::physics_hover())
            );
        }
        assert_eq!(
            controller_profile(&object, Some(PhysicsVehicleType::Ghost)),
            None
        );
    }

    #[test]
    fn nonphysics_flags_match_retail_precedence_and_config_gate() {
        let air_movement = ProtoObject {
            flags: vec!["Flying".to_owned(), "AirMovement".to_owned()],
            ..ProtoObject::default()
        };
        let flying = ProtoObject {
            flags: vec!["Flying".to_owned()],
            ..ProtoObject::default()
        };

        let unconditional = controller_profile(&air_movement, None).unwrap();
        assert_eq!(unconditional.resolve(false), FlightControllerKind::MoveAir);
        let gated = controller_profile(&flying, None).unwrap();
        assert_eq!(gated.resolve(false), FlightControllerKind::Direct);
        assert_eq!(gated.resolve(true), FlightControllerKind::MoveAir);
        assert_eq!(
            controller_profile(&air_movement, Some(PhysicsVehicleType::Ground)),
            None,
            "PhysicsInfo prevents the nonphysics flag fallback"
        );
    }
}
