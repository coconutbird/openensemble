//! Checks the stock Warthog from database profiles to movement.

use std::sync::Arc;

use glam::Vec3;
use pipeline::database::hw1::Vector3;
use pipeline::database::hw1::physics::{PhysicsBlueprint, PhysicsInfo, PhysicsVehicleType};
use pipeline::database::hw1::squads::{TurnRadius, UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use pipeline::xsd;
use sim::entities::squads::warthog::{WARTHOG_SQUAD_NAME, WarthogSquadSpec};
use sim::entities::units::warthog::{WARTHOG_HITPOINTS, WARTHOG_UNIT_NAME, WarthogUnitSpec};
use sim::physics::{ObjectPhysicsTemplates, PhysicsBlueprintBody, PhysicsTemplate};
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

/// A scenario with the player only; the test spawns its Warthog after the physics setup.
const PLAYER_SCENARIO: &str = r#"<Scenario>
    <Players><Player Name="Cutter" Team="1" /></Players>
</Scenario>"#;
/// The spawn point of the physics Warthog, on the flat test terrain.
const WARTHOG_START: Vec3 = Vec3::new(20.0, 0.0, 60.0);
/// The `CenterOffset` of the shipped `warthog` physics info.
const WARTHOG_CENTER_OFFSET: Vec3 = Vec3::new(0.0, 2.28, 0.0);

/// A flat terrain at height 0: 32 by 32 height samples, 4 units apart.
fn flat_terrain() -> xsd::Terrain {
    let axis = 4 * xsd::HEIGHT_BLOCK_AXIS;
    xsd::Terrain {
        byte_order: xsd::ByteOrder::Big,
        data_axis: axis,
        data_tile_scale: 4.0,
        height_axis: axis,
        cache_axis: axis,
        height_tile_scale: 4.0,
        heights: vec![0.0; axis * axis],
        tile_types: None,
    }
}

/// The shipped `warthog` physics info and blueprint: vehicle type Warthog, `CenterOffset`
/// (0, 2.28, 0), a 1.5 × 1 × 3 box of mass 150, friction 2, restitution 0.5 and damping
/// 0.1 / 0.01.
fn warthog_template() -> Arc<PhysicsTemplate> {
    let info = PhysicsInfo {
        blueprints: vec!["warthog".to_owned()],
        center_offset: Vector3 {
            x: WARTHOG_CENTER_OFFSET.x,
            y: WARTHOG_CENTER_OFFSET.y,
            z: WARTHOG_CENTER_OFFSET.z,
        },
        vehicle_type: PhysicsVehicleType::Warthog,
        ..PhysicsInfo::default()
    };
    let blueprint = PhysicsBlueprint {
        mass: 150.0,
        friction: 2.0,
        restitution: 0.5,
        linear_damping: 0.1,
        angular_damping: 0.01,
        ..PhysicsBlueprint::default()
    };
    let body = PhysicsBlueprintBody::with_box("warthog", &blueprint, Vec3::new(1.5, 1.0, 3.0));
    Arc::new(PhysicsTemplate::new("warthog", info, vec![body]).expect("one body"))
}

/// A world with the flat terrain body and a Warthog squad whose unit has the `warthog` body.
fn physics_warthog_world(database: &Database) -> (LoadedScenario, sim::EntityId) {
    let scenario = ScenarioData::from_xml_str(PLAYER_SCENARIO).unwrap();
    let mut loaded = load_scenario_into_world(&scenario, database);
    loaded.world.configure_terrain_simulation(flat_terrain());
    loaded.world.set_prototype_physics(
        WARTHOG_UNIT_NAME,
        ObjectPhysicsTemplates {
            info: Some(warthog_template()),
            replacement: None,
        },
    );
    let squad_id = sim::spawn_squad_at(
        &mut loaded.world,
        database,
        1,
        WARTHOG_PROTO_SQUAD_ID,
        WARTHOG_START,
        Vec3::X,
    )
    .expect("the Warthog squad spawns");
    loaded.world.create_pending_physics_objects();
    (loaded, squad_id)
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

/// The stock Warthog as a physics vehicle squad (`BSquad::isSquadAPhysicsVehicle`): the squad
/// moves itself, the `BPhysicsWarthogAction` drives the body after it, and the readback
/// (`BUnitActionPhysics_update`) moves the unit.
#[test]
fn move_command_drives_member_and_squad_end_to_end() {
    let database = warthog_database();
    let (mut first, first_squad_id) = physics_warthog_world(&database);
    let (mut second, second_squad_id) = physics_warthog_world(&database);
    let unit_id = first.world.get_squad(first_squad_id).unwrap().unit_ids[0];
    let unit = first.world.get_unit(unit_id).unwrap();
    assert!(
        unit.physics_object
            .as_ref()
            .is_some_and(sim::physics::UnitPhysics::has_vehicle_action)
    );
    let target = WARTHOG_START + Vec3::new(50.0, 0.0, 0.0);
    issue_move(&mut first.world, first_squad_id, target);
    issue_move(&mut second.world, second_squad_id, target);

    first.world.update_entities(0.05);
    second.world.update_entities(0.05);
    // The squad moves itself; its unit gets no move of its own, only the moving state.
    let squad = first.world.get_squad(first_squad_id).unwrap();
    let unit = first.world.get_unit(unit_id).unwrap();
    assert!(squad.position().x > WARTHOG_START.x);
    assert_eq!(unit.state, sim::UnitState::Moving);
    assert!(unit.move_target.is_none());

    let mut top_speed = 0.0_f32;
    for _ in 0..199 {
        let body_position = first.world.unit_body_position(unit_id).unwrap();
        let body_rotation = first.world.unit_body_rotation(unit_id).unwrap();
        let body_active = first.world.unit_body_is_active(unit_id) == Some(true);
        first.world.update_entities(0.05);
        second.world.update_entities(0.05);
        let unit = first.world.get_unit(unit_id).unwrap();
        if body_active {
            assert_eq!(
                unit.base.position,
                body_position - body_rotation * WARTHOG_CENTER_OFFSET
            );
        }
        top_speed = top_speed.max(unit.base.velocity.length());
    }

    let squad = first.world.get_squad(first_squad_id).unwrap();
    let unit = first.world.get_unit(unit_id).unwrap();
    assert_eq!(squad.state, SquadState::Idle);
    assert_eq!(squad.position(), target);
    let miss = Vec3::new(
        unit.base.position.x - target.x,
        0.0,
        unit.base.position.z - target.z,
    )
    .length();
    assert!(miss < 1.5, "the Warthog stopped {miss} from the goal");
    assert!(top_speed > 10.0, "top speed {top_speed}");
    assert!(
        first
            .world
            .unit_body_linear_velocity(unit_id)
            .unwrap()
            .length()
            < 0.5
    );
    assert_eq!(first.world.checksum(), second.world.checksum());
}
