use super::*;
use pipeline::database::hw1::squads::{MergedSquads, UnitEntry, UnitsWrapper};

fn database(names: &[&str]) -> Database {
    let mut database = Database::new();
    database.objects = names.iter().map(|name| object(name, None)).collect();
    database
}

fn object(name: &str, class: Option<&str>) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        object_class: class.map(str::to_owned),
        ..ProtoObject::default()
    }
}

fn squad(name: &str, units: &[(&str, i32)]) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        units: Some(UnitsWrapper {
            entries: units
                .iter()
                .map(|(unit, count)| UnitEntry {
                    proto_object: (*unit).to_owned(),
                    count: *count,
                    ..UnitEntry::default()
                })
                .collect(),
        }),
        ..ProtoSquad::default()
    }
}

/// Objects 0..=5 and squads 0..=1. Objects 1, 3 and 5 get implicit squads.
fn implicit_database() -> Database {
    Database {
        objects: vec![
            object("marine", Some("Unit")),
            object("command", Some("Building")),
            object("rock", Some("Object")),
            object("bullet", Some("Projectile")),
            object("hero", Some("squad")),
            object("turret", Some("UNIT")),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, mut object)| {
            if index == 3 {
                object.object_class = Some("Unit".to_owned());
            }
            object
        })
        .collect(),
        squads: vec![
            // Represents object 0: one unit of `marine`.
            squad("MARINE", &[("marine", 1)]),
            // Same name as object 4, one unit after a count-0 entry that does
            // not resolve: it still represents object 4.
            squad("hero", &[("missing", 3), ("hero", 1), ("rock", 0)]),
        ],
        ..Database::default()
    }
}

#[test]
fn name_lookups_keep_their_trim_policy() {
    let database = database(&[" Padded ", "Plain"]);

    assert!(object_by_name_untrimmed(&database, "padded").is_none());
    assert!(object_by_name_untrimmed(&database, " plain ").is_none());
    assert!(object_by_name_untrimmed(&database, " padded ").is_some());

    assert!(object_by_name_trim_query(&database, "padded").is_none());
    assert_eq!(
        object_entry_by_name_trim_query(&database, " plain ").map(|(index, _)| index),
        Some(1)
    );

    assert_eq!(
        object_entry_by_name_trim_both(&database, "PADDED").map(|(index, _)| index),
        Some(0)
    );
    assert!(object_by_name_trim_both(&database, " plain ").is_some());
}

#[test]
fn name_lookups_return_the_first_match() {
    let database = database(&["Dup", "dup"]);

    assert_eq!(
        object_entry_by_name_trim_query(&database, "DUP").map(|(index, _)| index),
        Some(0)
    );
    assert_eq!(
        object_entry_by_name_trim_both(&database, "DUP").map(|(index, _)| index),
        Some(0)
    );
}

#[test]
fn proto_ids_are_load_order_indices_even_when_dbids_collide() {
    // Retail objects.xml shares dbid 3436 between two objects; the engine IDs
    // are indices (`setupProtoObjects`), so both stay distinct.
    let mut database = database(&["cinematic", "cover", "other"]);
    database.objects[0].dbid = Some(3436);
    database.objects[1].dbid = Some(3436);
    database.objects[2].dbid = Some(0);
    database.squads = vec![squad("a", &[]), squad("b", &[])];
    database.squads[0].dbid = Some(1);

    for (index, prototype) in database.objects.iter().enumerate() {
        let id = proto_id(index);
        assert!(std::ptr::eq(
            object_by_id(&database, id).unwrap(),
            prototype
        ));
    }
    assert!(object_by_id(&database, 3436).is_none());
    assert!(object_by_id(&database, -1).is_none());
    assert_eq!(squad_by_id(&database, 1).unwrap().name, "b");
    assert!(squad_by_id(&database, 2).is_none());
    assert_eq!(proto_id(usize::MAX), -1);
}

#[test]
fn implicit_squads_follow_records_in_object_order() {
    let database = implicit_database();

    // Squad (1), Building (2) and Unit (3) classes without a representing
    // squad, in object order: command, bullet (class Unit), turret. The size
    // of `hero` counts only the entries that resolve.
    let ids = proto_squads(&database)
        .map(|(id, entry)| (id, entry.name(), entry.size(&database)))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            (0, "MARINE".to_owned(), 1),
            (1, "hero".to_owned(), 1),
            (2, "command".to_owned(), 1),
            (3, "bullet".to_owned(), 1),
            (4, "turret".to_owned(), 1),
        ]
    );
    assert_eq!(object_squad_id(&database, 0), Some(0));
    assert_eq!(object_squad_id(&database, 1), Some(2));
    assert_eq!(object_squad_id(&database, 2), None);
    assert_eq!(object_squad_id(&database, 4), Some(1));
    assert_eq!(object_squad_id(&database, 5), Some(4));
    assert_eq!(object_squad_id(&database, 6), None);
}

#[test]
fn a_squad_represents_its_object_only_with_one_unit_of_it_first() {
    let mut database = Database {
        objects: vec![object("a", Some("Unit")), object("b", Some("Unit"))],
        squads: vec![squad("a", &[("b", 0), ("a", 1)])],
        ..Database::default()
    };
    // `b` resolves first (count 0 still counts as the first unit).
    assert_eq!(representing_squad(&database, 0), None);

    database.squads[0] = squad("a", &[("a", 1), ("b", 1)]);
    assert_eq!(representing_squad(&database, 0), None);

    database.squads[0] = squad("a", &[("a", 2), ("b", -1)]);
    assert_eq!(representing_squad(&database, 0), Some(0));

    database.squads[0] = squad("A", &[("a", 1), ("missing", 5)]);
    assert_eq!(representing_squad(&database, 0), Some(0));

    // A squad of another name never represents the object.
    database.squads[0] = squad("b", &[("a", 1)]);
    assert_eq!(representing_squad(&database, 0), None);
    assert_eq!(object_squad_id(&database, 0), Some(1));
}

#[test]
fn merged_squads_come_between_records_and_implicit_squads() {
    let mut database = implicit_database();
    database.merged_squads = vec![
        MergedSquads {
            squad: "hero".to_owned(),
            targets: vec!["MARINE".to_owned(), "unknown".to_owned()],
        },
        MergedSquads {
            squad: "unknown".to_owned(),
            targets: vec!["MARINE".to_owned()],
        },
    ];

    assert_eq!(
        merged_proto_squads(&database)
            .map(|(id, joining, target)| (id, merged_squad_name(joining, target)))
            .collect::<Vec<_>>(),
        [(2, "merged_MARINE_hero".to_owned())]
    );
    assert_eq!(squad_id(&database, "merged_marine_hero"), Some(2));
    assert!(matches!(
        proto_squad(&database, 2),
        Some(ProtoSquadEntry::Merged { joining, target })
            if joining.name == "hero" && target.name == "MARINE"
    ));
    assert_eq!(proto_squad(&database, 2).unwrap().size(&database), 2);
    assert_eq!(squad_id(&database, "command"), Some(3));
    assert_eq!(object_squad_id(&database, 5), Some(5));
    assert!(proto_squad(&database, 6).is_none());
}

#[test]
fn squad_names_resolve_records_first_then_implicit_object_names() {
    let mut database = implicit_database();
    // A record that shares the object name but does not represent it keeps the
    // name; the object still gets an implicit squad (first entry wins).
    database.squads.push(squad("turret", &[("turret", 4)]));

    assert_eq!(squad_id(&database, "marine"), Some(0));
    assert_eq!(squad_id(&database, " Turret "), Some(2));
    assert_eq!(object_squad_id(&database, 5), Some(5));
    assert_eq!(squad_id(&database, "rock"), None);
    assert_eq!(squad_id(&database, "missing"), None);

    for name in ["command", "bullet"] {
        let id = squad_id(&database, name).unwrap();
        assert!(matches!(
            proto_squad(&database, id),
            Some(ProtoSquadEntry::Implicit { object, .. }) if object.name == name
        ));
    }
    assert!(proto_squad(&database, -1).is_none());
}

#[test]
fn scenario_names_resolve_like_bscenario_load_object() {
    let database = implicit_database();

    let squad = scenario_proto(&database, "command", Some(true));
    assert!(matches!(
        squad,
        Some(ScenarioProto::Squad(ProtoSquadEntry::Implicit {
            object_index: 1,
            ..
        }))
    ));
    assert!(matches!(
        scenario_proto(&database, "command", Some(false)),
        Some(ScenarioProto::Object(1, _))
    ));
    // Without `IsSquad`, the squad name map wins.
    assert!(matches!(
        scenario_proto(&database, "marine", None),
        Some(ScenarioProto::Squad(ProtoSquadEntry::Squad(record))) if record.name == "MARINE"
    ));
    assert!(matches!(
        scenario_proto(&database, "rock", None),
        Some(ScenarioProto::Object(2, _))
    ));
    assert!(scenario_proto(&database, "rock", Some(true)).is_none());
    assert!(scenario_proto(&database, "missing", None).is_none());
}
