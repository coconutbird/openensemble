use super::*;
use pipeline::database::hw1::powers::PowerAttributes;

fn database() -> Database {
    Database {
        powers: vec![
            Power {
                name: "First".to_owned(),
                attributes: Some(PowerAttributes {
                    icon_locations: vec![3],
                    ..PowerAttributes::default()
                }),
                ..Power::default()
            },
            Power {
                name: "Second".to_owned(),
                attributes: Some(PowerAttributes {
                    infinite_uses: Some(true),
                    ..PowerAttributes::default()
                }),
                ..Power::default()
            },
        ],
        ..Database::default()
    }
}

fn grant(proto_power_id: i32, squad_id: EntityId, icon_location: i32) -> PowerGrant {
    PowerGrant {
        proto_power_id,
        squad_id,
        uses: 1,
        icon_location,
        ignore_cost: false,
        ignore_tech_prerequisites: false,
        ignore_population: false,
    }
}

#[test]
fn database_names_and_implicit_icon_locations_drive_player_state() {
    let database = database();
    assert_eq!(power_prototype_id(&database, "second"), Some(1));
    assert_eq!(power_prototype_id(&database, "0"), Some(0));
    assert_eq!(power_prototype_id(&database, "missing"), None);

    let mut world = World::new();
    world.init_players(1);
    assert!(world.grant_player_power(1, &database, grant(0, EntityId::INVALID, -1)));
    assert!(world.grant_player_power(1, &database, grant(1, EntityId::INVALID, 3)));

    let player = world.get_player(1).unwrap();
    assert!(player.power_entry(0).is_none());
    assert!(player.power_entry(1).unwrap().has_available_uses());
    assert!(!player.power_entry(1).unwrap().ignores_cost());
}

#[test]
fn removing_a_squad_revokes_its_owner_power_source() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad(1);
    assert!(world.grant_player_power(1, &database, grant(0, squad_id, -1)));
    assert_eq!(
        world.get_player(1).unwrap().power_entry(0).unwrap().items()[0].squad_id(),
        squad_id
    );

    world.remove_squad(squad_id).unwrap();

    assert!(
        world
            .get_player(1)
            .unwrap()
            .power_entry(0)
            .unwrap()
            .items()
            .is_empty()
    );
}

#[test]
fn invalid_squad_binding_falls_back_to_global_entry_before_grant_or_revoke() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let stale_squad = EntityId::new(crate::EntityClass::Squad, 9);
    assert!(world.grant_player_power(1, &database, grant(0, stale_squad, -1)));
    assert!(
        world.get_player(1).unwrap().power_entry(0).unwrap().items()[0]
            .squad_id()
            .is_invalid()
    );

    assert!(world.revoke_player_power(1, &database, 0, stale_squad));
    assert!(world.get_player(1).unwrap().power_entry(0).is_none());
}
