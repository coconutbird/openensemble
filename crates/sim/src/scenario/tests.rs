use super::*;

const SAMPLE_SCENARIO: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Scenario>
    <Positions>
        <Position Number="1" Position="100.0,0.0,100.0" Forward="0.0,0.0,1.0" />
        <Position Number="2" Position="200.0,0.0,200.0" Forward="0.0,0.0,-1.0" />
    </Positions>
    <Players>
        <Player Name="Player1" Civ="UNSC" Leader1="Cutter" Team="1" Color="0" />
        <Player Name="Player2" Civ="Covenant" Leader1="Arbiter" Team="2" Color="1" />
    </Players>
    <Objects>
        <Object IsSquad="true" Player="1" ID="0" Position="100.0,0.0,100.0" Forward="0.0,0.0,1.0">
            unsc_inf_marine_01
        </Object>
        <Object IsSquad="true" Player="1" ID="1" Position="110.0,0.0,100.0">
            unsc_inf_marine_01
        </Object>
        <Object IsSquad="true" Player="2" ID="2" Position="200.0,0.0,200.0">
            cov_inf_grunt_01
        </Object>
    </Objects>
</Scenario>"#;

#[test]
fn test_load_into_world() {
    let scenario = ScenarioData::from_xml_str(SAMPLE_SCENARIO).unwrap();
    let db = Database::new();
    let loaded = load_scenario_into_world(&scenario, &db);

    assert_eq!(loaded.world.player_count(), 3);
    let p1 = loaded.world.get_player(1).unwrap();
    assert_eq!(p1.name, "Player1");
    assert_eq!(p1.team_id, 1);
    assert_eq!(p1.civ_id, -1);
    let p2 = loaded.world.get_player(2).unwrap();
    assert_eq!(p2.name, "Player2");
    assert_eq!(p2.team_id, 2);

    let squad_count = scenario.objects.as_ref().map_or(0, |objects| {
        objects
            .entries
            .iter()
            .filter(|entry| entry.is_squad)
            .count()
    });
    assert_eq!(loaded.world.squads.len(), squad_count);
    let entity_id = loaded.get_entity_id(0).unwrap();
    assert!((loaded.world.get_squad(entity_id).unwrap().position().x - 100.0).abs() < 0.01);
}

#[test]
fn loads_squad_members_buildings_and_base_anchors() {
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

    let scenario = ScenarioData::from_xml_str(
        r#"<Scenario>
            <Players><Player Name="P1" Team="1" /></Players>
            <Objects>
                <Object IsSquad="true" Player="1" ID="10">marine_squad</Object>
                <Object Player="1" ID="20" Position="5,0,7">unsc_base</Object>
            </Objects>
        </Scenario>"#,
    )
    .unwrap();
    let mut db = Database::new();
    db.objects.push(ProtoObject {
        name: "marine".to_owned(),
        dbid: Some(101),
        object_class: Some("Unit".to_owned()),
        hitpoints: Some(75.0),
        ..ProtoObject::default()
    });
    db.objects.push(ProtoObject {
        name: "unsc_base".to_owned(),
        dbid: Some(202),
        object_class: Some("Building".to_owned()),
        flags: vec!["KBCreatesBase".to_owned()],
        hitpoints: Some(1_000.0),
        ..ProtoObject::default()
    });
    db.squads.push(ProtoSquad {
        name: "marine_squad".to_owned(),
        dbid: Some(303),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: "marine".to_owned(),
                count: 2,
                role: None,
            }],
        }),
        ..ProtoSquad::default()
    });

    let loaded = load_scenario_into_world(&scenario, &db);
    let squad_id = loaded.get_entity_id(10).unwrap();
    let building_squad_id = loaded.get_entity_id(20).unwrap();
    let squad = loaded.world.get_squad(squad_id).unwrap();
    let building_squad = loaded.world.get_squad(building_squad_id).unwrap();
    let building = loaded
        .world
        .get_building(building_squad.unit_ids[0])
        .unwrap();

    assert_eq!(squad.proto_squad_id, 303);
    assert_eq!(squad.unit_ids.len(), 2);
    assert_eq!(building_squad.proto_squad_name, "unsc_base");
    assert_eq!(building_squad.unit_ids.len(), 1);
    assert_eq!(building.proto_object_id, 202);
    assert!((building.hitpoints - 1_000.0).abs() < f32::EPSILON);
    assert_eq!(loaded.world.bases().count(), 1);
    assert!(building.base_id.is_some());
}

#[test]
fn scenario_visual_variation_reaches_every_squad_member_and_checksum() {
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

    let database = Database {
        objects: vec![ProtoObject {
            name: "marine".to_owned(),
            object_class: Some("Unit".to_owned()),
            ..ProtoObject::default()
        }],
        squads: vec![ProtoSquad {
            name: "marine_squad".to_owned(),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "marine".to_owned(),
                    count: 2,
                    role: None,
                }],
            }),
            ..ProtoSquad::default()
        }],
        ..Database::default()
    };
    let load = |variation| {
        let scenario = ScenarioData::from_xml_str(&format!(
            r#"<Scenario><Players><Player Name="P1" /></Players><Objects>
                <Object IsSquad="true" Player="1" ID="10" VisualVariationIndex="{variation}">
                    marine_squad
                </Object>
            </Objects></Scenario>"#
        ))
        .expect("valid scenario");
        load_scenario_into_world(&scenario, &database)
    };

    let first = load(1);
    let squad = first
        .world
        .get_squad(first.get_entity_id(10).unwrap())
        .unwrap();
    assert_eq!(squad.unit_ids.len(), 2);
    assert!(squad.unit_ids.iter().all(|&unit_id| {
        first
            .world
            .get_unit(unit_id)
            .unwrap()
            .object_state
            .visual_variation_index()
            == Some(1)
    }));

    let second = load(2);
    assert_ne!(first.world.checksum(), second.world.checksum());
}

#[test]
fn loads_implicit_class_zero_visual_objects_and_preserves_scenario_mapping() {
    let scenario = ScenarioData::from_xml_str(
        r#"<Scenario>
            <Players><Player Name="P1" Team="1" /></Players>
            <Objects>
                <Object Player="1" ID="1958" Position="5,2,7" Forward="0,0,-1">
                    env_harvest_forebridge_01
                </Object>
            </Objects>
        </Scenario>"#,
    )
    .unwrap();
    let database = Database {
        objects: vec![ProtoObject {
            name: "env_harvest_forebridge_01".to_owned(),
            dbid: Some(2308),
            visual: Some("environment\\harvest\\forebridge_01\\forebridge_01.vis".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };

    let loaded = load_scenario_into_world(&scenario, &database);
    let entity_id = loaded.get_entity_id(1958).unwrap();
    let object = loaded.world.get_object(entity_id).unwrap();

    assert_eq!(entity_id.class(), Some(crate::EntityClass::Object));
    assert!(object.is_visual());
    assert_eq!(object.proto_object_id, 2308);
    assert_eq!(object.proto_object_name, "env_harvest_forebridge_01");
    assert_eq!(object.base.position, Vec3::new(7.0, 2.0, 5.0));
    assert_eq!(object.base.forward, Vec3::NEG_X);
}

#[test]
fn lobby_leader_selection_configures_database_population_slots() {
    use pipeline::database::hw1::gamedata::PopsWrapper;
    use pipeline::database::hw1::leaders::PopEntry;
    use pipeline::database::hw1::{Civ, GameData, Leader, Tech};

    let database = Database {
        civs: vec![Civ {
            name: "UNSC".to_owned(),
            civ_tech: Some("UnscBootstrap".to_owned()),
            ..Civ::default()
        }],
        leaders: vec![Leader {
            name: "Cutter".to_owned(),
            civ: Some("UNSC".to_owned()),
            tech: Some("CutterBootstrap".to_owned()),
            pops: vec![PopEntry {
                pop_type: "Unit".to_owned(),
                count: 30.0,
                max: Some(99.0),
            }],
            ..Leader::default()
        }],
        game_data: Some(GameData {
            pops: Some(PopsWrapper {
                entries: vec!["Unit".to_owned(), "Spartan".to_owned()],
            }),
            ..GameData::default()
        }),
        techs: vec![
            Tech {
                name: "UnscBootstrap".to_owned(),
                ..Tech::default()
            },
            Tech {
                name: "CutterBootstrap".to_owned(),
                ..Tech::default()
            },
        ],
        ..Database::default()
    };
    let mut world = World::new();
    world.init_players(1);
    let base_id = world.create_building(1);
    world
        .get_building_mut(base_id)
        .unwrap()
        .population_cap_additions
        .push(crate::player::PopulationCost::new(0, 5.0));

    assert!(configure_player_leader(&mut world, &database, 1, 0));
    let player = world.get_player(1).unwrap();
    assert_eq!(player.leader_id, 0);
    assert_eq!(player.civ_id, 0);
    assert_eq!(
        player
            .technologies
            .active_technologies()
            .collect::<Vec<_>>(),
        ["UnscBootstrap", "CutterBootstrap"]
    );
    assert_eq!(player.population.len(), 2);
    assert!((player.population[0].cap - 35.0).abs() <= f32::EPSILON);
    assert!((player.population[0].max - 99.0).abs() <= f32::EPSILON);
    assert!((player.population[1].cap - 0.0).abs() <= f32::EPSILON);
}

#[test]
fn scenario_configures_player_rates_from_layered_game_data() {
    use pipeline::database::hw1::GameData;
    use pipeline::database::hw1::gamedata::{RatesWrapper, ResourcesWrapper};

    let scenario = ScenarioData::from_xml_str(SAMPLE_SCENARIO).unwrap();
    let database = Database {
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper::default()),
            rates: Some(RatesWrapper {
                entries: vec![
                    "Supplies".to_owned(),
                    "Power".to_owned(),
                    "LeaderPowerCharge".to_owned(),
                ],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    };

    let loaded = load_scenario_into_world(&scenario, &database);
    for player in loaded.world.players() {
        assert_eq!(player.rate_slot_count(), 3);
        assert!(player.get_rate(0).abs() <= f32::EPSILON);
    }
}
