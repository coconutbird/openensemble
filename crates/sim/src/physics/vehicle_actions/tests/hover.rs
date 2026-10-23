//! The hover flight action: the flight height and the crash branch.

use glam::Vec3;
use pipeline::database::hw1::physics::PhysicsVehicleType;

use super::{Scene, squad_at};
use crate::physics::vehicle_actions::input::{
    AirInput, CrashInput, HeightSample, UnitInput, VehicleActionInput,
};

const TERRAIN: f32 = 10.0;

fn air(crash: Option<CrashInput>) -> AirInput {
    let sample = HeightSample {
        terrain: TERRAIN,
        flight: TERRAIN + 16.0,
    };
    AirInput {
        hover_altitude_offset: 0.0,
        max_pitch_roll: None,
        crash,
        avoiding_obstacle_altitude: None,
        speed_limited: false,
        transport_hover_offset: None,
        body_samples: [sample; 3],
        future_samples: [sample; 5],
    }
}

fn hover_input(unit: UnitInput, crash: Option<CrashInput>) -> VehicleActionInput {
    VehicleActionInput {
        unit,
        squad: Some(squad_at(unit.position, Vec3::Z, None)),
        air: Some(air(crash)),
    }
}

#[test]
fn the_hover_action_holds_the_flight_height() {
    let start = Vec3::new(60.0, 30.0, 60.0);
    let mut scene = Scene::new(PhysicsVehicleType::Hornet, None, start, 0.0, |_, _| TERRAIN);
    let mut squad_height = None;
    for _ in 0..400 {
        squad_height = scene.tick(|unit| hover_input(unit, None));
    }
    let unit = scene.unit().position;
    assert!(
        (unit.y - (TERRAIN + 16.0)).abs() < 0.1,
        "the aircraft holds {} above the terrain",
        unit.y - TERRAIN
    );
    assert!(Vec3::new(unit.x - start.x, 0.0, unit.z - start.z).length() < 0.1);
    assert!(scene.velocity().length() < 0.05, "{}", scene.velocity());
    assert!(scene.rotation().y_axis.y > 0.999);
    // The action moves the squad to the height that the readback gave the unit.
    let squad_height = squad_height.expect("the hornet has a squad");
    assert!((squad_height - unit.y).abs() < 0.01);
}

#[test]
fn a_crashing_aircraft_dives_through_its_crash_position() {
    let start = Vec3::new(40.0, 40.0, 60.0);
    let mut scene = Scene::new(PhysicsVehicleType::Banshee, None, start, 0.0, |_, _| {
        TERRAIN
    });
    let crash = CrashInput {
        position: Vec3::new(80.0, TERRAIN, 60.0),
        kamikaze_target: None,
    };
    for _ in 0..40 {
        scene.tick(|unit| hover_input(unit, Some(crash)));
    }
    let unit = scene.unit().position;
    assert!(unit.y < start.y - 5.0, "the aircraft fell to {}", unit.y);
    assert!(unit.x > start.x + 5.0, "the aircraft flew to {}", unit.x);
    assert!(scene.velocity().y < 0.0);
}
