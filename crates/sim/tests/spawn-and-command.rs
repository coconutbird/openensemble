use glam::Vec3;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use sim::entities::squads::marine::{MARINE_PROTO_SQUAD_ID, MARINE_SQUAD_NAME, MarineSquadSpec};
use sim::entities::units::marine::{
    MARINE_HITPOINTS, MARINE_PROTO_OBJECT_ID, MARINE_UNIT_NAME, MarineUnitSpec,
};
use sim::{
    GameCommand, MS_PER_TICK, Simulation, SquadArchetype, TeamRelation, WorkCommand, World,
    spawn_squad_from_base_by_name,
};

fn marine_database() -> Database {
    let unit_spec = MarineUnitSpec::default();
    let squad_spec = MarineSquadSpec::default();
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: MARINE_UNIT_NAME.to_owned(),
        dbid: Some(MARINE_PROTO_OBJECT_ID),
        object_class: Some("Unit".to_owned()),
        select_type: Some("Unit".to_owned()),
        hitpoints: Some(MARINE_HITPOINTS),
        velocity: Some(unit_spec.max_speed),
        acceleration: Some(unit_spec.acceleration),
        turn_rate: Some(unit_spec.turn_rate_degrees),
        obstruction_radius_x: Some(unit_spec.half_extents.x),
        obstruction_radius_y: Some(unit_spec.half_extents.y),
        obstruction_radius_z: Some(unit_spec.half_extents.z),
        ..ProtoObject::default()
    });
    database.squads.push(ProtoSquad {
        name: MARINE_SQUAD_NAME.to_owned(),
        dbid: Some(MARINE_PROTO_SQUAD_ID),
        formation_type: Some("Flock".to_owned()),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: MARINE_UNIT_NAME.to_owned(),
                count: i32::try_from(squad_spec.member_count).expect("Marine count fits i32"),
                role: Some("normal".to_owned()),
            }],
        }),
        ..ProtoSquad::default()
    });
    database
}

fn team_world() -> World {
    let mut world = World::new();
    world.init_players(3);
    world.get_player_mut(1).expect("Player 1").team_id = 1;
    world.get_player_mut(2).expect("Player 2").team_id = 1;
    world.get_player_mut(3).expect("Player 3").team_id = 2;
    world.configure_standard_team_relations();
    world
}

#[test]
fn teams_distinguish_allies_enemies_and_gaia() {
    let mut world = team_world();

    assert_eq!(world.player_relation(1, 2), Some(TeamRelation::Ally));
    assert_eq!(world.player_relation(1, 3), Some(TeamRelation::Enemy));
    assert_eq!(world.player_relation(1, 0), Some(TeamRelation::Neutral));
    assert!(world.players_are_allied(1, 2));
    assert!(world.players_are_enemies(1, 3));

    let checksum = world.checksum();
    assert!(world.set_mutual_team_relation(1, 2, TeamRelation::Neutral));
    assert_eq!(world.player_relation(1, 3), Some(TeamRelation::Neutral));
    assert_ne!(world.checksum(), checksum);
}

#[test]
fn base_spawns_owned_marine_squad_outside_anchor() {
    let database = marine_database();
    let mut world = team_world();
    let base_id = world.create_base(1, Vec3::new(10.0, 2.0, 20.0));
    let anchor_id = world.get_base(base_id).expect("base").anchor_building_id;
    let anchor = world.get_building_mut(anchor_id).expect("anchor");
    anchor.base.set_forward(Vec3::X);
    anchor.obstruction_half_extents = Vec3::new(12.0, 4.0, 10.0);

    let squad_id = spawn_squad_from_base_by_name(&mut world, &database, base_id, MARINE_SQUAD_NAME)
        .expect("base should spawn Marines");
    let squad = world.get_squad(squad_id).expect("spawned squad");

    assert_eq!(squad.base.player_id, 1);
    assert_eq!(squad.archetype, SquadArchetype::Marine);
    assert_eq!(squad.proto_squad_id, MARINE_PROTO_SQUAD_ID);
    assert_eq!(
        squad.unit_ids.len(),
        MarineSquadSpec::default().member_count
    );
    assert!(
        squad
            .base
            .position
            .abs_diff_eq(Vec3::new(30.0, 2.0, 20.0), 1.0e-6)
    );
    assert!(squad.base.forward.abs_diff_eq(Vec3::X, 1.0e-6));
    assert!(squad.unit_ids.iter().all(|&unit_id| {
        world
            .get_unit(unit_id)
            .is_some_and(|unit| unit.base.player_id == 1)
    }));
}

#[test]
fn queued_create_then_owned_move_runs_end_to_end() {
    let database = marine_database();
    let mut world = team_world();
    let mut simulation = Simulation::new();
    simulation.start();
    simulation.command_queue.enqueue_game(
        GameCommand::create_squads(1, MARINE_PROTO_SQUAD_ID, 1, Vec3::new(5.0, 0.0, 7.0)),
        MS_PER_TICK,
        1,
    );
    simulation.tick_with_world_and_database(&mut world, &database);

    let squad_id = world.squads.ids().next().expect("created squad");
    let start = world.get_squad(squad_id).expect("squad").base.position;
    let target = Vec3::new(20.0, 0.0, 7.0);

    simulation.command_queue.enqueue_work(
        WorkCommand::move_squads(3, vec![squad_id], target),
        simulation.game_time_ms + MS_PER_TICK,
        3,
    );
    simulation.tick_with_world_and_database(&mut world, &database);
    assert_eq!(world.get_squad(squad_id).expect("squad").move_target, None);
    assert_eq!(
        world.get_squad(squad_id).expect("squad").base.position,
        start
    );

    simulation.command_queue.enqueue_work(
        WorkCommand::move_squads(1, vec![squad_id], target),
        simulation.game_time_ms + MS_PER_TICK,
        1,
    );
    simulation.tick_with_world_and_database(&mut world, &database);
    assert_eq!(
        world.get_squad(squad_id).expect("squad").move_target,
        Some(target)
    );
    assert!(world.get_squad(squad_id).expect("squad").base.position.x > start.x);

    for _ in 0..300 {
        simulation.tick_with_world_and_database(&mut world, &database);
    }
    let squad = world.get_squad(squad_id).expect("squad");
    assert_eq!(squad.base.position, target);
    assert!(squad.unit_ids.iter().all(|&unit_id| {
        world
            .get_unit(unit_id)
            .is_some_and(|unit| unit.base.player_id == 1)
    }));
}
