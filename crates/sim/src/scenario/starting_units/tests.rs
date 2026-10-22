use pipeline::database::hw1::gamedata::{CodeProtoObject, CodeProtoObjectsWrapper};
use pipeline::database::hw1::leaders::{StartingSquad, StartingUnit};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::types::Vector3;
use pipeline::database::hw1::{Civ, GameData, ProtoObject, Squad as ProtoSquad};

use super::*;
use crate::entities::units::RallyPoint;
use crate::scenario::{GameSettings, LoadedScenario, PlayerSettings, load_scenario_into_world};

fn proto(name: &str, object_class: &str) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        object_class: Some(object_class.to_owned()),
        hitpoints: Some(100.0),
        ..ProtoObject::default()
    }
}

/// Leader `Cutter` starts with the `socket` unit and the `jeep_squad` squad at offset
/// (60, 0, -20). Gamedata maps `UnitStart`, `RallyStart` and `SkirmishEmptyBaseObject`.
fn database() -> Database {
    let code_proto = |object_type: &str, proto_name: &str| CodeProtoObject {
        object_type: object_type.to_owned(),
        proto_name: proto_name.to_owned(),
    };
    Database {
        civs: vec![
            Civ {
                name: "None".to_owned(),
                ..Civ::default()
            },
            Civ {
                name: "UNSC".to_owned(),
                ..Civ::default()
            },
        ],
        leaders: vec![Leader {
            name: "Cutter".to_owned(),
            civ: Some("UNSC".to_owned()),
            starting_units: vec![StartingUnit {
                proto_object: "socket".to_owned(),
                ..StartingUnit::default()
            }],
            starting_squads: vec![StartingSquad {
                proto_squad: "jeep_squad".to_owned(),
                offset: Some(Vector3 {
                    x: 60.0,
                    y: 0.0,
                    z: -20.0,
                }),
                ..StartingSquad::default()
            }],
            ..Leader::default()
        }],
        objects: vec![
            proto("socket", "Building"),
            proto("empty_socket", "Building"),
            proto("jeep", "Unit"),
            // Retail `sys_unitstart_01` and `sys_rallystart_01` are plain objects.
            proto("unit_start", "Object"),
            proto("rally_start", "Object"),
        ],
        squads: vec![ProtoSquad {
            name: "jeep_squad".to_owned(),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "jeep".to_owned(),
                    count: 1,
                    role: None,
                }],
            }),
            ..ProtoSquad::default()
        }],
        game_data: Some(GameData {
            code_proto_objects: Some(CodeProtoObjectsWrapper {
                entries: vec![
                    code_proto("UnitStart", "unit_start"),
                    code_proto("RallyStart", "rally_start"),
                    code_proto("SkirmishEmptyBaseObject", "empty_socket"),
                ],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

/// Load a scenario with one human slot and the positions and objects of `body`.
fn load(body: &str) -> LoadedScenario {
    let scenario = ScenarioData::from_xml_str(&format!(
        r#"<Scenario>{body}<Players>
            <Player Name="P1" UseStartingUnits="true" Civ="UNSC" Leader1="Cutter" Team="1" />
        </Players></Scenario>"#
    ))
    .unwrap();
    let settings = GameSettings::skirmish(vec![PlayerSettings::default()], 0);
    load_scenario_into_world(&scenario, &database(), &settings).unwrap()
}

/// Owner and position of each live unit of a proto.
fn units(world: &World, proto: &str) -> Vec<(PlayerId, Vec3)> {
    world
        .units
        .iter()
        .filter(|(_, unit)| unit.proto_object_name == proto)
        .map(|(_, unit)| (unit.base.player_id, unit.base.position))
        .collect()
}

#[test]
fn starting_squad_uses_its_offset_in_the_start_frame_and_unused_starts_get_gaia_sockets() {
    // `BScenario_createStartingUnits` @ 0x140384760: the first starting unit takes the start
    // position; the squad takes `start + x * right + y * up + z * forward` with
    // `right = up x forward`. Forward (1, 0, 0) gives right (0, 0, -1), so offset (60, 0, -20)
    // lands at (100, 0, 200) + (-20, 0, -60) = (80, 0, 140), (140, 0, 80) in world axes.
    // The unused second start gets `SkirmishEmptyBaseObject` for Gaia (0x1403855D9).
    let loaded = load(
        r#"<Positions>
            <Position Number="1" Position="100,0,200" Forward="1,0,0" />
            <Position Number="2" Position="400,0,500" Forward="0,0,1" />
        </Positions>"#,
    );
    let world = &loaded.world;

    assert_eq!(units(world, "socket"), [(1, Vec3::new(200.0, 0.0, 100.0))]);
    assert_eq!(units(world, "jeep"), [(1, Vec3::new(140.0, 0.0, 80.0))]);
    assert_eq!(
        units(world, "empty_socket"),
        [(GAIA_PLAYER, Vec3::new(500.0, 0.0, 400.0))]
    );
}

#[test]
fn unit_start_and_rally_start_markers_are_consumed_and_used() {
    // `BScenario_loadObject` @ 0x14037D6A0 keeps `UnitStart` and `RallyStart` objects as
    // markers and creates nothing. The second spawn point is `UnitStartObject1`, and
    // `RallyStartObject` becomes the player rally point (`sub_1402F3470`) because the player
    // has no `Base` building.
    let loaded = load(
        r#"<Positions>
            <Position Number="1" Position="100,0,200" Forward="0,0,1" UnitStartObject1="50"
                RallyStartObject="51" />
        </Positions>
        <Objects>
            <Object Player="0" ID="50" Position="130,0,210">unit_start</Object>
            <Object Player="0" ID="51" Position="150,0,250">rally_start</Object>
        </Objects>"#,
    );
    let world = &loaded.world;

    assert!(units(world, "unit_start").is_empty() && units(world, "rally_start").is_empty());
    assert!(
        world
            .objects
            .iter()
            .all(|(_, object)| !object.proto_object_name.ends_with("_start"))
    );
    assert_eq!(units(world, "socket"), [(1, Vec3::new(200.0, 0.0, 100.0))]);
    assert_eq!(units(world, "jeep"), [(1, Vec3::new(210.0, 0.0, 130.0))]);
    assert_eq!(
        world.player_rally_point(1).map(RallyPoint::position),
        Some(Vec3::new(250.0, 0.0, 150.0))
    );
    // Every start position is used, so no empty base appears.
    assert_eq!(units(world, "empty_socket"), []);
}
