use super::*;
use crate::entities::JoinKind;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use pipeline::database::hw1::objects::{ProtoObject, TrainLimit, TrainLimitType};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};

#[test]
fn auto_join_spawn_maintains_count_and_allows_multiple_followers() {
    let (database, gameplay) = fixture(None);
    let mut world = World::new();
    world.init_players(2);
    let owner_squad = spawn_owner(&mut world, &database);
    let owner_id = world.get_squad(owner_squad).unwrap().unit_ids[0];

    advance(&mut world, &database, &gameplay, 4);

    let initial_followers = followers(&world, owner_id);
    assert_eq!(initial_followers.len(), 2);
    for follower_id in &initial_followers {
        let follower = world.get_squad(*follower_id).unwrap();
        assert_eq!(follower.join_target(), Some(owner_squad));
        assert_eq!(follower.join_kind(), Some(JoinKind::Follow));
        assert!(follower.join_allows_multiple());
    }
    advance(&mut world, &database, &gameplay, 4);
    assert_eq!(followers(&world, owner_id).len(), 2);

    assert!(world.kill_squad(initial_followers[0], false));
    advance(&mut world, &database, &gameplay, 4);
    assert_eq!(followers(&world, owner_id).len(), 2);
}

#[test]
fn authored_train_limit_blocks_additional_persistent_spawns() {
    let limit = TrainLimit {
        target: "child_squad".to_owned(),
        limit_type: Some(TrainLimitType::Squad),
        count: Some(1),
        bucket: Some(3),
    };
    let (database, gameplay) = fixture(Some(limit));
    let mut world = World::new();
    world.init_players(2);
    let owner_squad = spawn_owner(&mut world, &database);
    let owner_id = world.get_squad(owner_squad).unwrap().unit_ids[0];

    advance(&mut world, &database, &gameplay, 10);

    let followers = followers(&world, owner_id);
    assert_eq!(followers.len(), 1);
    assert_eq!(
        world.get_squad(followers[0]).unwrap().train_limit_bucket,
        Some(3)
    );
}

fn advance(world: &mut World, database: &Database, gameplay: &GameplayCatalog, ticks: usize) {
    for tick in 0..ticks {
        world.game_time_ms = u32::try_from(tick + 1).unwrap() * 50;
        world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
    }
}

fn followers(world: &World, owner_id: EntityId) -> Vec<EntityId> {
    world
        .squads
        .iter()
        .filter_map(|(id, squad)| {
            (squad.is_alive() && squad.trained_by == Some(owner_id)).then_some(id)
        })
        .collect()
}

fn spawn_owner(world: &mut World, database: &Database) -> EntityId {
    let prototype = squad_prototype_id(database, "owner_squad").unwrap();
    spawn_squad_at(world, database, 1, prototype, Vec3::ZERO, Vec3::Z).unwrap()
}

fn fixture(limit: Option<TrainLimit>) -> (Database, GameplayCatalog) {
    let owner = ProtoObject {
        name: "owner".to_owned(),
        object_class: Some("Unit".to_owned()),
        tactics: Some("owner.tactics".to_owned()),
        obstruction_radius_x: Some(1.0),
        obstruction_radius_z: Some(1.0),
        train_limits: limit.into_iter().collect(),
        ..ProtoObject::default()
    };
    let child = ProtoObject {
        name: "child".to_owned(),
        object_class: Some("Unit".to_owned()),
        tactics: Some("child.tactics".to_owned()),
        obstruction_radius_x: Some(0.5),
        obstruction_radius_z: Some(0.5),
        ..ProtoObject::default()
    };
    let database = Database {
        objects: vec![owner, child],
        squads: vec![
            proto_squad("owner_squad", "owner"),
            proto_squad("child_squad", "child"),
        ],
        ..Database::default()
    };
    let spawn = Action {
        name: "SpawnFollowers".to_owned(),
        action_type: Some("SpawnSquad".to_owned()),
        squad_type: Some("child_squad".to_owned()),
        stationary: Some(true),
        auto_join: Some(true),
        count: Some(2),
        ..Action::default()
    };
    let owner_tactics = TacticData {
        actions: vec![spawn],
        tactic: Some(TacticRules {
            persistent_actions: vec!["SpawnFollowers".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let child_tactics = TacticData {
        actions: vec![Action {
            name: "Join".to_owned(),
            action_type: Some("Join".to_owned()),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [
            ("owner".to_owned(), owner_tactics),
            ("child".to_owned(), child_tactics),
        ],
    );
    (database, gameplay)
}

fn proto_squad(name: &str, member: &str) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        build_points: Some(0.0),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: member.to_owned(),
                count: 1,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
}
