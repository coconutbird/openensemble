use super::*;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{BuildingCommand, WorkCommand};
use crate::entities::units::ResearchTask;
use crate::entities::{SquadChangeModePhase, UnitChangeModePhase};
use crate::executor::CommandExecutor;
use crate::player::Resources;
use glam::Vec3;
use pipeline::database::hw1::objects::SquadModeAnimation;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule};
use pipeline::database::hw1::{Ability, Database, ProtoObject};

#[test]
fn command_ability_toggles_through_checksummed_animation_lifecycle() {
    let (database, gameplay) = change_mode_fixture();
    let (mut first, first_squad, first_unit) = change_mode_world(false);
    let (mut second, second_squad, second_unit) = change_mode_world(false);
    let initial_checksum = first.checksum();

    execute_work_change_mode(&mut first, &database, &gameplay, first_squad);
    execute_work_change_mode(&mut second, &database, &gameplay, second_squad);

    assert_eq!(first.checksum(), second.checksum());
    assert_ne!(first.checksum(), initial_checksum);
    let squad = first.get_squad(first_squad).unwrap();
    assert_eq!(squad.mode, SquadMode::Normal);
    assert_eq!(squad.changing_to_mode(), Some(SquadMode::Lockdown));
    assert_eq!(squad.change_mode_phase(), SquadChangeModePhase::Pending);
    assert!(!squad.can_accept_movement_order());
    assert!(!first.issue_move_order(1, first_squad, Vec3::X * 30.0));

    advance_pair(&mut first, &mut second, 0.1, &gameplay);
    assert_eq!(
        first.get_squad(first_squad).unwrap().change_mode_phase(),
        SquadChangeModePhase::Working
    );
    assert_eq!(
        first.get_unit(first_unit).unwrap().change_mode_phase(),
        UnitChangeModePhase::Working
    );
    let animation = first.entity_scripted_animation(first_unit).unwrap();
    assert_eq!(animation.animation_type(), "AnchorDown");
    assert_eq!(animation.asset_path(), Some("art/cobra/anchor_down.gr2"));

    advance_pair(&mut first, &mut second, 0.51, &gameplay);
    assert_eq!(first.checksum(), second.checksum());
    let squad = first.get_squad(first_squad).unwrap();
    assert_eq!(squad.mode, SquadMode::Lockdown);
    assert_eq!(squad.change_mode_phase(), SquadChangeModePhase::Inactive);
    assert!((squad.mode_movement_modifier() - 0.5).abs() < f32::EPSILON);
    assert!(!squad.can_accept_movement_order());
    assert_eq!(
        first
            .entity_scripted_animation(first_unit)
            .unwrap()
            .animation_type(),
        "Idle"
    );

    execute_work_change_mode(&mut first, &database, &gameplay, first_squad);
    execute_work_change_mode(&mut second, &database, &gameplay, second_squad);
    advance_pair(&mut first, &mut second, 0.1, &gameplay);
    assert_eq!(
        first
            .entity_scripted_animation(first_unit)
            .unwrap()
            .animation_type(),
        "AnchorUp"
    );
    advance_pair(&mut first, &mut second, 0.51, &gameplay);

    let squad = first.get_squad(first_squad).unwrap();
    assert_eq!(squad.mode, SquadMode::Normal);
    assert!((squad.mode_movement_modifier() - 1.0).abs() < f32::EPSILON);
    assert!(squad.can_accept_movement_order());
    assert_eq!(
        second.get_unit(second_unit).unwrap().change_mode_phase(),
        UnitChangeModePhase::Inactive
    );
    assert_eq!(first.checksum(), second.checksum());
}

#[test]
fn direct_building_command_targets_the_parent_squad() {
    let (database, gameplay) = change_mode_fixture();
    let (mut world, squad_id, building_id) = change_mode_world(true);
    let command = BuildingCommand::change_mode(1, vec![building_id], SquadMode::Lockdown);

    execute_command(
        &mut world,
        &database,
        &gameplay,
        QueuedCommand::Building(command),
    );

    assert_eq!(
        world.get_squad(squad_id).unwrap().changing_to_mode(),
        Some(SquadMode::Lockdown)
    );
    world.update_entities_with_gameplay(0.56, &gameplay);
    assert_eq!(world.get_squad(squad_id).unwrap().mode, SquadMode::Lockdown);
}

#[test]
fn change_mode_propagates_to_associated_wall_towers() {
    let (database, gameplay) = change_mode_fixture();
    let (mut world, source_squad, _) = change_mode_world(false);
    let target_squad = world.create_squad_at(1, Vec3::X * 10.0);
    let target_unit = world.create_unit_at(1, Vec3::X * 10.0);
    configure_cobra(world.get_unit_mut(target_unit).unwrap());
    assert!(world.attach_unit_to_squad(target_unit, target_squad));
    assert!(world.set_tower_wall_destination(source_squad, target_squad));

    let command =
        WorkCommand::change_mode_squads(1, vec![source_squad], Some(SquadMode::Lockdown), None);
    execute_command(
        &mut world,
        &database,
        &gameplay,
        QueuedCommand::Work(command),
    );

    assert_eq!(
        world.get_squad(source_squad).unwrap().changing_to_mode(),
        Some(SquadMode::Lockdown)
    );
    assert_eq!(
        world.get_squad(target_squad).unwrap().changing_to_mode(),
        Some(SquadMode::Lockdown)
    );
    world.update_entities_with_gameplay(0.56, &gameplay);
    assert_eq!(
        world.get_squad(source_squad).unwrap().mode,
        SquadMode::Lockdown
    );
    assert_eq!(
        world.get_squad(target_squad).unwrap().mode,
        SquadMode::Lockdown
    );
}

#[test]
fn leaving_lockdown_menu_clears_and_refunds_production() {
    let (database, gameplay) = change_mode_fixture();
    let (mut world, squad_id, building_id) = change_mode_world(true);
    world.get_squad_mut(squad_id).unwrap().mode = SquadMode::Lockdown;
    let mut cost = Resources::new();
    cost.set(0, 25.0);
    world
        .get_building_mut(building_id)
        .unwrap()
        .production
        .enqueue_research(ResearchTask {
            player_id: 1,
            technology_id: 7,
            technology_name: "QueuedUpgrade".to_owned(),
            current_points: 0.0,
            total_points: 10.0,
            cost,
            trigger_state: None,
        });

    let command = BuildingCommand::change_mode(1, vec![building_id], SquadMode::Normal);
    execute_command(
        &mut world,
        &database,
        &gameplay,
        QueuedCommand::Building(command),
    );
    assert!(
        !world
            .get_building(building_id)
            .unwrap()
            .production
            .is_idle()
    );

    world.update_entities_with_gameplay(0.1, &gameplay);

    assert!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .is_idle()
    );
    assert!((world.get_player(1).unwrap().resources.get(0) - 25.0).abs() < f32::EPSILON);
}

#[test]
fn lockdown_ability_move_unlocks_resumes_and_relocks() {
    let (database, gameplay) = change_mode_fixture();
    let (mut first, first_squad, _) = change_mode_world(false);
    let (mut second, second_squad, _) = change_mode_world(false);
    first.get_squad_mut(first_squad).unwrap().mode = SquadMode::Lockdown;
    second.get_squad_mut(second_squad).unwrap().mode = SquadMode::Lockdown;
    let target = Vec3::X * 20.0;

    execute_ability_move(&mut first, &database, &gameplay, first_squad, target);
    execute_ability_move(&mut second, &database, &gameplay, second_squad, target);
    assert_eq!(
        first.get_squad(first_squad).unwrap().changing_to_mode(),
        Some(SquadMode::Normal)
    );
    assert_eq!(first.get_squad(first_squad).unwrap().move_target, None);

    advance_pair(&mut first, &mut second, 0.56, &gameplay);
    let squad = first.get_squad(first_squad).unwrap();
    assert_eq!(squad.mode, SquadMode::Normal);
    assert_eq!(squad.state, crate::entities::SquadState::Moving);
    assert_eq!(squad.move_target, Some(target));

    for _ in 0..100 {
        advance_pair(&mut first, &mut second, 0.1, &gameplay);
        if first.get_squad(first_squad).unwrap().mode == SquadMode::Lockdown
            && !first.get_squad(first_squad).unwrap().is_changing_mode()
        {
            break;
        }
    }
    let squad = first.get_squad(first_squad).unwrap();
    assert_eq!(squad.base.position, target);
    assert_eq!(squad.mode, SquadMode::Lockdown);
    assert_eq!(squad.state, crate::entities::SquadState::Idle);
    assert_eq!(first.checksum(), second.checksum());
}

#[test]
fn lockdown_ability_attack_unlocks_to_chase_then_relocks_before_firing() {
    let (database, gameplay) = change_mode_fixture();
    let (mut world, squad_id, _) = change_mode_world(false);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world.get_squad_mut(squad_id).unwrap().mode = SquadMode::Lockdown;
    let target_id = world.create_unit_at(2, Vec3::X * 20.0);
    let mut command = WorkCommand::attack_squads(1, vec![squad_id], target_id);
    command.ability_id = 0;
    command.range = 2.0;

    execute_command(
        &mut world,
        &database,
        &gameplay,
        QueuedCommand::Work(command),
    );
    assert_eq!(
        world.get_squad(squad_id).unwrap().changing_to_mode(),
        Some(SquadMode::Normal)
    );

    world.update_entities_with_gameplay(0.56, &gameplay);
    assert_eq!(world.get_squad(squad_id).unwrap().mode, SquadMode::Normal);
    assert!(world.get_squad(squad_id).unwrap().move_target.is_some());

    for _ in 0..100 {
        world.update_entities_with_gameplay(0.1, &gameplay);
        if world.get_squad(squad_id).unwrap().mode == SquadMode::Lockdown
            && !world.get_squad(squad_id).unwrap().is_changing_mode()
        {
            break;
        }
    }
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.mode, SquadMode::Lockdown);
    assert_eq!(squad.state, crate::entities::SquadState::Attacking);
    assert_eq!(squad.attack_target, Some(target_id));
    assert!(squad.base.position.x >= 18.0);
}

#[test]
fn lockdown_menu_vehicle_installs_and_removes_default_rally() {
    let (database, gameplay) = change_mode_fixture();
    let (mut world, squad_id, unit_id) = change_mode_world(false);

    execute_work_change_mode(&mut world, &database, &gameplay, squad_id);
    world.update_entities_with_gameplay(0.56, &gameplay);

    assert_eq!(world.get_player(1).unwrap().goto_base_units(), &[unit_id]);
    assert_eq!(
        world
            .unit_rally_point(unit_id, 1)
            .map(crate::RallyPoint::position),
        Some(Vec3::new(0.0, 0.0, -50.0))
    );

    execute_work_change_mode(&mut world, &database, &gameplay, squad_id);
    world.update_entities_with_gameplay(0.56, &gameplay);
    assert_eq!(
        world.get_player(1).unwrap().goto_base_units(),
        [] as [EntityId; 0]
    );
    assert!(world.unit_rally_point(unit_id, 1).is_none());
}

#[test]
fn lockdown_menu_membership_follows_squad_ownership() {
    let (database, gameplay) = change_mode_fixture();
    let (mut world, squad_id, unit_id) = change_mode_world(false);
    world.init_players(2);

    execute_work_change_mode(&mut world, &database, &gameplay, squad_id);
    world.update_entities_with_gameplay(0.56, &gameplay);
    assert_eq!(world.get_player(1).unwrap().goto_base_units(), &[unit_id]);

    assert!(world.change_squad_owner(squad_id, 2));
    assert_eq!(
        world.get_player(1).unwrap().goto_base_units(),
        [] as [EntityId; 0]
    );
    assert_eq!(world.get_player(2).unwrap().goto_base_units(), &[unit_id]);
    assert!(world.unit_rally_point(unit_id, 2).is_some());
}

#[test]
fn killing_lockdown_menu_leader_removes_goto_and_rally_state_immediately() {
    let (database, gameplay) = change_mode_fixture();
    let (mut world, squad_id, unit_id) = change_mode_world(false);

    execute_work_change_mode(&mut world, &database, &gameplay, squad_id);
    world.update_entities_with_gameplay(0.56, &gameplay);
    assert_eq!(world.get_player(1).unwrap().goto_base_units(), &[unit_id]);

    assert!(world.kill_unit(unit_id, false));
    assert_eq!(
        world.get_player(1).unwrap().goto_base_units(),
        [] as [EntityId; 0]
    );
    assert!(world.unit_rally_point(unit_id, 1).is_none());
    assert!(world.get_unit(unit_id).is_some());
}

fn advance_pair(first: &mut World, second: &mut World, dt: f32, gameplay: &GameplayCatalog) {
    first.update_entities_with_gameplay(dt, gameplay);
    second.update_entities_with_gameplay(dt, gameplay);
}

fn execute_work_change_mode(
    world: &mut World,
    database: &Database,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
) {
    let command = WorkCommand::change_mode_squads(1, vec![squad_id], None, Some(0));
    execute_command(world, database, gameplay, QueuedCommand::Work(command));
}

fn execute_ability_move(
    world: &mut World,
    database: &Database,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
    target: Vec3,
) {
    let mut command = WorkCommand::move_squads(1, vec![squad_id], target);
    command.ability_id = 0;
    execute_command(world, database, gameplay, QueuedCommand::Work(command));
}

fn execute_command(
    world: &mut World,
    database: &Database,
    gameplay: &GameplayCatalog,
    command: QueuedCommand,
) {
    CommandExecutor::with_database_and_gameplay(database, gameplay).execute(
        world,
        &CommandEntry {
            command,
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn change_mode_world(building: bool) -> (World, EntityId, EntityId) {
    let mut world = World::with_seed(37);
    world.init_players(1);
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let unit_id = if building {
        world.create_building_at(1, Vec3::ZERO)
    } else {
        world.create_unit_at(1, Vec3::ZERO)
    };
    configure_cobra(world.get_unit_mut(unit_id).unwrap());
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (world, squad_id, unit_id)
}

fn configure_cobra(unit: &mut crate::entities::Unit) {
    unit.proto_object_name = "Cobra".to_owned();
    unit.logical_proto_object_name = "Cobra".to_owned();
    unit.object_types = vec!["Vehicle".to_owned()];
}

fn change_mode_fixture() -> (Database, GameplayCatalog) {
    let database = Database {
        abilities: vec![
            Ability {
                name: "Command".to_owned(),
                ..Ability::default()
            },
            Ability {
                name: "CobraLockdown".to_owned(),
                ability_type: Some("ChangeMode".to_owned()),
                squad_mode: Some("Lockdown".to_owned()),
                keep_squad_mode: Some(true),
                movement_speed_modifier: Some(0.5),
                movement_modifier_type: Some("Mode".to_owned()),
                ..Ability::default()
            },
        ],
        objects: vec![ProtoObject {
            name: "Cobra".to_owned(),
            tactics: Some("cobra.tactics".to_owned()),
            ability_command: Some("CobraLockdown".to_owned()),
            flags: vec!["LockdownMenu".to_owned()],
            goto_type: Some("Vehicle".to_owned()),
            rally_point: Some("Military".to_owned()),
            squad_mode_animations: vec![
                SquadModeAnimation {
                    animation: "AnchorDown".to_owned(),
                    mode: Some("Lockdown".to_owned()),
                },
                SquadModeAnimation {
                    animation: "AnchorUp".to_owned(),
                    mode: Some("Normal".to_owned()),
                },
            ],
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let tactics = TacticData {
        actions: vec![
            Action {
                name: "Lock".to_owned(),
                action_type: Some("ChangeMode".to_owned()),
                squad_mode: Some("Lockdown".to_owned()),
                ..Action::default()
            },
            Action {
                name: "Unlock".to_owned(),
                action_type: Some("ChangeMode".to_owned()),
                squad_mode: Some("Normal".to_owned()),
                ..Action::default()
            },
        ],
        tactic: Some(TacticRules {
            target_rules: vec![
                change_mode_rule("Normal", "Lock"),
                change_mode_rule("Lockdown", "Unlock"),
            ],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let mut gameplay = GameplayCatalog::from_tactics(&database, [("Cobra".to_owned(), tactics)]);
    gameplay.insert_test_scripted_animation_clip(
        "Cobra",
        "AnchorDown",
        "art/cobra/anchor_down.gr2",
        500,
    );
    gameplay.insert_test_scripted_animation_clip(
        "Cobra",
        "AnchorUp",
        "art/cobra/anchor_up.gr2",
        500,
    );
    (database, gameplay)
}

fn change_mode_rule(mode: &str, action: &str) -> TargetRule {
    TargetRule {
        relation: Some("Self".to_owned()),
        squad_mode: Some(mode.to_owned()),
        ability: Some("CobraLockdown".to_owned()),
        action: Some(action.to_owned()),
        ..TargetRule::default()
    }
}
