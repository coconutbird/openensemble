use super::*;
use pipeline::database::hw1::{ProtoObject, Squad, Tech};

#[test]
fn scn_player_lists_seed_distinct_authoritative_forbid_state() {
    let document = Document::from_xml(
        r#"<Scenario>
            <Players>
                <Player Name="One">
                    <ForbidObjects><Object>object_a</Object></ForbidObjects>
                    <ForbidSquads><Squad>squad_a</Squad></ForbidSquads>
                    <ForbidTechs><Tech>technology_a</Tech></ForbidTechs>
                </Player>
                <Player Name="Two">
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
    let mut world = World::new();
    world.init_players(2);

    apply_scenario_forbids(&mut world, &database, &document);

    let first = world.get_player(1).unwrap();
    assert!(first.is_object_forbidden(&database, 101));
    assert!(first.is_squad_forbidden(&database, 201));
    assert!(first.is_technology_forbidden(&database, 0));
    let second = world.get_player(2).unwrap();
    assert!(!second.is_object_forbidden(&database, 101));
    assert!(!second.is_squad_forbidden(&database, 201));
    assert!(!second.is_technology_forbidden(&database, 0));
}
