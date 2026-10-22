use super::*;

fn database(names: &[&str]) -> Database {
    let mut database = Database::new();
    database.objects = names
        .iter()
        .map(|name| ProtoObject {
            name: (*name).to_owned(),
            ..ProtoObject::default()
        })
        .collect();
    database
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
fn id_lookup_prefers_explicit_ids_and_falls_back_to_index() {
    let mut database = database(&["first", "second"]);
    database.objects[0].dbid = Some(1);

    assert_eq!(
        object_by_id(&database, 1).map(|o| o.name.as_str()),
        Some("first")
    );
    assert_eq!(object_by_id(&database, 0).map(|o| o.name.as_str()), None);
    assert_eq!(database_id(None, 7), 7);
    assert_eq!(database_id(Some(3), 7), 3);
    assert_eq!(database_id(None, usize::MAX), -1);
}
