use super::*;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData};
use pipeline::database::hw1::{Database, GameData, ProtoObject};

#[test]
fn finite_target_clamps_income_completes_and_dies_at_zero() {
    let (mut world, gameplay, squad_id, unit_ids, target_id) = gather_world(1, 6.0, -1, false);

    assert!(world.issue_gather_order(1, squad_id, target_id, &gameplay));
    world.update_entities_with_gameplay(1.0, &gameplay);

    assert_close(world.get_player(1).unwrap().get_resource(0), 5.0);
    assert_close(world.get_unit(target_id).unwrap().resource_amount(), 1.0);
    assert_eq!(
        world.get_squad(squad_id).unwrap().gather_phase(),
        GatherPhase::Working
    );
    assert!(world.get_unit(unit_ids[0]).unwrap().is_gathering());

    world.update_entities_with_gameplay(0.25, &gameplay);

    assert_close(world.get_player(1).unwrap().get_resource(0), 6.0);
    assert!(world.get_unit(target_id).is_none());
    assert_eq!(
        world.get_squad(squad_id).unwrap().gather_phase(),
        GatherPhase::Done
    );
    assert_eq!(
        world.get_unit(unit_ids[0]).unwrap().gather_phase(),
        GatherPhase::Done
    );
}

#[test]
fn gatherer_limit_reserves_the_first_unit_in_entity_order() {
    let (mut world, gameplay, squad_id, unit_ids, target_id) = gather_world(2, 20.0, 1, false);

    assert!(world.issue_gather_order(1, squad_id, target_id, &gameplay));
    world.update_entities_with_gameplay(1.0, &gameplay);

    assert_close(world.get_player(1).unwrap().get_resource(0), 5.0);
    assert_close(world.get_unit(target_id).unwrap().resource_amount(), 15.0);
    assert_eq!(world.unit_resource_gatherer_count(target_id), 1);
    assert!(world.get_unit(unit_ids[0]).unwrap().is_gathering());
    assert_eq!(
        world.get_unit(unit_ids[1]).unwrap().gather_phase(),
        GatherPhase::Moving
    );
}

#[test]
fn team_share_divides_each_gather_credit_between_playing_allies() {
    let (mut world, gameplay, squad_id, _, target_id) = gather_world(1, 20.0, -1, true);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 3;
    world.get_player_mut(2).unwrap().team_id = 3;

    assert!(world.issue_gather_order(1, squad_id, target_id, &gameplay));
    world.update_entities_with_gameplay(0.4, &gameplay);

    assert_close(world.get_player(1).unwrap().get_resource(0), 1.0);
    assert_close(world.get_player(2).unwrap().get_resource(0), 1.0);
    assert_close(world.get_unit(target_id).unwrap().resource_amount(), 18.0);
}

#[test]
fn distant_squad_approaches_without_duplicating_income() {
    let (mut world, gameplay, squad_id, unit_ids, target_id) = gather_world(1, 20.0, -1, false);
    world.get_unit_mut(target_id).unwrap().base.position = Vec3::X * 20.0;

    assert!(world.issue_gather_order(1, squad_id, target_id, &gameplay));
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert_close(world.get_player(1).unwrap().get_resource(0), 0.0);
    assert_eq!(
        world.get_squad(squad_id).unwrap().gather_phase(),
        GatherPhase::Moving
    );
    assert_eq!(
        world.get_unit(unit_ids[0]).unwrap().gather_phase(),
        GatherPhase::Moving
    );
    assert!(world.get_squad(squad_id).unwrap().base.position.x > 0.0);
}

#[test]
fn resource_payload_and_connected_order_change_the_world_checksum() {
    let (first, _, _, _, _) = gather_world(1, 20.0, -1, false);
    let (mut second, gameplay, squad_id, _, target_id) = gather_world(1, 20.0, -1, false);
    assert_eq!(first.checksum(), second.checksum());

    assert!(second.issue_gather_order(1, squad_id, target_id, &gameplay));

    assert_ne!(first.checksum(), second.checksum());
}

fn gather_world(
    unit_count: usize,
    target_amount: f32,
    gatherer_limit: i32,
    team_share: bool,
) -> (World, GameplayCatalog, EntityId, Vec<EntityId>, EntityId) {
    let (database, tactic) = gather_database(team_share);
    let gameplay = GameplayCatalog::from_tactics(&database, [("worker".to_owned(), tactic)]);
    let mut world = World::new();
    world.init_players(2);
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let mut unit_ids = Vec::new();
    for _ in 0..unit_count {
        let unit_id = world.create_unit_at(1, Vec3::ZERO);
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = "worker".to_owned();
        unit.obstruction_half_extents = Vec3::ONE;
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        unit_ids.push(unit_id);
    }
    let target_id = world.create_unit_at(GAIA_PLAYER, Vec3::X * 2.0);
    let target = world.get_unit_mut(target_id).unwrap();
    target.obstruction_half_extents = Vec3::ONE;
    target.resource_node.configure(
        Some("Supplies".to_owned()),
        Some(target_amount),
        false,
        true,
        Some(gatherer_limit),
    );
    (world, gameplay, squad_id, unit_ids, target_id)
}

fn gather_database(team_share: bool) -> (Database, TacticData) {
    let database = Database {
        objects: vec![ProtoObject {
            name: "worker".to_owned(),
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    ..ResourceDef::default()
                }],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let tactic = TacticData {
        actions: vec![Action {
            name: "GatherSupplies".to_owned(),
            action_type: Some("Gather".to_owned()),
            resource: Some("Supplies".to_owned()),
            work_rate: Some(5.0),
            work_range: Some(0.1),
            team_share: Some(team_share),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    (database, tactic)
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.0e-5,
        "{actual} != {expected}"
    );
}
