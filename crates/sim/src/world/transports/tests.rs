use super::*;

fn squad_with_unit(world: &mut World, player_id: u8, position: Vec3) -> (EntityId, EntityId) {
    let squad_id = world.create_squad_at(player_id, position);
    let unit_id = world.create_unit_at(player_id, position);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

#[test]
fn transport_hides_passenger_then_releases_and_self_destructs() {
    let mut world = World::new();
    world.init_players(1);
    let (passenger, passenger_unit) = squad_with_unit(&mut world, 1, Vec3::ZERO);
    let (transport, transport_unit) = squad_with_unit(&mut world, 1, Vec3::ZERO);
    world.get_squad_mut(transport).unwrap().speed = 20.0;
    let plan = SquadTransportPlan {
        passenger_squad_id: passenger,
        start_position: Vec3::new(-10.0, 10.0, 0.0),
        dropoff_position: Vec3::ZERO,
        incoming_target: Vec3::new(0.0, 5.0, 0.0),
        outgoing_target: Vec3::new(10.0, 10.0, 0.0),
        rally_point: Some(Vec3::new(5.0, 0.0, 0.0)),
        attack_move: true,
        facing: Some(Vec3::X),
    };

    assert!(world.start_transport_fly_in(transport, plan));
    assert!(world.get_unit(passenger_unit).unwrap().is_garrisoned());
    assert_eq!(
        world.get_unit(transport_unit).unwrap().base.forward,
        Vec3::X
    );
    assert_eq!(
        world
            .get_squad(transport)
            .unwrap()
            .transport_fly_in()
            .unwrap()
            .phase(),
        TransportFlyInPhase::Incoming
    );
    assert_eq!(
        world
            .get_unit(transport_unit)
            .unwrap()
            .garrison
            .contained_unit_ids(),
        &[passenger_unit]
    );

    for _ in 0..40 {
        world.update_entities(0.05);
    }
    assert!(world.get_squad(transport).is_none());
    assert!(!world.get_unit(passenger_unit).unwrap().is_garrisoned());
    let passenger = world.get_squad(passenger).unwrap();
    assert!(
        passenger.is_executing_attack_move()
            || passenger.base.position == plan.rally_point.unwrap()
    );
}

#[test]
fn invalid_cross_owner_plan_is_atomic() {
    let mut world = World::new();
    world.init_players(2);
    let (passenger, passenger_unit) = squad_with_unit(&mut world, 1, Vec3::ZERO);
    let (transport, _) = squad_with_unit(&mut world, 2, Vec3::ZERO);
    let plan = SquadTransportPlan {
        passenger_squad_id: passenger,
        start_position: Vec3::ZERO,
        dropoff_position: Vec3::ZERO,
        incoming_target: Vec3::X,
        outgoing_target: Vec3::X * 2.0,
        rally_point: None,
        attack_move: false,
        facing: None,
    };
    assert!(!world.start_transport_fly_in(transport, plan));
    assert!(!world.get_unit(passenger_unit).unwrap().is_garrisoned());
    assert!(
        world
            .get_squad(transport)
            .unwrap()
            .transport_fly_in()
            .is_none()
    );
}
