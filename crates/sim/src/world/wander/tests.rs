use super::*;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn isolated_wander_uses_retail_circular_rng_and_target_range() {
    let gameplay = wander_catalog(false, Some(20.0));
    let (mut world, squad_id, _unit_id) = wander_world(Vec3::new(10.0, 3.0, 20.0));
    let mut oracle = World::new();
    let expected = oracle.random_circular_position(Vec3::new(10.0, 3.0, 20.0), 20.0, 0.0);

    world.update_entities_with_gameplay(0.05, &gameplay);

    let squad = world.get_squad(squad_id).unwrap();
    assert!(squad.is_wandering());
    assert_eq!(squad.wander_origin(), Some(Vec3::new(10.0, 3.0, 20.0)));
    assert_eq!(squad.wander_target(), Some(expected));
    assert_eq!(
        squad.move_target,
        ranged_destination(Vec3::new(10.0, 3.0, 20.0), expected)
    );
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );
}

#[test]
fn nearby_same_prototype_uses_retail_inverted_separation_without_rng() {
    let gameplay = wander_catalog(false, Some(20.0));
    let (mut world, first, _first_unit) = wander_world(Vec3::ZERO);
    let (second, _second_unit) = add_wander_squad(&mut world, Vec3::X * 4.0);
    let mut oracle = World::new();

    world.update_entities_with_gameplay(0.05, &gameplay);

    assert_eq!(
        world.get_squad(first).unwrap().wander_target(),
        Some(Vec3::X * 20.0)
    );
    assert_eq!(
        world.get_squad(second).unwrap().wander_target(),
        Some(Vec3::X * -16.0)
    );
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );
}

#[test]
fn live_enablement_and_membership_cleanup_control_wander() {
    let gameplay = wander_catalog(true, Some(200.0));
    let (mut world, squad_id, unit_id) = wander_world(Vec3::ZERO);

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(!world.get_squad(squad_id).unwrap().is_wandering());
    world
        .get_unit_mut(unit_id)
        .unwrap()
        .actions
        .set_enabled("WanderAction", true);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_squad(squad_id).unwrap().is_wandering());
    assert!(world.get_squad(squad_id).unwrap().move_target.is_some());

    assert!(world.detach_unit_from_squad(unit_id));
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.wander_origin(), None);
    assert_eq!(squad.move_target, None);
}

fn wander_catalog(starts_disabled: bool, work_range: Option<f32>) -> GameplayCatalog {
    let database = Database {
        objects: vec![ProtoObject {
            name: "spore".to_owned(),
            tactics: Some("spore.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    GameplayCatalog::from_tactics(
        &database,
        [(
            "spore".to_owned(),
            TacticData {
                actions: vec![Action {
                    name: "WanderAction".to_owned(),
                    action_type: Some("Wander".to_owned()),
                    start_disabled: Some(starts_disabled),
                    work_range,
                    ..Action::default()
                }],
                tactic: Some(TacticRules {
                    persistent_squad_actions: vec!["WanderAction".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    )
}

fn wander_world(position: Vec3) -> (World, EntityId, EntityId) {
    let mut world = World::new();
    world.init_players(1);
    let (squad_id, unit_id) = add_wander_squad(&mut world, position);
    (world, squad_id, unit_id)
}

fn add_wander_squad(world: &mut World, position: Vec3) -> (EntityId, EntityId) {
    let squad_id = world.create_squad_at(1, position);
    world.get_squad_mut(squad_id).unwrap().proto_squad_id = 77;
    let unit_id = world.create_unit_at(1, position);
    world.get_unit_mut(unit_id).unwrap().proto_object_name = "spore".to_owned();
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}
