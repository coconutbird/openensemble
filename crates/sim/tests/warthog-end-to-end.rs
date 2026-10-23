//! Checks the stock Warthog from database profiles to movement.

use glam::Vec3;
use pipeline::database::hw1::squads::{TurnRadius, UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use sim::entities::squads::warthog::{WARTHOG_SQUAD_NAME, WarthogSquadSpec};
use sim::entities::units::warthog::{WARTHOG_HITPOINTS, WARTHOG_UNIT_NAME, WarthogUnitSpec};
use sim::{
    Command, CommandEntry, CommandExecutor, CommandPayload, GameSettings, LoadedScenario,
    OrderType, PlayerSettings, ScenarioData, SquadArchetype, SquadState, UnitArchetype,
    WorkCommand, World,
};

/// Proto IDs are load-order indices; the Warthog is first in each table.
const WARTHOG_PROTO_OBJECT_ID: i32 = 0;
const WARTHOG_PROTO_SQUAD_ID: i32 = 0;

const MOVEMENT_SCENARIO: &str = r#"<Scenario>
    <Players><Player Name="Cutter" Team="1" /></Players>
    <Objects>
        <Object IsSquad="true" Player="1" ID="10" Position="0,0,0" Forward="1,0,0">
            unsc_veh_warthog_01
        </Object>
    </Objects>
</Scenario>"#;

/// Load the scenario with one lobby slot for its controllable player.
fn load_scenario_into_world(scenario: &ScenarioData, database: &Database) -> LoadedScenario {
    let settings = GameSettings::skirmish(vec![PlayerSettings::default()], 0);
    sim::load_scenario_into_world(scenario, database, &settings).expect("players set up")
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < f32::EPSILON);
}

fn warthog_database() -> Database {
    let unit_spec = WarthogUnitSpec::default();
    let squad_spec = WarthogSquadSpec::default();
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: WARTHOG_UNIT_NAME.to_owned(),
        object_class: Some("Squad".to_owned()),
        select_type: Some("Unit".to_owned()),
        movement_type: Some("Land".to_owned()),
        hitpoints: Some(WARTHOG_HITPOINTS),
        velocity: Some(unit_spec.max_speed),
        turn_rate: Some(unit_spec.turn_rate_degrees),
        obstruction_radius_x: Some(unit_spec.half_extents.x),
        obstruction_radius_y: Some(unit_spec.half_extents.y),
        obstruction_radius_z: Some(unit_spec.half_extents.z),
        physics_info: Some("warthog".to_owned()),
        flags: vec!["DontRotateObstruction".to_owned()],
        ..ProtoObject::default()
    });
    database.squads.push(ProtoSquad {
        name: WARTHOG_SQUAD_NAME.to_owned(),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: WARTHOG_UNIT_NAME.to_owned(),
                count: 1,
                role: None,
            }],
        }),
        turn_radius: Some(TurnRadius {
            min: Some(squad_spec.min_turn_radius),
            max: Some(squad_spec.max_turn_radius),
            value: squad_spec.turn_radius,
        }),
        ..ProtoSquad::default()
    });
    database
}

fn issue_move(world: &mut World, squad_id: sim::EntityId, target: Vec3) {
    let work_command = WorkCommand {
        base: Command {
            id: OrderType::Move as i32,
            player_id: 1,
            recipients: vec![squad_id],
            ..Command::default()
        },
        terrain_point: Some(target),
        ..WorkCommand::default()
    };
    let entry = CommandEntry {
        command: CommandPayload::Work(work_command),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };
    CommandExecutor::new().execute(world, &entry);
}

#[test]
fn scenario_builds_stock_warthog_unit_and_squad_profiles() {
    let scenario = ScenarioData::from_xml_str(MOVEMENT_SCENARIO).unwrap();
    let loaded = load_scenario_into_world(&scenario, &warthog_database());
    let squad_id = loaded.get_entity_id(10).unwrap();
    let squad = loaded.world.get_squad(squad_id).unwrap();
    let unit = loaded.world.get_unit(squad.unit_ids[0]).unwrap();

    assert_eq!(squad.archetype, SquadArchetype::Warthog);
    assert_eq!(squad.proto_squad_id, WARTHOG_PROTO_SQUAD_ID);
    assert_eq!(squad.proto_squad_name, WARTHOG_SQUAD_NAME);
    assert_close(squad.turn_radius, 0.0);
    assert_close(squad.min_turn_radius, 1.5);
    assert_close(squad.max_turn_radius, 4.0);
    assert_eq!(unit.archetype, UnitArchetype::Warthog);
    assert_eq!(unit.proto_object_id, WARTHOG_PROTO_OBJECT_ID);
    assert_close(unit.hitpoints, WARTHOG_HITPOINTS);
    // The synthetic database has no physics files, so the `warthog` info makes no body.
    assert!(unit.physics_object.is_none());
}

#[test]
fn move_command_drives_member_and_squad_end_to_end() {
    let scenario = ScenarioData::from_xml_str(MOVEMENT_SCENARIO).unwrap();
    let database = warthog_database();
    let mut first = load_scenario_into_world(&scenario, &database);
    let mut second = load_scenario_into_world(&scenario, &database);
    let first_squad_id = first.get_entity_id(10).unwrap();
    let second_squad_id = second.get_entity_id(10).unwrap();
    let target = Vec3::new(50.0, 0.0, 0.0);
    issue_move(&mut first.world, first_squad_id, target);
    issue_move(&mut second.world, second_squad_id, target);

    first.world.update_entities(0.05);
    second.world.update_entities(0.05);
    let unit_id = first.world.get_squad(first_squad_id).unwrap().unit_ids[0];
    let unit = first.world.get_unit(unit_id).unwrap();
    assert!(unit.base.velocity.x > 0.0);
    assert!(unit.base.velocity.length() <= 3.0 + f32::EPSILON);
    assert_eq!(
        first.world.get_squad(first_squad_id).unwrap().position(),
        unit.base.position
    );

    for _ in 0..199 {
        first.world.update_entities(0.05);
        second.world.update_entities(0.05);
    }

    let squad = first.world.get_squad(first_squad_id).unwrap();
    let unit = first.world.get_unit(unit_id).unwrap();
    assert_eq!(squad.state, SquadState::Idle);
    assert_eq!(unit.base.position, target);
    assert_eq!(squad.position(), target);
    assert_eq!(first.world.checksum(), second.world.checksum());
}
