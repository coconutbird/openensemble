//! `BEntity_setupVehiclePhysics`: the action, the inertia, the filter and the hull by type.

use glam::Vec3;
use pipeline::database::hw1::physics::PhysicsVehicleType;

use super::ground::scorpion_info;
use super::{Scene, VehicleFilter};
use crate::physics::SimPhysics;
use crate::physics::vehicle_actions::{ActionKind, GroundVariant, VehicleAction};

fn scene(vehicle_type: PhysicsVehicleType) -> Scene {
    let info = scorpion_info();
    Scene::new(
        vehicle_type,
        Some(&info),
        Vec3::new(60.0, 10.0, 60.0),
        0.0,
        |_, _| 10.0,
    )
}

fn inertia(scene: &Scene) -> Vec3 {
    scene.world().inertia_local(scene.body).expect("body")
}

#[test]
fn the_warthog_ghost_and_aircraft_get_the_vehicle_filter_and_the_hardcoded_inertia() {
    for vehicle_type in [
        PhysicsVehicleType::Warthog,
        PhysicsVehicleType::Chopper,
        PhysicsVehicleType::Ghost,
        PhysicsVehicleType::Hawk,
        PhysicsVehicleType::Sentinel,
    ] {
        let scene = scene(vehicle_type);
        assert_eq!(
            scene.setup.filter,
            VehicleFilter::Vehicles,
            "{vehicle_type:?}"
        );
        assert_eq!(scene.setup.vehicle_hull, !vehicle_type.is_aircraft());
        assert!((inertia(&scene) - Vec3::new(150.0, 150.0, 60.0)).length() < 1e-3);
    }
    let chopper = scene(PhysicsVehicleType::Chopper);
    assert!(matches!(
        chopper.action.kind(),
        ActionKind::Warthog(warthog) if warthog.is_chopper()
    ));
    let hawk = scene(PhysicsVehicleType::Hawk);
    assert!(hawk.action.is_hover());
}

#[test]
fn the_ground_family_gets_the_no_terrain_filter_and_the_data_inertia() {
    for (vehicle_type, variant) in [
        (PhysicsVehicleType::Scorpion, GroundVariant::Scorpion),
        (PhysicsVehicleType::Rhino, GroundVariant::Scorpion),
        (PhysicsVehicleType::Reactor, GroundVariant::Scorpion),
        (PhysicsVehicleType::Cobra, GroundVariant::Cobra),
        (PhysicsVehicleType::Gremlin, GroundVariant::Cobra),
        (PhysicsVehicleType::Wolverine, GroundVariant::Wolverine),
        (PhysicsVehicleType::Ground, GroundVariant::Base),
    ] {
        let scene = scene(vehicle_type);
        assert_eq!(scene.setup.filter, VehicleFilter::NoTerrainVehicles);
        assert!(scene.setup.vehicle_hull);
        // `diag(pitch · m, yaw · m, roll · m)` with 3, 3 and 2.4.
        assert!((inertia(&scene) - Vec3::new(450.0, 450.0, 360.0)).length() < 1e-3);
        assert!(matches!(
            scene.action.kind(),
            ActionKind::Ground(ground) if ground.variant() == variant
        ));
        assert_eq!(scene.action.vehicle_type(), vehicle_type);
    }
}

#[test]
fn no_vehicle_type_gives_no_action() {
    let mut physics = SimPhysics::new();
    let body = physics.world_mut().add_body(physics::BodyDesc::new(
        std::sync::Arc::new(physics::Shape::Sphere { radius: 1.0 }),
        physics::MotionType::BoxInertia,
        Vec3::ZERO,
    ));
    assert!(
        VehicleAction::setup(physics.world_mut(), body, PhysicsVehicleType::None, None).is_none()
    );
}
