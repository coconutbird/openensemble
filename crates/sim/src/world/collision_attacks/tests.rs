use super::*;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::WorkCommand;
use crate::entities::RecoveryType;
use crate::executor::CommandExecutor;
use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
use pipeline::database::hw1::tactics::{Action, TacticData, TacticState, Weapon};
use pipeline::database::hw1::weapontypes::DamageModifier;
use pipeline::database::hw1::{Ability, Database, ProtoObject, WeaponType};

#[test]
fn command_ram_consumes_juice_applies_reflection_impulses_and_recovery() {
    let (database, gameplay) = ram_gameplay(Some(10_000.0));
    let (mut world, attacker_squad, attacker_id, target_squad, target_id) = ram_world(1_000.0);
    issue_ram(&mut world, &database, attacker_squad, target_squad);

    assert_eq!(
        world.get_squad(attacker_squad).unwrap().mode,
        SquadMode::HitAndRun
    );
    world.update_entities_with_gameplay(0.05, &gameplay);

    let attacker = world.get_unit(attacker_id).unwrap();
    let target = world.get_unit(target_id).unwrap();
    assert_close(target.hitpoints, 920.0);
    assert_close(attacker.hitpoints, 987.5);
    assert_close(attacker.ammunition.current(), 0.0);
    assert!(target.base.velocity.y > 0.0);
    assert!(attacker.base.velocity.y > 0.0);
    let squad = world.get_squad(attacker_squad).unwrap();
    assert_eq!(squad.mode, SquadMode::Normal);
    assert_eq!(squad.attack_target, None);
    assert_eq!(squad.recovery.recovery_type(), Some(RecoveryType::Ability));
    assert_eq!(squad.recovery.ability_id(), Some(1));
    assert_close(squad.recovery.remaining(), 2.0);
}

#[test]
fn capped_lethal_ram_spends_target_hitpoints_and_fires_retail_event() {
    let (database, gameplay) = ram_gameplay(Some(40.0));
    let (mut world, attacker_squad, attacker_id, target_squad, target_id) = ram_world(30.0);
    let event_id =
        world.subscribe_general_event(GeneralEventType::GameEntityRammed, Some(2), false);
    issue_ram(&mut world, &database, attacker_squad, target_squad);

    world.update_entities_with_gameplay(0.05, &gameplay);

    assert!(world.get_unit(target_id).is_none());
    assert_close(
        world.get_unit(attacker_id).unwrap().ammunition.current(),
        70.0,
    );
    assert!(world.general_event_fired(event_id));
    assert_eq!(
        world
            .get_squad(attacker_squad)
            .unwrap()
            .recovery
            .ability_id(),
        Some(1)
    );
}

#[test]
fn physical_rammer_detects_and_bowls_nonphysical_infantry_obstruction() {
    let (database, gameplay) = ram_gameplay(Some(10_000.0));
    let mut world = World::with_seed(23);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let attacker_squad = world.create_squad_at(1, Vec3::ZERO);
    let attacker_id = physical_unit(&mut world, 1, Vec3::ZERO, "rammer", 1_000.0);
    assert!(world.attach_unit_to_squad(attacker_id, attacker_squad));
    let target_position = Vec3::X * 1.5;
    let target_squad = world.create_squad_at(2, target_position);
    let target_id = world.create_unit_at(2, target_position);
    let target = world.get_unit_mut(target_id).unwrap();
    target.proto_object_name = "light_target".to_owned();
    target.set_max_hitpoints(1_000.0);
    target.obstruction_half_extents = Vec3::ONE;
    assert!(world.attach_unit_to_squad(target_id, target_squad));
    issue_ram(&mut world, &database, attacker_squad, target_squad);

    world.update_entities_with_gameplay(0.05, &gameplay);

    assert_close(world.get_unit(target_id).unwrap().hitpoints, 800.0);
    assert!(world.get_squad(target_squad).unwrap().base.position.x > target_position.x);
    let attacker = world.get_unit(attacker_id).unwrap();
    assert_close(attacker.ammunition.current(), 0.0);
    let squad = world.get_squad(attacker_squad).unwrap();
    assert_eq!(squad.mode, SquadMode::HitAndRun);
    assert_eq!(squad.attack_target, Some(target_squad));
    assert_eq!(squad.recovery.ability_id(), Some(1));
}

#[test]
fn persistent_collision_action_sets_and_clears_authored_tactic_state() {
    let (database, gameplay) = ram_gameplay(Some(10_000.0));
    let (mut world, attacker_squad, attacker_id, target_squad, _) =
        ram_world_at(1_000.0, Vec3::X * 100.0);
    let ram_state = gameplay.tactic_state_id("rammer", "RamState").unwrap();
    issue_ram(&mut world, &database, attacker_squad, target_squad);

    world.update_entities_with_gameplay(0.05, &gameplay);
    let attacker = world.get_unit(attacker_id).unwrap();
    assert_eq!(attacker.tactic_state(), Some(ram_state));
    assert_eq!(attacker.tactic_state_revision(), 1);

    world.get_squad_mut(attacker_squad).unwrap().mode = SquadMode::Normal;
    world.update_entities_with_gameplay(0.05, &gameplay);
    let attacker = world.get_unit(attacker_id).unwrap();
    assert_eq!(attacker.tactic_state(), None);
    assert_eq!(attacker.tactic_state_revision(), 2);
}

fn ram_world(target_hitpoints: f32) -> (World, EntityId, EntityId, EntityId, EntityId) {
    ram_world_at(target_hitpoints, Vec3::X * 1.5)
}

fn ram_world_at(
    target_hitpoints: f32,
    target_position: Vec3,
) -> (World, EntityId, EntityId, EntityId, EntityId) {
    let mut world = World::with_seed(17);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let attacker_squad = world.create_squad_at(1, Vec3::ZERO);
    let attacker_id = physical_unit(&mut world, 1, Vec3::ZERO, "rammer", 1_000.0);
    assert!(world.attach_unit_to_squad(attacker_id, attacker_squad));
    let target_squad = world.create_squad_at(2, target_position);
    let target_id = physical_unit(&mut world, 2, target_position, "target", target_hitpoints);
    assert!(world.attach_unit_to_squad(target_id, target_squad));
    (world, attacker_squad, attacker_id, target_squad, target_id)
}

fn physical_unit(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    proto: &str,
    hitpoints: f32,
) -> EntityId {
    let unit_id = world.create_unit_at(player_id, position);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_name = proto.to_owned();
    unit.set_max_hitpoints(hitpoints);
    unit.obstruction_half_extents = Vec3::ONE;
    unit.physics = Some(PhysicsBody::ground_vehicle(
        PhysicsMaterial {
            mass: 10.0,
            friction: 0.0,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
        },
        BoxCollider::new(Vec3::ONE, Vec3::ZERO),
        0.0,
        20.0,
        1_000.0,
        720.0,
    ));
    unit.ammunition.configure(100.0, 0.0, true);
    unit_id
}

fn issue_ram(
    world: &mut World,
    database: &Database,
    attacker_squad: EntityId,
    target_squad: EntityId,
) {
    let mut command = WorkCommand::attack_squads(1, vec![attacker_squad], target_squad);
    command.ability_id = 0;
    CommandExecutor::with_database(database).execute(
        world,
        &CommandEntry {
            command: QueuedCommand::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn ram_gameplay(max_damage_per_ram: Option<f32>) -> (Database, GameplayCatalog) {
    let mut database = Database::new();
    database.abilities.extend([
        Ability {
            name: "Command".to_owned(),
            ..Ability::default()
        },
        Ability {
            name: "Ram".to_owned(),
            squad_mode: Some("HitAndRun".to_owned()),
            recover_start: Some("Attack".to_owned()),
            recover_type: Some("Ability".to_owned()),
            recover_time: Some(2.0),
            ..Ability::default()
        },
    ]);
    database.objects.extend([
        ProtoObject {
            name: "rammer".to_owned(),
            tactics: Some("rammer.tactics".to_owned()),
            ability_command: Some("Ram".to_owned()),
            damage_type: Some("Medium".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "target".to_owned(),
            damage_type: Some("Medium".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "light_target".to_owned(),
            damage_type: Some("Light".to_owned()),
            ..ProtoObject::default()
        },
    ]);
    database.weapon_types.push(WeaponType {
        name: "WarthogRam".to_owned(),
        damage_modifiers: vec![
            DamageModifier {
                damage_type: "Medium".to_owned(),
                modifier: 0.8,
                reflect_damage_factor: Some(0.25),
                rammable: Some(true),
                ..DamageModifier::default()
            },
            DamageModifier {
                damage_type: "Light".to_owned(),
                modifier: 2.0,
                reflect_damage_factor: Some(0.3),
                bowlable: Some(true),
                ..DamageModifier::default()
            },
        ],
        ..WeaponType::default()
    });
    let tactics = TacticData {
        weapons: vec![Weapon {
            name: "Ram".to_owned(),
            weapon_type: Some("WarthogRam".to_owned()),
            aoe_radius: Some(40.0),
            max_damage_per_ram,
            reflect_damage_factor: Some(0.5),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "PersistentCollisionAttack".to_owned(),
            action_type: Some("CollisionAttack".to_owned()),
            weapon: Some("Ram".to_owned()),
            new_tactic_state: Some("RamState".to_owned()),
            ..Action::default()
        }],
        states: vec![TacticState {
            name: "RamState".to_owned(),
            run_anim: Some("Run".to_owned()),
            ..TacticState::default()
        }],
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("rammer".to_owned(), tactics)]);
    (database, gameplay)
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= 0.000_1 * expected.abs().max(1.0));
}
