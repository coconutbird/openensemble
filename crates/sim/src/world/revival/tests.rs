use super::*;
use crate::entity::Entity;
use pipeline::database::hw1::gamedata::{CodeObjectType, CodeObjectTypesWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData};
use pipeline::database::hw1::{Database, GameData, ProtoObject};

#[test]
fn revive_action_hibernates_at_zero_then_returns_to_working() {
    let gameplay = revival_catalog();
    let mut world = test_world();
    let squad_id = world.create_squad_at(1, glam::Vec3::ZERO);
    let unit_id = world.create_unit_at(1, glam::Vec3::ZERO);
    world.get_unit_mut(unit_id).unwrap().proto_object_name = "test_reviver".to_owned();
    world.get_unit_mut(unit_id).unwrap().set_max_hitpoints(10.0);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    world.configure_unit_revivals(&gameplay);
    let initial_checksum = world.checksum();

    assert!(world.damage_unit_with_gameplay(unit_id, 10.0, &gameplay));
    let unit = world.get_unit(unit_id).unwrap();
    assert!(unit.is_alive());
    assert!(unit.is_hibernating());
    assert!(unit.hitpoints.abs() < f32::EPSILON);
    assert!(world.is_squad_hibernating(squad_id));
    assert!(!world.is_entity_trigger_alive(unit_id));
    assert!(!world.is_entity_trigger_alive(squad_id));
    assert_ne!(world.checksum(), initial_checksum);

    let hibernating_checksum = world.checksum();
    world.update_entities_with_gameplay(0.1, &gameplay);
    assert!(!world.get_unit(unit_id).unwrap().is_hibernating());
    assert!(world.is_entity_trigger_alive(unit_id));
    assert_ne!(world.checksum(), hibernating_checksum);

    world.update_entities_with_gameplay(0.2, &gameplay);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_unit(unit_id).unwrap().hitpoints > 0.0);
}

#[test]
fn override_revive_requires_a_later_damage_event_at_zero_hitpoints() {
    let gameplay = revival_catalog();
    let mut world = test_world();
    let unit_id = world.create_unit_at(1, glam::Vec3::ZERO);
    {
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = "test_reviver".to_owned();
        unit.set_max_hitpoints(10.0);
    }

    assert!(world.damage_unit_with_gameplay_override(unit_id, 10.0, &gameplay, true));
    let unit = world.get_unit(unit_id).unwrap();
    assert!(unit.is_alive());
    assert!(unit.is_hibernating());
    assert!(unit.hitpoints.abs() < f32::EPSILON);

    let hibernating_checksum = world.checksum();
    assert!(world.damage_unit_with_gameplay_override(unit_id, 1.0, &gameplay, true));
    assert!(world.get_unit(unit_id).unwrap().is_alive());
    assert_ne!(world.checksum(), hibernating_checksum);

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_unit(unit_id).is_none());
}

#[test]
fn hero_remains_live_and_downed_until_a_nearby_allied_squad_is_present() {
    let gameplay = revival_catalog();
    let mut world = test_world();
    let hero_squad = world.create_squad_at(1, glam::Vec3::ZERO);
    let hero_id = world.create_unit_at(1, glam::Vec3::ZERO);
    {
        let hero = world.get_unit_mut(hero_id).unwrap();
        hero.proto_object_name = "test_hero".to_owned();
        hero.set_max_hitpoints(100.0);
    }
    assert!(world.attach_unit_to_squad(hero_id, hero_squad));
    world.configure_unit_revivals(&gameplay);

    assert!(world.kill_squad(hero_squad, false));
    let hero = world.get_unit(hero_id).unwrap();
    assert!(hero.is_alive());
    assert!(hero.is_down());
    assert!((hero.hitpoints - 1.0).abs() < f32::EPSILON);
    assert!(world.get_squad(hero_squad).unwrap().is_alive());
    assert!(world.is_squad_down(hero_squad));
    assert!(!world.is_entity_trigger_alive(hero_id));
    assert!(!world.is_entity_trigger_alive(hero_squad));

    world.update_entities_with_gameplay(1.0, &gameplay);
    assert!(world.get_unit(hero_id).unwrap().is_down());
    assert!((world.get_unit(hero_id).unwrap().hitpoints - 100.0).abs() < f32::EPSILON);

    let ally_squad = world.create_squad_at(2, glam::Vec3::X * 5.0);
    let ally_id = world.create_unit_at(2, glam::Vec3::X * 5.0);
    assert!(world.attach_unit_to_squad(ally_id, ally_squad));
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert!(!world.get_unit(hero_id).unwrap().is_down());
    assert!(world.is_entity_trigger_alive(hero_id));
    assert!(world.is_entity_trigger_alive(hero_squad));

    assert!(world.kill_squad(hero_squad, false));
    assert!(world.get_unit(hero_id).unwrap().is_down());
    assert!(world.kill_squad(hero_squad, true));
    assert!(world.get_unit(hero_id).is_none());
    assert!(world.get_squad(hero_squad).is_none());
}

fn test_world() -> World {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 1;
    world.configure_standard_team_relations();
    world
}

fn revival_catalog() -> GameplayCatalog {
    let mut database = Database::new();
    database.game_data = Some(GameData {
        code_object_types: Some(CodeObjectTypesWrapper {
            entries: vec![CodeObjectType {
                object_type: "HeroDeath".to_owned(),
                value: "_HeroDeath".to_owned(),
            }],
        }),
        hero_hp_regen_time: Some(1.0),
        hero_revival_distance: Some(15.0),
        hero_percent_hp_revival_threshhold: Some(0.5),
        ..GameData::default()
    });
    database.objects.extend([
        ProtoObject {
            name: "test_hero".to_owned(),
            tactics: Some("test_hero.tactics".to_owned()),
            object_types: vec!["_HeroDeath".to_owned()],
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "test_reviver".to_owned(),
            tactics: Some("test_reviver.tactics".to_owned()),
            ..ProtoObject::default()
        },
    ]);
    GameplayCatalog::from_tactics(
        &database,
        [
            ("test_hero".to_owned(), TacticData::default()),
            (
                "test_reviver".to_owned(),
                TacticData {
                    actions: vec![Action {
                        action_type: Some("Revive".to_owned()),
                        revive_delay: Some(0.2),
                        hibernate_revive_delay: Some(0.1),
                        revive_rate: Some(10.0),
                        ..Action::default()
                    }],
                    ..TacticData::default()
                },
            ),
        ],
    )
}
