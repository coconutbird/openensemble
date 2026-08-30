use glam::Vec3;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use sim::entities::squads::marine::{MARINE_PROTO_SQUAD_ID, MARINE_SQUAD_NAME, MarineSquadSpec};
use sim::entities::units::marine::{
    MARINE_HITPOINTS, MARINE_PHYSICS_REPLACEMENT_INFO, MARINE_PROTO_OBJECT_ID, MARINE_UNIT_NAME,
    MarineUnitSpec,
};
use sim::{
    Command, CommandEntry, CommandExecutor, GroundMovePhase, OrderType, QueuedCommand,
    ScenarioData, SquadArchetype, SquadFormation, SquadState, UnitArchetype, WorkCommand, World,
    load_scenario_into_world,
};

const MARINE_SCENARIO: &str = r#"<Scenario>
    <Players><Player Name="Cutter" Team="1" /></Players>
    <Objects>
        <Object IsSquad="true" Player="1" ID="10" Position="0,0,0" Forward="0,0,1">
            unsc_inf_marine_01
        </Object>
    </Objects>
</Scenario>"#;

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < f32::EPSILON);
}

fn marine_database() -> Database {
    let unit_spec = MarineUnitSpec::default();
    let squad_spec = MarineSquadSpec::default();
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: MARINE_UNIT_NAME.to_owned(),
        dbid: Some(MARINE_PROTO_OBJECT_ID),
        object_class: Some("Squad".to_owned()),
        select_type: Some("Unit".to_owned()),
        movement_type: Some("Land".to_owned()),
        hitpoints: Some(MARINE_HITPOINTS),
        velocity: Some(unit_spec.max_speed),
        acceleration: Some(unit_spec.acceleration),
        turn_rate: Some(unit_spec.turn_rate_degrees),
        obstruction_radius_x: Some(unit_spec.half_extents.x),
        obstruction_radius_y: Some(unit_spec.half_extents.y),
        obstruction_radius_z: Some(unit_spec.half_extents.z),
        physics_replacement_info: Some(MARINE_PHYSICS_REPLACEMENT_INFO.to_owned()),
        flags: vec!["DontRotateObstruction".to_owned()],
        ..ProtoObject::default()
    });
    database.squads.push(ProtoSquad {
        name: MARINE_SQUAD_NAME.to_owned(),
        dbid: Some(MARINE_PROTO_SQUAD_ID),
        formation_type: Some("Flock".to_owned()),
        leash_distance: Some(squad_spec.leash_distance),
        aggro_distance: Some(squad_spec.aggro_distance),
        leash_deadzone: Some(squad_spec.leash_deadzone),
        leash_recall_delay: Some(squad_spec.leash_recall_delay_ms),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: MARINE_UNIT_NAME.to_owned(),
                count: i32::try_from(squad_spec.member_count).unwrap(),
                role: Some("normal".to_owned()),
            }],
        }),
        ..ProtoSquad::default()
    });
    database
}

fn issue_move(world: &mut World, squad_id: sim::EntityId, target: Vec3) {
    let entry = CommandEntry {
        command: QueuedCommand::Work(WorkCommand {
            base: Command {
                id: OrderType::Move as i32,
                player_id: 1,
                recipients: vec![squad_id],
                ..Command::default()
            },
            terrain_point: Some(target),
            ..WorkCommand::default()
        }),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };
    CommandExecutor::new().execute(world, &entry);
}

#[test]
fn scenario_builds_stock_marine_unit_and_squad_profiles() {
    let scenario = ScenarioData::from_xml_str(MARINE_SCENARIO).unwrap();
    let loaded = load_scenario_into_world(&scenario, &marine_database());
    let squad_id = loaded.get_entity_id(10).unwrap();
    let squad = loaded.world.get_squad(squad_id).unwrap();
    let unit_spec = MarineUnitSpec::default();
    let squad_spec = MarineSquadSpec::default();

    assert_eq!(squad.archetype, SquadArchetype::Marine);
    assert_eq!(squad.formation, SquadFormation::Flock);
    assert_eq!(squad.proto_squad_id, MARINE_PROTO_SQUAD_ID);
    assert_eq!(squad.proto_squad_name, MARINE_SQUAD_NAME);
    assert_eq!(squad.unit_ids.len(), squad_spec.member_count);
    assert_close(squad.speed, unit_spec.max_speed);
    assert_close(squad.acceleration, unit_spec.acceleration);
    assert_close(squad.turn_rate_degrees, unit_spec.turn_rate_degrees);
    assert_close(squad.aggro_distance, squad_spec.aggro_distance);
    assert_close(squad.leash_distance, squad_spec.leash_distance);

    for (slot, &unit_id) in squad.unit_ids.iter().enumerate() {
        let unit = loaded.world.get_unit(unit_id).unwrap();
        assert_eq!(unit.archetype, UnitArchetype::Marine);
        assert_eq!(unit.proto_object_id, MARINE_PROTO_OBJECT_ID);
        assert_eq!(unit.proto_object_name, MARINE_UNIT_NAME);
        assert_close(unit.hitpoints, MARINE_HITPOINTS);
        assert_close(unit.speed, unit_spec.max_speed);
        assert_close(unit.acceleration, unit_spec.acceleration);
        assert_close(unit.turn_rate_degrees, unit_spec.turn_rate_degrees);
        assert_eq!(unit.obstruction_half_extents, unit_spec.half_extents);
        assert!(unit.physics.is_none());
        assert_eq!(
            unit.formation_offset,
            squad_spec.initial_formation_offset(slot).unwrap()
        );
    }
}

#[test]
fn move_command_advances_one_squad_and_four_formation_members_deterministically() {
    let scenario = ScenarioData::from_xml_str(MARINE_SCENARIO).unwrap();
    let database = marine_database();
    let mut first = load_scenario_into_world(&scenario, &database);
    let mut second = load_scenario_into_world(&scenario, &database);
    let first_squad_id = first.get_entity_id(10).unwrap();
    let second_squad_id = second.get_entity_id(10).unwrap();
    let target = Vec3::new(30.0, 0.0, 0.0);
    issue_move(&mut first.world, first_squad_id, target);
    issue_move(&mut second.world, second_squad_id, target);

    first.world.update_entities(0.05);
    second.world.update_entities(0.05);
    let squad = first.world.get_squad(first_squad_id).unwrap();
    assert!(squad.base.velocity.x > 0.0);
    assert!(squad.base.velocity.length() <= 1.3 + f32::EPSILON);
    let initial_offsets = squad
        .unit_ids
        .iter()
        .map(|&id| first.world.get_unit(id).unwrap().formation_offset)
        .collect::<Vec<_>>();
    for &unit_id in &squad.unit_ids {
        let unit = first.world.get_unit(unit_id).unwrap();
        assert_eq!(unit.ground_move_phase(), GroundMovePhase::Working);
        assert!(unit.ground_move_target().is_some());
        assert!(unit.has_active_move_action());
        assert!(unit.base.velocity.length() > 0.0);
    }

    for _ in 0..159 {
        first.world.update_entities(0.05);
        second.world.update_entities(0.05);
    }

    let squad = first.world.get_squad(first_squad_id).unwrap();
    assert_eq!(squad.state, SquadState::Idle);
    assert_eq!(squad.position(), target);
    for (slot, &unit_id) in squad.unit_ids.iter().enumerate() {
        let unit = first.world.get_unit(unit_id).unwrap();
        assert_eq!(unit.formation_offset, initial_offsets[slot]);
        assert_ne!(unit.base.position, squad.position());
        assert_eq!(unit.base.forward, squad.base.forward);
        assert_eq!(unit.ground_move_phase(), GroundMovePhase::Inactive);
    }
    assert_eq!(first.world.checksum(), second.world.checksum());
}
