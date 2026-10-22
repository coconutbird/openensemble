use super::*;
use pipeline::database::hw1::{ProtoObject, Squad};

const ALLOWED_OBJECT: i32 = 0;
const FORBIDDEN_OBJECT: i32 = 1;
const ALLOWED_SQUAD: i32 = 0;

fn database() -> Database {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "allowed_object".to_owned(),
        dbid: Some(101),
        ..ProtoObject::default()
    });
    database.objects.push(ProtoObject {
        name: "authored_forbidden_object".to_owned(),
        dbid: Some(102),
        flags: vec!["Forbid".to_owned()],
        ..ProtoObject::default()
    });
    database.squads.push(Squad {
        name: "allowed_squad".to_owned(),
        dbid: Some(201),
        ..Squad::default()
    });
    database.techs.push(Tech {
        name: "authored_forbidden_technology".to_owned(),
        flags: vec!["forbid".to_owned()],
        ..Tech::default()
    });
    database
}

#[test]
fn effective_flags_layer_player_overrides_on_authored_data() {
    let database = database();
    let mut player = Player::new(1);

    assert!(!player.is_object_forbidden(&database, ALLOWED_OBJECT));
    assert!(player.is_object_forbidden(&database, FORBIDDEN_OBJECT));
    assert!(!player.is_squad_forbidden(&database, ALLOWED_SQUAD));
    assert!(player.is_technology_forbidden(&database, 0));

    assert_eq!(
        player.set_object_forbidden(&database, ALLOWED_OBJECT, true),
        Some(true)
    );
    assert_eq!(
        player.set_object_forbidden(&database, FORBIDDEN_OBJECT, false),
        Some(true)
    );
    assert_eq!(
        player.set_squad_forbidden(&database, ALLOWED_SQUAD, true),
        Some(true)
    );
    assert_eq!(
        player.set_technology_forbidden(&database, 0, false),
        Some(true)
    );

    assert!(player.is_object_forbidden(&database, ALLOWED_OBJECT));
    assert!(!player.is_object_forbidden(&database, FORBIDDEN_OBJECT));
    assert!(player.is_squad_forbidden(&database, ALLOWED_SQUAD));
    assert!(!player.is_technology_forbidden(&database, 0));
}

#[test]
fn setting_the_effective_value_is_idempotent_and_invalid_ids_are_ignored() {
    let database = database();
    let mut player = Player::new(1);

    assert_eq!(
        player.set_object_forbidden(&database, ALLOWED_OBJECT, false),
        Some(false)
    );
    // The `dbid` values are save-game keys, not proto IDs.
    assert_eq!(player.set_object_forbidden(&database, 101, true), None);
    assert_eq!(player.set_squad_forbidden(&database, 201, true), None);
    assert_eq!(player.set_technology_forbidden(&database, 999, true), None);
    assert!(!player.is_object_forbidden(&database, 101));
}
