use glam::Vec3;
use sim::{
    CommandEntry, CommandExecutor, GarrisonError, PopulationCost, QueuedCommand,
    SquadContainmentState, UnitGarrison, WorkCommand, World,
};

const STEP_MS: u32 = 50;
const STEP_SECONDS: f32 = 0.05;

#[test]
fn work_command_drives_authoritative_teleporter_lifecycle() {
    let (mut world, source_squad, source_unit, destination_squad, passenger_squad, passenger_unit) =
        teleporter_world(0xC0C0_A551);
    let command = WorkCommand::garrison_squads(1, vec![passenger_squad], source_squad);
    CommandExecutor::new().execute(&mut world, &command_entry(command));

    assert!(matches!(
        world.get_squad(passenger_squad).unwrap().garrison.state(),
        SquadContainmentState::Garrisoning { target, .. } if target == source_unit
    ));

    advance_until(&mut world, |world| {
        matches!(
            world.get_squad(passenger_squad).unwrap().garrison.state(),
            SquadContainmentState::Garrisoned { .. }
        )
    });
    assert!(world.get_unit(passenger_unit).unwrap().is_garrisoned());
    assert_eq!(
        world
            .get_unit(source_unit)
            .unwrap()
            .garrison
            .contained_unit_ids(),
        &[passenger_unit]
    );

    advance(&mut world);
    let (exit_position, rally_position) =
        match world.get_squad(passenger_squad).unwrap().garrison.state() {
            SquadContainmentState::Ungarrisoning {
                destination,
                exit_position,
                rally_position: Some(rally_position),
                ..
            } => {
                assert_eq!(
                    destination,
                    world.get_squad(destination_squad).unwrap().unit_ids[0]
                );
                (exit_position, rally_position)
            }
            state => panic!("expected teleporter exit phase, got {state:?}"),
        };
    let destination_position = world.get_squad(destination_squad).unwrap().position();
    assert!(exit_position.distance(destination_position) > 2.0);
    assert!((rally_position.distance(exit_position) - 1.0).abs() < 1.0e-4);
    assert!(world.get_unit(passenger_unit).unwrap().is_garrisoned());

    advance(&mut world);
    let passenger = world.get_squad(passenger_squad).unwrap();
    assert_eq!(passenger.garrison.state(), SquadContainmentState::Free);
    assert_eq!(passenger.position(), exit_position);
    assert_eq!(passenger.move_target, Some(rally_position));
    assert!(!world.get_unit(passenger_unit).unwrap().is_garrisoned());
    assert!(world.get_unit(passenger_unit).unwrap().is_operational());
    assert!(
        world
            .get_unit(source_unit)
            .unwrap()
            .garrison
            .contained_unit_ids()
            .is_empty()
    );
}

#[test]
fn removing_container_emergency_unloads_hidden_passenger() {
    let (mut world, source_squad, source_unit, _, passenger_squad, passenger_unit) =
        teleporter_world(7);
    world
        .issue_garrison_order(1, passenger_squad, source_squad, 0.0)
        .expect("accepted garrison order");
    advance_until(&mut world, |world| {
        matches!(
            world.get_squad(passenger_squad).unwrap().garrison.state(),
            SquadContainmentState::Garrisoned { .. }
        )
    });

    world.remove_unit(source_unit).expect("source pad unit");

    assert_eq!(
        world.get_squad(passenger_squad).unwrap().garrison.state(),
        SquadContainmentState::Free
    );
    assert!(!world.get_unit(passenger_unit).unwrap().is_garrisoned());
    assert!(world.get_unit(passenger_unit).unwrap().is_operational());
}

#[test]
fn destroyed_container_releases_passenger_at_its_last_position() {
    let (mut world, source_squad, source_unit, _, passenger_squad, passenger_unit) =
        teleporter_world(8);
    world
        .issue_garrison_order(1, passenger_squad, source_squad, 0.0)
        .expect("accepted garrison order");
    advance_until(&mut world, |world| {
        matches!(
            world.get_squad(passenger_squad).unwrap().garrison.state(),
            SquadContainmentState::Garrisoned { .. }
        )
    });
    let last_position = world.get_unit(source_unit).unwrap().base.position;
    world.get_unit_mut(source_unit).unwrap().kill();

    advance(&mut world);

    assert_eq!(
        world.get_squad(passenger_squad).unwrap().garrison.state(),
        SquadContainmentState::Free
    );
    assert_eq!(
        world.get_squad(passenger_squad).unwrap().position(),
        last_position
    );
    assert!(!world.get_unit(passenger_unit).unwrap().is_garrisoned());
}

#[test]
fn nonteleporter_enforces_object_type_and_population_capacity() {
    let mut world = World::new();
    world.init_players(1);
    let (container_squad, container_unit) = squad_with_unit(&mut world, 0, Vec3::ZERO, true);
    world.get_unit_mut(container_unit).unwrap().garrison =
        UnitGarrison::container(1.0, false, false, vec!["Infantry".to_owned()]);
    let (passenger_squad, passenger_unit) =
        squad_with_unit(&mut world, 1, Vec3::new(2.0, 0.0, 0.0), false);
    world
        .get_squad_mut(passenger_squad)
        .unwrap()
        .population_costs = vec![PopulationCost::new(0, 1.0)];

    assert_eq!(
        world.issue_garrison_order(1, passenger_squad, container_squad, 0.0),
        Err(GarrisonError::CannotContain)
    );
    world.get_unit_mut(passenger_unit).unwrap().object_types = vec!["Infantry".to_owned()];
    world
        .issue_garrison_order(1, passenger_squad, container_squad, 0.0)
        .expect("matching passenger");
    advance_until(&mut world, |world| {
        matches!(
            world.get_squad(passenger_squad).unwrap().garrison.state(),
            SquadContainmentState::Garrisoned { .. }
        )
    });

    let (second_squad, second_unit) =
        squad_with_unit(&mut world, 1, Vec3::new(2.0, 0.0, 0.0), false);
    world.get_squad_mut(second_squad).unwrap().population_costs = vec![PopulationCost::new(0, 1.0)];
    world.get_unit_mut(second_unit).unwrap().object_types = vec!["Infantry".to_owned()];
    assert_eq!(
        world.issue_garrison_order(1, second_squad, container_squad, 0.0),
        Err(GarrisonError::CannotContain)
    );
}

#[test]
fn teleporter_state_and_rng_are_deterministic() {
    let first = completed_teleporter_checksum(123_456);
    let second = completed_teleporter_checksum(123_456);
    assert_eq!(first, second);
}

fn completed_teleporter_checksum(seed: u64) -> u32 {
    let (mut world, source_squad, _, _, passenger_squad, _) = teleporter_world(seed);
    world
        .issue_garrison_order(1, passenger_squad, source_squad, 0.0)
        .expect("accepted garrison order");
    for _ in 0..40 {
        advance(&mut world);
    }
    world.checksum_with_rng()
}

fn teleporter_world(
    seed: u64,
) -> (
    World,
    sim::EntityId,
    sim::EntityId,
    sim::EntityId,
    sim::EntityId,
    sim::EntityId,
) {
    let mut world = World::with_seed(seed);
    world.init_players(1);
    let (source_squad, source_unit) =
        squad_with_unit(&mut world, 0, Vec3::new(15.0, 0.0, 0.0), true);
    let (destination_squad, destination_unit) =
        squad_with_unit(&mut world, 0, Vec3::new(100.0, 0.0, 80.0), true);
    let (passenger_squad, passenger_unit) = squad_with_unit(&mut world, 1, Vec3::ZERO, false);
    {
        let source = world.get_unit_mut(source_unit).unwrap();
        source.garrison = UnitGarrison::container(4.0, true, true, Vec::new());
        source.obstruction_half_extents = Vec3::splat(2.0);
    }
    world
        .get_unit_mut(destination_unit)
        .unwrap()
        .obstruction_half_extents = Vec3::splat(2.0);
    {
        let passenger = world.get_unit_mut(passenger_unit).unwrap();
        passenger.obstruction_half_extents = Vec3::splat(0.5);
        passenger.speed = 20.0;
    }
    {
        let passenger = world.get_squad_mut(passenger_squad).unwrap();
        passenger.speed = 20.0;
        passenger.population_costs = vec![PopulationCost::new(0, 1.0)];
    }
    world
        .get_squad_mut(source_squad)
        .unwrap()
        .set_teleporter_destination(destination_squad);
    (
        world,
        source_squad,
        source_unit,
        destination_squad,
        passenger_squad,
        passenger_unit,
    )
}

fn squad_with_unit(
    world: &mut World,
    player_id: u8,
    position: Vec3,
    building: bool,
) -> (sim::EntityId, sim::EntityId) {
    let squad_id = world.create_squad_at(player_id, position);
    let unit_id = if building {
        world.create_building_at(player_id, position)
    } else {
        world.create_unit_at(player_id, position)
    };
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

fn advance(world: &mut World) {
    world.advance_time(STEP_MS);
    world.update_entities(STEP_SECONDS);
}

fn advance_until(world: &mut World, condition: impl Fn(&World) -> bool) {
    for _ in 0..100 {
        advance(world);
        if condition(world) {
            return;
        }
    }
    panic!("condition was not reached within 100 simulation ticks");
}

fn command_entry(command: WorkCommand) -> CommandEntry {
    CommandEntry {
        command: QueuedCommand::Work(command),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    }
}
