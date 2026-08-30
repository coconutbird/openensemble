use super::*;
use crate::entities::FlightControllerKind;
use crate::gameplay::GameplayCatalog;
use glam::Vec3;
use pipeline::database::hw1::objects::{TrainLimit, TrainLimitType};
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::{Civ, Database, ProtoObject, Squad as ProtoSquad};

#[test]
fn controller_initializes_once_and_honors_live_enablement() {
    let (mut world, database, gameplay, controller_id) = fixture(true);
    world.update_air_traffic_controls(&database, &gameplay);
    let control = world
        .get_unit(controller_id)
        .unwrap()
        .production
        .air_traffic_control()
        .expect("enabled persistent controller");
    assert_eq!(
        control.landing_spots()[0].position(),
        Vec3::new(13.0, 5.0, 20.0)
    );

    world.get_unit_mut(controller_id).unwrap().base.position = Vec3::splat(100.0);
    world.update_air_traffic_controls(&database, &gameplay);
    assert_eq!(
        world
            .get_unit(controller_id)
            .unwrap()
            .production
            .air_traffic_control()
            .unwrap()
            .landing_spots()[0]
            .position(),
        Vec3::new(13.0, 5.0, 20.0)
    );

    world
        .get_unit_mut(controller_id)
        .unwrap()
        .actions
        .set_enabled("Controller", false);
    world.update_air_traffic_controls(&database, &gameplay);
    assert!(
        world
            .get_unit(controller_id)
            .unwrap()
            .production
            .air_traffic_control()
            .is_none()
    );
}

#[test]
fn fly_in_birth_does_not_consume_an_air_traffic_pad() {
    let (mut world, database, gameplay, controller_id) = fixture(true);
    world.update_air_traffic_controls(&database, &gameplay);
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let aircraft_id = world.create_unit_at(1, Vec3::ZERO);
    {
        let aircraft = world.get_unit_mut(aircraft_id).unwrap();
        aircraft.flying = true;
        aircraft.speed = 200.0;
    }
    assert!(world.attach_unit_to_squad(aircraft_id, squad_id));
    let placement = super::super::placement::SquadBirthPlacement {
        position: Vec3::new(50.0, 0.0, 50.0),
        forward: Vec3::X,
        obstruction_radius: 1.0,
        preferred: true,
    };

    assert!(world.start_trained_squad_fly_in(&database, 1, squad_id, placement, None,));
    let first = world
        .get_unit(controller_id)
        .unwrap()
        .production
        .air_traffic_control()
        .unwrap()
        .landing_spots()[0];
    assert_eq!(first.aircraft_id(), None);
    assert_eq!(
        world
            .get_squad(squad_id)
            .unwrap()
            .trained_air_birth()
            .unwrap()
            .landing_position(),
        placement.position
    );

    world.update_trained_air_births(1.0);
    assert!(
        world
            .get_squad(squad_id)
            .unwrap()
            .trained_air_birth()
            .is_none()
    );
    assert_eq!(
        world
            .get_unit(controller_id)
            .unwrap()
            .production
            .air_traffic_control()
            .unwrap()
            .landing_spots()[0]
            .aircraft_id(),
        None
    );

    let _removed = world.remove_unit(aircraft_id);
    assert!(
        world
            .get_unit(controller_id)
            .unwrap()
            .production
            .air_traffic_control()
            .unwrap()
            .landing_spots()[0]
            .aircraft_id()
            .is_none()
    );
}

#[test]
fn train_limited_move_air_reserves_a_pad_refills_and_waits_for_launch() {
    let (mut world, mut database, gameplay, controller_id) = fixture(true);
    database.objects[0].train_limits.push(TrainLimit {
        target: "strike_squad".to_owned(),
        limit_type: Some(TrainLimitType::Squad),
        count: Some(8),
        bucket: None,
    });
    database.squads.push(ProtoSquad {
        name: "strike_squad".to_owned(),
        ..ProtoSquad::default()
    });
    let squad_id = world.create_squad_at(1, Vec3::new(80.0, 0.0, 80.0));
    let aircraft_id = world.create_unit_at(1, Vec3::new(80.0, 10.0, 80.0));
    {
        let squad = world.get_squad_mut(squad_id).unwrap();
        squad.proto_squad_name = "strike_squad".to_owned();
        squad.trained_by = Some(controller_id);
        let aircraft = world.get_unit_mut(aircraft_id).unwrap();
        aircraft.flying = true;
        aircraft.configure_flight_controller(FlightControllerKind::MoveAir, 10.0);
        aircraft.ammunition.configure(100.0, 0.0, true);
        aircraft.ammunition.set_current(5.0);
    }
    assert!(world.attach_unit_to_squad(aircraft_id, squad_id));

    world.update_air_traffic_controls(&database, &gameplay);
    let first = world
        .get_unit(controller_id)
        .unwrap()
        .production
        .air_traffic_control()
        .unwrap()
        .landing_spots()[0];
    assert_eq!(first.aircraft_id(), Some(aircraft_id));
    assert_eq!(
        world.get_unit(aircraft_id).unwrap().move_air_base_id(),
        Some(controller_id)
    );

    world.update_move_air(0.05);
    let aircraft = world.get_unit(aircraft_id).unwrap();
    assert!(aircraft.is_move_air_parked());
    assert_eq!(aircraft.base.position, first.position());
    assert!((aircraft.base.forward - first.forward()).length() < 1.0e-6);
    assert!((aircraft.ammunition.current() - 100.0).abs() <= f32::EPSILON);
    assert_eq!(first.aircraft_id(), Some(aircraft_id));

    let aircraft = world.get_unit_mut(aircraft_id).unwrap();
    let mut state = aircraft.move_air_state().unwrap();
    state.lifecycle.set_launch_requested(true);
    aircraft.set_move_air_state(state);
    world.update_move_air(0.05);
    assert!(!world.get_unit(aircraft_id).unwrap().is_move_air_parked());
    assert_eq!(
        world
            .get_unit(controller_id)
            .unwrap()
            .production
            .air_traffic_control()
            .unwrap()
            .landing_spots()[0]
            .aircraft_id(),
        Some(aircraft_id),
        "retail releases the pad only when MoveAir disconnects"
    );
}

#[test]
fn controller_denies_a_ninth_aircraft_and_reuses_released_pad() {
    let (mut world, database, gameplay, controller_id) = fixture(false);
    world.update_air_traffic_controls(&database, &gameplay);
    let aircraft = (0..9).map(|_| world.create_unit(1)).collect::<Vec<_>>();
    for (index, aircraft_id) in aircraft.iter().copied().take(8).enumerate() {
        let spot = world
            .request_air_traffic_landing_spot(controller_id, aircraft_id)
            .unwrap();
        assert_eq!(
            spot,
            world
                .get_unit(controller_id)
                .unwrap()
                .production
                .air_traffic_control()
                .unwrap()
                .landing_spots()[index]
        );
    }
    assert!(
        world
            .request_air_traffic_landing_spot(controller_id, aircraft[8])
            .is_none()
    );
    world.release_air_traffic_assignment(aircraft[0]);
    assert_eq!(
        world
            .request_air_traffic_landing_spot(controller_id, aircraft[8])
            .unwrap()
            .aircraft_id(),
        Some(aircraft[8])
    );
}

fn fixture(unsc: bool) -> (World, Database, GameplayCatalog, EntityId) {
    let civilization = if unsc { "UNSC" } else { "Covenant" };
    let database = Database {
        civs: vec![Civ {
            name: civilization.to_owned(),
            ..Civ::default()
        }],
        objects: vec![ProtoObject {
            name: "air_pad".to_owned(),
            tactics: Some("air_pad.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [(
            "air_pad".to_owned(),
            TacticData {
                actions: vec![Action {
                    name: "Controller".to_owned(),
                    action_type: Some("AirTrafficControl".to_owned()),
                    ..Action::default()
                }],
                tactic: Some(TacticRules {
                    persistent_actions: vec!["Controller".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    );
    let mut world = World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().civ_id = 0;
    let controller_id = world.create_building_at(1, Vec3::new(10.0, 2.0, 20.0));
    let controller = world.get_unit_mut(controller_id).unwrap();
    controller.proto_object_name = "air_pad".to_owned();
    controller.base.set_forward(Vec3::Z);
    (world, database, gameplay, controller_id)
}
