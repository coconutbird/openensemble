use super::*;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::WorkCommand;
use crate::entities::units::UnitJumpPhase;
use crate::entities::{GatherPhase, RecoveryType, SquadContainmentState, UnitGarrison};
use crate::executor::CommandExecutor;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::tactics::{Action, ActionDuration, TacticData, Weapon};
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper, TechEffect};
use pipeline::database::hw1::{Ability, Database, GameData, ProtoObject, Tech};

#[test]
fn work_command_drives_deterministic_uninterruptible_jump_and_recovery() {
    let (database, gameplay) = jump_fixture(false);
    let (mut first, first_squad, first_units) = jump_world(2);
    let (mut second, second_squad, second_units) = jump_world(2);
    let initial_checksum = first.checksum();

    execute_location_jump(&mut first, &database, &gameplay, first_squad);
    execute_location_jump(&mut second, &database, &gameplay, second_squad);

    assert_eq!(first.checksum(), second.checksum());
    assert_ne!(first.checksum(), initial_checksum);
    assert_eq!(
        first.get_squad(first_squad).unwrap().jump_phase(),
        SquadJumpPhase::Pending,
    );
    for unit_id in &first_units {
        let unit = first.get_unit(*unit_id).unwrap();
        assert_eq!(unit.jump_phase(), UnitJumpPhase::Pending);
        assert!(unit.has_active_move_action());
        assert!(!unit.is_attackable());
    }
    assert!(!first.issue_move_order(1, first_squad, Vec3::Z * 30.0));

    first.update_entities_with_gameplay(0.5, &gameplay);
    second.update_entities_with_gameplay(0.5, &gameplay);
    assert_eq!(first.checksum(), second.checksum());
    assert_eq!(
        first.get_squad(first_squad).unwrap().jump_phase(),
        SquadJumpPhase::Flying,
    );
    assert!(first.get_unit(first_units[0]).unwrap().base.position.y > 0.0);

    advance_until_landed(&mut first, first_squad, &gameplay);
    advance_until_landed(&mut second, second_squad, &gameplay);
    assert_eq!(first.checksum(), second.checksum());
    let squad = first.get_squad(first_squad).unwrap();
    assert_eq!(squad.jump_phase(), SquadJumpPhase::Inactive);
    assert_eq!(squad.recovery.recovery_type(), Some(RecoveryType::Ability));
    assert_eq!(squad.recovery.ability_id(), Some(1));
    assert!(squad.recovery.remaining() > 14.0);
    assert!(squad.recovery.remaining() < 15.0);
    assert_vec3_close(squad.base.position, Vec3::X * 40.0);
    for unit_id in first_units {
        let unit = first.get_unit(unit_id).unwrap();
        assert!(!unit.is_jumping());
        assert!(!unit.has_active_move_action());
        assert!(unit.is_attackable());
    }
    assert_eq!(second_units.len(), 2);
}

#[test]
fn brute_shadow_and_upgrade_effects_gate_all_voluntary_jump_actions() {
    let (database, gameplay) = jump_fixture(true);
    let (mut world, squad_id, _) = jump_world(1);

    assert!(!world.issue_jump_order(
        1,
        squad_id,
        JumpOrderRequest::location(JumpOrderType::Jump, Vec3::X * 40.0, Some(0)),
        &gameplay,
    ));
    assert_eq!(world.activate_technology(1, &database, "basic"), Ok(true));
    assert!(!world.issue_jump_order(
        1,
        squad_id,
        JumpOrderRequest::location(JumpOrderType::Jump, Vec3::X * 40.0, Some(0)),
        &gameplay,
    ));
    assert_eq!(
        world.activate_technology(1, &database, "cov_brute_upgrade2"),
        Ok(true),
    );
    assert!(world.issue_jump_order(
        1,
        squad_id,
        JumpOrderRequest::location(JumpOrderType::Jump, Vec3::X * 40.0, Some(0)),
        &gameplay,
    ));
}

#[test]
fn jump_attack_lands_at_half_weapon_range_without_starting_an_attack() {
    let (database, gameplay) = jump_fixture(false);
    let (mut world, squad_id, unit_ids) = jump_world(1);
    let target_id = world.create_unit_at(2, Vec3::X * 80.0);
    world
        .get_unit_mut(target_id)
        .unwrap()
        .obstruction_half_extents = Vec3::splat(0.5);
    let command = WorkCommand::jump_attack_squads(1, vec![squad_id], target_id, Some(0));
    execute_work(&mut world, &database, &gameplay, command);

    assert_vec3_close(
        world.get_squad(squad_id).unwrap().jump_target().unwrap(),
        Vec3::X * 62.5,
    );
    advance_until_landed(&mut world, squad_id, &gameplay);

    let unit = world.get_unit(unit_ids[0]).unwrap();
    assert_vec3_close(unit.base.position, Vec3::X * 62.5);
    assert_eq!(unit.attack_target, None);
    assert_eq!(world.get_squad(squad_id).unwrap().attack_target, None);
}

#[test]
fn jump_gather_retains_and_starts_the_resource_order_after_landing() {
    let (database, gameplay) = jump_fixture(false);
    let (mut world, squad_id, _) = jump_world(1);
    let target_id = world.create_unit_at(0, Vec3::X * 40.0);
    let target = world.get_unit_mut(target_id).unwrap();
    target.obstruction_half_extents = Vec3::splat(0.5);
    target.resource_node.configure(
        Some("Supplies".to_owned()),
        Some(100.0),
        false,
        false,
        Some(-1),
    );
    let command = WorkCommand::jump_gather_squads(1, vec![squad_id], target_id, Some(0));
    execute_work(&mut world, &database, &gameplay, command);
    advance_until_landed(&mut world, squad_id, &gameplay);

    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.gather_target(), Some(target_id));
    assert!(matches!(
        squad.gather_phase(),
        GatherPhase::Moving | GatherPhase::Working,
    ));
}

#[test]
fn jump_garrison_retains_and_enters_the_container_after_landing() {
    let (database, gameplay) = jump_fixture(false);
    let (mut world, squad_id, unit_ids) = jump_world(1);
    let container_id = world.create_unit_at(1, Vec3::X * 40.0);
    let container = world.get_unit_mut(container_id).unwrap();
    container.obstruction_half_extents = Vec3::splat(0.5);
    container.garrison = UnitGarrison::container(10.0, false, false, vec!["Infantry".to_owned()]);
    let command = WorkCommand::jump_garrison_squads(1, vec![squad_id], container_id, Some(0));
    execute_work(&mut world, &database, &gameplay, command);
    advance_until_landed(&mut world, squad_id, &gameplay);

    let squad = world.get_squad(squad_id).unwrap();
    match squad.garrison.state() {
        SquadContainmentState::Garrisoning { target, .. } => assert_eq!(target, container_id),
        SquadContainmentState::Garrisoned { container, .. } => {
            assert_eq!(container, container_id);
        }
        state => panic!("unexpected post-jump containment state: {state:?}"),
    }
    if squad.garrison.is_garrisoned() {
        assert_eq!(
            world.get_unit(unit_ids[0]).unwrap().garrison.container_id(),
            Some(container_id),
        );
    }
}

fn execute_location_jump(
    world: &mut World,
    database: &Database,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
) {
    let command = WorkCommand::jump_squads(1, vec![squad_id], Vec3::X * 40.0, Some(0));
    execute_work(world, database, gameplay, command);
}

fn execute_work(
    world: &mut World,
    database: &Database,
    gameplay: &GameplayCatalog,
    command: WorkCommand,
) {
    CommandExecutor::with_database_and_gameplay(database, gameplay).execute(
        world,
        &CommandEntry {
            command: QueuedCommand::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn advance_until_landed(world: &mut World, squad_id: EntityId, gameplay: &GameplayCatalog) {
    for _ in 0..100 {
        if !world.get_squad(squad_id).unwrap().is_jumping() {
            return;
        }
        world.update_entities_with_gameplay(0.05, gameplay);
    }
    panic!("jump did not complete");
}

fn jump_world(unit_count: usize) -> (World, EntityId, Vec<EntityId>) {
    assert!((1..=2).contains(&unit_count));
    let mut world = World::with_seed(29);
    world.init_players(2);
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let mut unit_ids = Vec::new();
    for index in 0..unit_count {
        let x = match (unit_count, index) {
            (1, _) => 0.0,
            (2, 0) => -1.0,
            (2, _) => 1.0,
            _ => unreachable!(),
        };
        let unit_id = world.create_unit_at(1, Vec3::X * x);
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = "Brute".to_owned();
        unit.logical_proto_object_name = "Brute".to_owned();
        unit.object_types = vec!["Infantry".to_owned()];
        unit.obstruction_half_extents = Vec3::splat(0.25);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        unit_ids.push(unit_id);
    }
    (world, squad_id, unit_ids)
}

fn jump_fixture(locked: bool) -> (Database, GameplayCatalog) {
    let mut database = Database {
        abilities: vec![
            Ability {
                name: "Command".to_owned(),
                ..Ability::default()
            },
            Ability {
                name: "CovJumppack".to_owned(),
                ability_type: Some("Work".to_owned()),
                target_type: Some("Location".to_owned()),
                recover_type: Some("Ability".to_owned()),
                recover_time: Some(15.0),
                ..Ability::default()
            },
        ],
        objects: vec![ProtoObject {
            name: "Brute".to_owned(),
            tactics: Some("brute.tactics".to_owned()),
            ability_command: Some("CovJumppack".to_owned()),
            flags: if locked {
                vec!["AbilityDisabled".to_owned()]
            } else {
                Vec::new()
            },
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
    if locked {
        database
            .techs
            .extend([basic_shadow_tech(), jump_upgrade_tech()]);
    }
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("Brute".to_owned(), brute_tactics(locked))]);
    (database, gameplay)
}

fn brute_tactics(starts_disabled: bool) -> TacticData {
    let mut actions = [
        ("Jump", "Jump"),
        ("JumpGather", "JumpGather"),
        ("JumpGarrison", "JumpGarrison"),
        ("JumpAttack", "JumpAttack"),
    ]
    .into_iter()
    .map(|(name, action_type)| jump_action(name, action_type, starts_disabled))
    .collect::<Vec<_>>();
    actions.push(Action {
        name: "GatherSupplies".to_owned(),
        action_type: Some("Gather".to_owned()),
        resource: Some("Supplies".to_owned()),
        work_rate: Some(5.0),
        work_range: Some(3.0),
        ..Action::default()
    });
    TacticData {
        weapons: vec![Weapon {
            name: "BruteGun".to_owned(),
            max_range: Some(35.0),
            ..Weapon::default()
        }],
        actions,
        ..TacticData::default()
    }
}

fn jump_action(name: &str, action_type: &str, starts_disabled: bool) -> Action {
    Action {
        name: name.to_owned(),
        action_type: Some(action_type.to_owned()),
        weapon: Some("BruteGun".to_owned()),
        duration: Some(ActionDuration {
            seconds: 175.0,
            ..ActionDuration::default()
        }),
        velocity_scalar: Some(40.0),
        start_disabled: Some(starts_disabled),
        ..Action::default()
    }
}

fn basic_shadow_tech() -> Tech {
    Tech {
        name: "basic".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![jump_data_effect("AbilityDisabled", 1.0, None)],
        }),
        ..Tech::default()
    }
}

fn jump_upgrade_tech() -> Tech {
    let mut entries = vec![jump_data_effect("AbilityDisabled", 0.0, None)];
    entries.extend(
        ["Jump", "JumpGather", "JumpGarrison", "JumpAttack"]
            .into_iter()
            .map(|action| jump_data_effect("ActionEnable", 1.0, Some(action))),
    );
    Tech {
        name: "cov_brute_upgrade2".to_owned(),
        effects: Some(EffectsWrapper { entries }),
        ..Tech::default()
    }
}

fn jump_data_effect(subtype: &str, amount: f32, action: Option<&str>) -> TechEffect {
    TechEffect {
        effect_type: "Data".to_owned(),
        subtype: Some(subtype.to_owned()),
        amount: Some(amount),
        relativity: Some("Absolute".to_owned()),
        action: action.map(str::to_owned),
        target: Some(EffectTarget {
            target_type: Some("ProtoUnit".to_owned()),
            value: Some("Brute".to_owned()),
        }),
        ..TechEffect::default()
    }
}

fn assert_vec3_close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, 0.001),
        "expected {expected:?}, got {actual:?}",
    );
}
