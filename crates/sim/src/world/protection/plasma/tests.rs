use super::*;
use crate::entities::ShieldCoverage;
use pipeline::database::hw1::tactics::{
    Action, ActionDuration, ProtoObjectRef, TacticData, TacticRules,
};
use pipeline::database::hw1::{Database, GameData, ProtoObject};

#[test]
fn generator_creates_authoritative_proxy_and_rebuilds_after_destruction() {
    let (gameplay, mut world, base_id, anchor_id, _) = shield_world(1);

    world.update_entities_with_gameplay(0.05, &gameplay);

    let shield_squad_id = world
        .get_base(base_id)
        .and_then(crate::entities::Base::plasma_shield_squad)
        .expect("created shield squad");
    let shield_unit_id = world.get_squad(shield_squad_id).unwrap().unit_ids[0];
    let protected_squad_id = world.get_unit(anchor_id).unwrap().squad_id.unwrap();
    assert_eq!(
        world.get_squad(protected_squad_id).unwrap().damage_proxy(),
        Some(shield_squad_id)
    );
    let shield = world.get_unit(shield_unit_id).unwrap();
    assert_eq!(shield.proto_object_name, "main_shield");
    assert_eq!(shield.shields.coverage, ShieldCoverage::Full);
    assert!(nearly_equal(shield.shields.maximum, 100.0));
    assert!(shield.shields.current > 0.0 && shield.shields.current < 100.0);

    let anchor_hp = world.get_unit(anchor_id).unwrap().hitpoints;
    assert!(world.damage_unit(anchor_id, 1.0));
    assert!(nearly_equal(
        world.get_unit(anchor_id).unwrap().hitpoints,
        anchor_hp
    ));
    assert!(world.damage_unit(shield_unit_id, 1_000.0));
    world.update_entities_with_gameplay(0.05, &gameplay);

    let base = world.get_base(base_id).unwrap();
    assert_eq!(base.plasma_shield_squad(), None);
    assert!(nearly_equal(base.plasma_shield_rebuild_remaining(), 30.0));
    assert_eq!(
        world.get_squad(protected_squad_id).unwrap().damage_proxy(),
        None
    );

    world.update_entities_with_gameplay(29.95, &gameplay);
    assert_eq!(world.get_base(base_id).unwrap().plasma_shield_squad(), None);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(
        world
            .get_base(base_id)
            .unwrap()
            .plasma_shield_squad()
            .is_some()
    );
}

#[test]
fn multiple_generators_scale_damage_and_transfer_primary() {
    let (gameplay, mut world, base_id, _, generators) = shield_world(2);
    world.update_plasma_shields(0.05, &gameplay);

    let base = world.get_base(base_id).unwrap();
    assert_eq!(base.primary_plasma_shield_generator(), Some(generators[0]));
    let shield_squad_id = base.plasma_shield_squad().unwrap();
    let shield_unit_id = world.get_squad(shield_squad_id).unwrap().unit_ids[0];
    assert!(nearly_equal(
        world
            .get_unit(shield_unit_id)
            .unwrap()
            .damage_taken_multiplier,
        0.5
    ));

    assert!(world.remove_unit(generators[0]).is_some());
    world.update_plasma_shields(0.05, &gameplay);

    assert_eq!(
        world
            .get_base(base_id)
            .unwrap()
            .primary_plasma_shield_generator(),
        Some(generators[1])
    );
    assert!(nearly_equal(
        world
            .get_unit(shield_unit_id)
            .unwrap()
            .damage_taken_multiplier,
        1.0
    ));
}

#[test]
fn active_attack_and_authored_wait_hold_the_rebuild_timer() {
    let (gameplay, mut world, base_id, anchor_id, _) = shield_world(1);
    world.update_plasma_shields(0.05, &gameplay);
    let shield_squad_id = world
        .get_base(base_id)
        .unwrap()
        .plasma_shield_squad()
        .unwrap();
    let shield_unit_id = world.get_squad(shield_squad_id).unwrap().unit_ids[0];
    world.get_unit_mut(shield_unit_id).unwrap().kill();
    world.update_plasma_shields(0.05, &gameplay);

    let attacker_id = world.create_unit(2);
    assert!(
        world
            .get_unit_mut(attacker_id)
            .unwrap()
            .attack(anchor_id, 0.0, None)
    );
    world.update_plasma_shields(3.0, &gameplay);
    assert!(nearly_equal(
        world
            .get_base(base_id)
            .unwrap()
            .plasma_shield_rebuild_remaining(),
        30.0
    ));

    world
        .get_unit_mut(attacker_id)
        .unwrap()
        .clear_attack_order();
    world.update_plasma_shields(4.0, &gameplay);
    assert!(nearly_equal(
        world
            .get_base(base_id)
            .unwrap()
            .plasma_shield_rebuild_remaining(),
        30.0
    ));
    world.update_plasma_shields(1.0, &gameplay);
    assert!(nearly_equal(
        world
            .get_base(base_id)
            .unwrap()
            .plasma_shield_rebuild_remaining(),
        30.0
    ));
    world.update_plasma_shields(1.0, &gameplay);
    assert!(nearly_equal(
        world
            .get_base(base_id)
            .unwrap()
            .plasma_shield_rebuild_remaining(),
        29.0
    ));
}

#[test]
fn socket_building_subshield_relays_damage_into_main_shield() {
    let (gameplay, mut world, base_id, _, _) = shield_world(1);
    let socket_id = world.create_building(1);
    let building_id = world.create_building_at(1, Vec3::new(12.0, 0.0, 4.0));
    {
        let building = world.get_unit_mut(building_id).unwrap();
        building.proto_object_name = "protected_building".to_owned();
        building.set_max_hitpoints(200.0);
    }
    assert!(world.connect_socket_plug(socket_id, building_id));
    assert!(world.add_building_to_base(base_id, building_id));

    world.update_plasma_shields(0.05, &gameplay);

    let base = world.get_base(base_id).unwrap();
    let main_squad_id = base.plasma_shield_squad().unwrap();
    let main_unit_id = world.get_squad(main_squad_id).unwrap().unit_ids[0];
    let sub_squad_id = base.plasma_subshield_squad(building_id).unwrap();
    let sub_unit_id = world.get_squad(sub_squad_id).unwrap().unit_ids[0];
    let protected_squad_id = world.get_unit(building_id).unwrap().squad_id.unwrap();
    assert_eq!(
        world.get_squad(protected_squad_id).unwrap().damage_proxy(),
        Some(sub_squad_id)
    );
    assert_eq!(
        world.get_squad(sub_squad_id).unwrap().damage_proxy(),
        Some(main_squad_id)
    );
    assert_eq!(
        world.get_unit(sub_unit_id).unwrap().proto_object_name,
        "sub_shield"
    );

    world
        .get_unit_mut(main_unit_id)
        .unwrap()
        .shields
        .set_current(80.0);
    world.update_plasma_shields(0.05, &gameplay);
    assert!(nearly_equal(
        world.get_unit(sub_unit_id).unwrap().shields.current,
        80.0
    ));
    let building_hp = world.get_unit(building_id).unwrap().hitpoints;
    assert!(world.damage_unit(building_id, 10.0));
    assert!(nearly_equal(
        world.get_unit(building_id).unwrap().hitpoints,
        building_hp
    ));
    assert!(nearly_equal(
        world.get_unit(main_unit_id).unwrap().shields.current,
        70.0
    ));
    world.update_plasma_shields(0.05, &gameplay);
    assert!(nearly_equal(
        world.get_unit(sub_unit_id).unwrap().shields.current,
        70.0
    ));

    world.get_unit_mut(main_unit_id).unwrap().kill();
    world.update_plasma_shields(0.05, &gameplay);
    assert!(world.get_squad(sub_squad_id).is_none());
    assert_eq!(
        world.get_squad(protected_squad_id).unwrap().damage_proxy(),
        None
    );
}

fn shield_world(
    generator_count: usize,
) -> (GameplayCatalog, World, BaseId, EntityId, Vec<EntityId>) {
    let (database, tactics) = shield_database();
    let gameplay = GameplayCatalog::from_tactics(&database, [("generator".to_owned(), tactics)]);
    let mut world = World::new();
    world.init_players(2);
    let base_id = world.create_base(1, Vec3::new(4.0, 2.0, 8.0));
    let anchor_id = world.get_base(base_id).unwrap().anchor_building_id;
    world
        .get_unit_mut(anchor_id)
        .unwrap()
        .set_max_hitpoints(400.0);
    let generators = (0..generator_count)
        .map(|_| {
            let id = world.create_building(1);
            world.get_unit_mut(id).unwrap().proto_object_name = "generator".to_owned();
            assert!(world.add_building_to_base(base_id, id));
            id
        })
        .collect();
    (gameplay, world, base_id, anchor_id, generators)
}

fn shield_database() -> (Database, TacticData) {
    let mut database = Database::new();
    database.game_data = Some(GameData {
        shield_regen_time: Some(10.0),
        ..GameData::default()
    });
    database.objects = vec![
        ProtoObject {
            name: "generator".to_owned(),
            tactics: Some("generator.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "main_shield".to_owned(),
            object_class: Some("Building".to_owned()),
            hitpoints: Some(1.0),
            shieldpoints: Some(100.0),
            damage_type: Some("Shielded".to_owned()),
            build_points: Some(30.0),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "protected_building".to_owned(),
            object_class: Some("Building".to_owned()),
            shield_type: Some("sub_shield".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "sub_shield".to_owned(),
            object_class: Some("Building".to_owned()),
            hitpoints: Some(1.0),
            shieldpoints: Some(100.0),
            damage_type: Some("Shielded".to_owned()),
            ..ProtoObject::default()
        },
    ];
    let tactics = TacticData {
        actions: vec![Action {
            name: "Shield".to_owned(),
            action_type: Some("PlasmaShieldGen".to_owned()),
            duration: Some(ActionDuration {
                seconds: 5.0,
                ..ActionDuration::default()
            }),
            proto_object: Some(ProtoObjectRef {
                name: "main_shield".to_owned(),
                ..ProtoObjectRef::default()
            }),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_actions: vec!["Shield".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    (database, tactics)
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= f32::EPSILON * left.abs().max(right.abs()).max(1.0) * 8.0
}
