use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::{Database, ProtoObject};
use sim::{
    CommandEntry, CommandExecutor, GameplayCatalog, QueuedCommand, SquadState, WorkCommand, World,
};

fn combat_database() -> Database {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "test_attacker".to_owned(),
        tactics: Some("test_attacker.tactics".to_owned()),
        ..ProtoObject::default()
    });
    database.objects.push(ProtoObject {
        name: "test_target".to_owned(),
        ..ProtoObject::default()
    });
    database
}

fn gameplay(database: &Database) -> GameplayCatalog {
    let tactics = TacticData {
        weapons: vec![Weapon {
            name: "TestRifle".to_owned(),
            damage_per_second: Some(10.0),
            max_range: Some(10.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "TestAttack".to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some("TestRifle".to_owned()),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    GameplayCatalog::from_tactics(database, [("test_attacker".to_owned(), tactics)])
}

fn combat_world() -> (World, sim::EntityId, sim::EntityId, sim::EntityId) {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();

    let attacker_squad = world.create_squad_at(1, Vec3::ZERO);
    let attacker_unit = world.create_unit_at(1, Vec3::ZERO);
    "test_attacker".clone_into(&mut world.get_unit_mut(attacker_unit).unwrap().proto_object_name);
    assert!(world.attach_unit_to_squad(attacker_unit, attacker_squad));

    let target_squad = world.create_squad_at(2, Vec3::new(30.0, 0.0, 0.0));
    let target_unit = world.create_unit_at(2, Vec3::new(30.0, 0.0, 0.0));
    "test_target".clone_into(&mut world.get_unit_mut(target_unit).unwrap().proto_object_name);
    assert!(world.attach_unit_to_squad(target_unit, target_squad));

    (world, attacker_squad, target_squad, target_unit)
}

fn execute_attack(world: &mut World, attacker_squad: sim::EntityId, target_unit: sim::EntityId) {
    let entry = CommandEntry {
        command: QueuedCommand::Work(WorkCommand::attack_squads(
            1,
            vec![attacker_squad],
            target_unit,
        )),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };
    CommandExecutor::new().execute(world, &entry);
}

#[test]
fn attack_command_canonicalizes_member_target_and_chases_to_tactic_range() {
    let database = combat_database();
    let gameplay = gameplay(&database);
    let (mut world, attacker_squad, target_squad, target_unit) = combat_world();
    execute_attack(&mut world, attacker_squad, target_unit);

    let squad = world.get_squad(attacker_squad).unwrap();
    assert_eq!(squad.state, SquadState::Attacking);
    assert_eq!(squad.attack_target, Some(target_squad));
    assert_eq!(squad.move_target, None);

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_squad(attacker_squad).unwrap().position().x > 0.0);
    for _ in 0..80 {
        world.update_entities_with_gameplay(0.05, &gameplay);
    }

    let attacker = world.get_squad(attacker_squad).unwrap();
    let target = world.get_squad(target_squad).unwrap();
    let offset = target.position() - attacker.position();
    let distance = Vec3::new(offset.x, 0.0, offset.z).length();
    assert!(distance <= 10.0);
    assert_eq!(attacker.state, SquadState::Attacking);
    assert_eq!(attacker.move_target, None);
    assert_eq!(attacker.attack_target, Some(target_squad));
    assert!(attacker.base.forward.x > 0.99);
}

#[test]
fn invalid_or_friendly_attack_orders_are_rejected() {
    let (mut world, attacker_squad, target_squad, target_unit) = combat_world();
    let friendly = world.create_unit(1);

    assert!(!world.issue_attack_order(1, attacker_squad, friendly, 0.0));
    assert!(!world.issue_attack_order(2, attacker_squad, target_unit, 0.0));
    assert!(!world.issue_attack_order(1, attacker_squad, sim::EntityId::INVALID, 0.0));
    assert_eq!(
        world.get_squad(attacker_squad).unwrap().state,
        SquadState::Idle
    );
    assert!(world.get_squad(target_squad).is_some());
}

#[test]
fn removing_the_target_squad_cancels_the_order_and_changes_checksum() {
    let database = combat_database();
    let gameplay = gameplay(&database);
    let (mut world, attacker_squad, target_squad, target_unit) = combat_world();
    let idle_checksum = world.checksum();
    execute_attack(&mut world, attacker_squad, target_unit);
    let attacking_checksum = world.checksum();
    assert_ne!(idle_checksum, attacking_checksum);

    assert!(world.remove_unit(target_unit).is_some());
    assert!(world.get_squad(target_squad).is_none());
    world.update_entities_with_gameplay(0.05, &gameplay);

    let attacker = world.get_squad(attacker_squad).unwrap();
    assert_eq!(attacker.state, SquadState::Idle);
    assert_eq!(attacker.attack_target, None);
    assert_eq!(attacker.move_target, None);
}
