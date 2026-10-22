use pipeline::database::hw1::{Database, ProtoObject, Squad, Tech};

#[test]
fn scn_player_lists_seed_distinct_authoritative_forbid_state() {
    let scenario = pipeline::hw1::scenario::ScenarioData::from_xml_str(
        r#"<Scenario>
            <Players>
                <Player Name="One" Team="1">
                    <ForbidObjects><Object>object_a</Object></ForbidObjects>
                    <ForbidSquads><Squad>squad_a</Squad></ForbidSquads>
                    <ForbidTechs><Tech>technology_a</Tech></ForbidTechs>
                </Player>
                <Player Name="Two" Team="2">
                    <ForbidObjects><Object>missing</Object></ForbidObjects>
                </Player>
            </Players>
        </Scenario>"#,
    )
    .unwrap();
    let database = Database {
        objects: vec![ProtoObject {
            name: "object_a".to_owned(),
            dbid: Some(101),
            ..ProtoObject::default()
        }],
        squads: vec![Squad {
            name: "squad_a".to_owned(),
            dbid: Some(201),
            ..Squad::default()
        }],
        techs: vec![Tech {
            name: "technology_a".to_owned(),
            ..Tech::default()
        }],
        ..Database::default()
    };
    let world = crate::scenario::load_test_scenario(&scenario, &database).world;

    // Proto IDs are table indices; the authored `dbid` values are not IDs.
    let first = world.get_player(1).unwrap();
    assert!(first.is_object_forbidden(&database, 0));
    assert!(first.is_squad_forbidden(&database, 0));
    assert!(first.is_technology_forbidden(&database, 0));
    let second = world.get_player(2).unwrap();
    assert!(!second.is_object_forbidden(&database, 0));
    assert!(!second.is_squad_forbidden(&database, 0));
    assert!(!second.is_technology_forbidden(&database, 0));
}
