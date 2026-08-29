use super::*;
use crate::entities::{TrainingKind, TrainingTask, UnitGarrison};
use crate::player::{PlayerState, Resources};
use crate::trigger::value::Cost;
use crate::trigger::{TriggerVar, VarType};
use glam::Vec3;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad, Tech};

#[test]
fn compare_proto_squad_uses_retail_ordered_operator_semantics() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::ProtoSquad,
        TriggerValue::ProtoSquad(12),
    );
    add_value(&mut script, 2, VarType::Operator, TriggerValue::Int(1));
    add_value(
        &mut script,
        3,
        VarType::ProtoSquad,
        TriggerValue::ProtoSquad(27),
    );
    let condition = Condition::new(1, ConditionType::CompareProtoSquad)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);

    assert_condition(&condition, &mut script, &mut world, ConditionResult::True);
}

#[test]
fn compare_design_line_uses_retail_ordered_operator_semantics() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::DesignLine,
        TriggerValue::DesignLine(12),
    );
    add_value(&mut script, 2, VarType::Operator, TriggerValue::Int(1));
    add_value(
        &mut script,
        3,
        VarType::DesignLine,
        TriggerValue::DesignLine(27),
    );
    let condition = Condition::new(1, ConditionType::CompareDesignLine)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);

    assert_condition(&condition, &mut script, &mut world, ConditionResult::True);
}

#[test]
fn is_idle_uses_action_presence_unit_precedence_and_retail_duration_output() {
    let mut world = World::new();
    world.init_players(1);
    let unit_id = world.create_unit(1);
    let squad_id = world.create_squad(1);
    world.update_entities(0.05);
    world.update_entities(0.05);

    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Unit, TriggerValue::Unit(unit_id));
    add_value(
        &mut script,
        2,
        VarType::Squad,
        TriggerValue::Squad(squad_id),
    );
    add_value(&mut script, 3, VarType::Time, TriggerValue::Time(999));
    let condition = Condition::new(1, ConditionType::IsIdle)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3);

    assert_condition(&condition, &mut script, &mut world, ConditionResult::True);
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::Time(50)
    );

    assert!(
        world
            .get_unit_mut(unit_id)
            .unwrap()
            .move_to(Vec3::new(10.0, 0.0, 0.0))
    );
    assert_condition(&condition, &mut script, &mut world, ConditionResult::False);
    assert_eq!(script.get_variable(3).unwrap().value, TriggerValue::Time(0));

    assert!(world.remove_unit(unit_id).is_some());
    assert_condition(&condition, &mut script, &mut world, ConditionResult::True);
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::Time(50)
    );
}

#[test]
fn garrison_conditions_follow_authoritative_containment_state() {
    let (mut world, container_squad, passenger_squad) = contained_world();
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::Squad).with_value(TriggerValue::Squad(container_squad)),
    );
    script.add_variable(
        TriggerVar::new(2, VarType::Integer)
            .with_value(TriggerValue::Int(0))
            .as_output(),
    );
    script.add_variable(
        TriggerVar::new(3, VarType::Squad).with_value(TriggerValue::Squad(passenger_squad)),
    );
    let has_garrisoned = Condition::new(1, ConditionType::HasGarrisoned)
        .with_input_at(1, 1)
        .with_output_at(2, 2);
    let is_garrisoned = Condition::new(2, ConditionType::IsGarrisoned).with_input_at(1, 3);

    assert_eq!(
        evaluate_condition(&has_garrisoned, 0, &mut script, &mut world),
        ConditionResult::True
    );
    assert_eq!(
        script.get_variable(2).map(|variable| &variable.value),
        Some(&TriggerValue::Int(1))
    );
    assert_eq!(
        evaluate_condition(&is_garrisoned, 0, &mut script, &mut world),
        ConditionResult::True
    );

    world
        .issue_ungarrison_order(1, passenger_squad, None)
        .expect("ungarrison order");
    world.advance_time(50);
    world.update_entities(0.05);
    assert_eq!(
        evaluate_condition(&is_garrisoned, 0, &mut script, &mut world),
        ConditionResult::False
    );
}

#[test]
fn contains_garrisoned_applies_optional_player_and_object_type_filters() {
    let (mut world, _, _, container_unit, _) = contained_world_with_units();
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::Unit).with_value(TriggerValue::Unit(container_unit)),
    );
    script.add_variable(TriggerVar::new(2, VarType::Player).with_value(TriggerValue::Player(1)));
    script.add_variable(
        TriggerVar::new(3, VarType::ObjectType)
            .with_value(TriggerValue::ObjectType("Infantry".to_owned())),
    );
    script.add_variable(TriggerVar::new(4, VarType::Player).with_value(TriggerValue::Player(0)));
    script.add_variable(
        TriggerVar::new(5, VarType::ObjectType)
            .with_value(TriggerValue::ObjectType("unsc_inf_marine_01".to_owned())),
    );
    script.add_variable(
        TriggerVar::new(6, VarType::ObjectType)
            .with_value(TriggerValue::ObjectType("Vehicle".to_owned())),
    );

    let unfiltered = Condition::new(1, ConditionType::ContainsGarrisoned).with_input_at(1, 1);
    let filtered = Condition::new(2, ConditionType::ContainsGarrisoned)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);
    let concrete_type = Condition::new(3, ConditionType::ContainsGarrisoned)
        .with_input_at(1, 1)
        .with_input_at(3, 5);
    let wrong_player = Condition::new(4, ConditionType::ContainsGarrisoned)
        .with_input_at(1, 1)
        .with_input_at(2, 4);
    let wrong_type = Condition::new(5, ConditionType::ContainsGarrisoned)
        .with_input_at(1, 1)
        .with_input_at(3, 6);

    for condition in [&unfiltered, &filtered, &concrete_type] {
        assert_eq!(
            evaluate_condition(condition, 0, &mut script, &mut world),
            ConditionResult::True
        );
    }
    for condition in [&wrong_player, &wrong_type] {
        assert_eq!(
            evaluate_condition(condition, 0, &mut script, &mut world),
            ConditionResult::False
        );
    }
}

#[test]
fn is_object_type_reproduces_retail_v1_and_v2_inputs() {
    let (mut world, _, passenger_squad, _, passenger_unit) = contained_world_with_units();
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::Squad).with_value(TriggerValue::Squad(passenger_squad)),
    );
    script.add_variable(
        TriggerVar::new(2, VarType::ObjectType)
            .with_value(TriggerValue::ObjectType("Infantry".to_owned())),
    );
    script.add_variable(
        TriggerVar::new(3, VarType::Unit).with_value(TriggerValue::Unit(passenger_unit)),
    );
    script.add_variable(
        TriggerVar::new(4, VarType::Object).with_value(TriggerValue::Object(passenger_unit)),
    );
    script.add_variable(
        TriggerVar::new(5, VarType::ProtoObject).with_value(TriggerValue::ProtoObject(17)),
    );
    script.add_variable(
        TriggerVar::new(6, VarType::ObjectType)
            .with_value(TriggerValue::ObjectType("Vehicle".to_owned())),
    );
    script.add_variable(
        TriggerVar::new(7, VarType::Unit)
            .with_value(TriggerValue::Unit(EntityId::new(EntityClass::Unit, 4_000))),
    );

    let mut version_one = Condition::new(1, ConditionType::IsObjectType)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    version_one.version = 1;
    let mut version_two = Condition::new(2, ConditionType::IsObjectType)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_input_at(5, 5);
    version_two.version = 2;
    let mut wrong_type = Condition::new(3, ConditionType::IsObjectType)
        .with_input_at(1, 1)
        .with_input_at(2, 6)
        .with_input_at(3, 3);
    wrong_type.version = 2;
    let mut stale_used_unit = Condition::new(4, ConditionType::IsObjectType)
        .with_input_at(2, 2)
        .with_input_at(3, 7);
    stale_used_unit.version = 2;

    for condition in [&version_one, &version_two, &stale_used_unit] {
        assert_eq!(
            evaluate_condition(condition, 0, &mut script, &mut world),
            ConditionResult::True
        );
    }
    assert_eq!(
        evaluate_condition(&wrong_type, 0, &mut script, &mut world),
        ConditionResult::False
    );
}

#[test]
fn player_unit_count_filters_live_units_and_preserves_the_retail_v2_bug() {
    let (mut world, mut script) = roster_condition_fixture();
    let mut filtered =
        roster_condition(10, ConditionType::ComparePlayerUnitCount, 4).with_input_at(2, 2);
    filtered.version = 1;
    let mut unfiltered = roster_condition(11, ConditionType::ComparePlayerUnitCount, 6);
    unfiltered.version = 1;
    let mut version_two = roster_condition(12, ConditionType::ComparePlayerUnitCount, 4)
        .with_input_at(2, 2)
        .with_input_at(5, 5);
    version_two.version = 2;
    let mut unknown_version = roster_condition(13, ConditionType::ComparePlayerUnitCount, 4);
    unknown_version.version = 3;

    assert_eq!(world.player_future_unit_count(1, Some("Infantry")), 1);
    for condition in [&filtered, &unfiltered, &version_two] {
        assert_eq!(
            evaluate_condition(condition, 0, &mut script, &mut world),
            ConditionResult::True
        );
    }
    assert_eq!(
        evaluate_condition(&unknown_version, 0, &mut script, &mut world),
        ConditionResult::False
    );
}

#[test]
fn player_squad_count_adds_only_queued_squads_when_requested() {
    let (mut world, mut script) = roster_condition_fixture();
    let filtered =
        roster_condition(20, ConditionType::ComparePlayerSquadCount, 4).with_input_at(2, 7);
    let with_training = roster_condition(21, ConditionType::ComparePlayerSquadCount, 6)
        .with_input_at(2, 7)
        .with_input_at(5, 5);
    let all_with_training =
        roster_condition(22, ConditionType::ComparePlayerSquadCount, 8).with_input_at(5, 5);

    for condition in [&filtered, &with_training, &all_with_training] {
        assert_eq!(
            evaluate_condition(condition, 0, &mut script, &mut world),
            ConditionResult::True
        );
    }
}

#[test]
fn player_identity_and_squad_capacity_follow_retail_signatures() {
    let (mut world, squad_id) = identity_condition_world();
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(&mut script, 2, VarType::PlayerState, TriggerValue::Int(2));
    add_value(&mut script, 3, VarType::Civ, TriggerValue::Int(4));
    add_value(&mut script, 4, VarType::Operator, TriggerValue::Int(3));
    add_value(&mut script, 5, VarType::Leader, TriggerValue::Int(7));
    add_value(&mut script, 6, VarType::Operator, TriggerValue::Int(1));
    add_value(
        &mut script,
        7,
        VarType::Squad,
        TriggerValue::Squad(squad_id),
    );

    let mut player_state = Condition::new(30, ConditionType::PlayerInState)
        .with_input_at(1, 1)
        .with_input_at(3, 2);
    player_state.version = 2;
    let civilization = identity_condition(31, ConditionType::CompareCiv, 3);
    let leader = identity_condition(32, ConditionType::CompareLeader, 5);
    let using_leader = Condition::new(33, ConditionType::PlayerUsingLeader)
        .with_input_at(1, 1)
        .with_input_at(2, 5);
    let squad_full = Condition::new(34, ConditionType::IsSquadAtMaxSize).with_input_at(1, 7);

    for condition in [&player_state, &civilization, &leader, &using_leader] {
        assert_condition(condition, &mut script, &mut world, ConditionResult::True);
    }
    assert_condition(&squad_full, &mut script, &mut world, ConditionResult::False);
    let second_member = world.create_unit(1);
    assert!(world.attach_unit_to_squad(second_member, squad_id));
    assert_condition(&squad_full, &mut script, &mut world, ConditionResult::True);

    player_state.version = 1;
    assert_condition(
        &player_state,
        &mut script,
        &mut world,
        ConditionResult::False,
    );
    let ordered_identity = Condition::new(35, ConditionType::CompareCiv)
        .with_input_at(1, 3)
        .with_input_at(2, 6)
        .with_input_at(3, 3);
    assert_condition(
        &ordered_identity,
        &mut script,
        &mut world,
        ConditionResult::False,
    );
}

#[test]
fn diplomacy_collects_live_teams_and_preserves_stale_input_behavior() {
    let (mut world, enemy_unit, allied_unit, enemy_squad) = diplomacy_condition_world();
    let stale_unit = EntityId::new(EntityClass::Unit, 4_000);
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Unit,
        TriggerValue::Unit(enemy_unit),
    );
    add_value(
        &mut script,
        2,
        VarType::UnitList,
        TriggerValue::UnitList(vec![enemy_unit, allied_unit]),
    );
    add_value(
        &mut script,
        3,
        VarType::SquadList,
        TriggerValue::SquadList(vec![enemy_squad]),
    );
    add_value(&mut script, 4, VarType::RelationType, TriggerValue::Int(3));
    add_value(&mut script, 5, VarType::Player, TriggerValue::Player(1));
    add_value(&mut script, 6, VarType::Team, TriggerValue::Team(1));
    add_value(
        &mut script,
        7,
        VarType::Unit,
        TriggerValue::Unit(stale_unit),
    );
    add_value(&mut script, 8, VarType::Player, TriggerValue::Player(99));

    let enemy_entities = diplomacy_condition(40, 1, 1)
        .with_input_at(4, 3)
        .with_input_at(5, 4)
        .with_input_at(6, 5);
    let mixed_teams = diplomacy_condition(41, 2, 2)
        .with_input_at(5, 4)
        .with_input_at(6, 5);
    let team_reference = diplomacy_condition(42, 4, 3)
        .with_input_at(5, 4)
        .with_input_at(7, 6);
    let stale_input = diplomacy_condition(43, 1, 7)
        .with_input_at(5, 4)
        .with_input_at(6, 5);
    let invalid_player_blocks_team_fallback = diplomacy_condition(44, 1, 1)
        .with_input_at(5, 4)
        .with_input_at(6, 8)
        .with_input_at(7, 6);
    let no_entities = Condition::new(45, ConditionType::CheckDiplomacy)
        .with_input_at(5, 4)
        .with_input_at(6, 5);

    for condition in [&enemy_entities, &team_reference, &stale_input] {
        assert_condition(condition, &mut script, &mut world, ConditionResult::True);
    }
    for condition in [
        &mixed_teams,
        &invalid_player_blocks_team_fallback,
        &no_entities,
    ] {
        assert_condition(condition, &mut script, &mut world, ConditionResult::False);
    }
}

#[test]
fn cost_and_lifetime_resource_conditions_distinguish_balance_from_total() {
    let mut world = World::new();
    world.init_players(1);
    let player = world.get_player_mut(1).unwrap();
    player.set_resource(0, 10.0);
    player.initialize_resource_totals();
    let initial_checksum = world.checksum();
    let player = world.get_player_mut(1).unwrap();
    player.add_resource(0, 5.0);
    player.resources.subtract(0, 5.0);
    assert!((player.get_resource(0) - 10.0).abs() < f32::EPSILON);
    assert!((player.get_total_resource(0) - 15.0).abs() < f32::EPSILON);
    assert_ne!(world.checksum(), initial_checksum);

    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_cost(&mut script, 2, 14.0, 0.0, 0.0);
    add_cost(&mut script, 3, 10.0, 2.0, 0.0);
    add_value(&mut script, 4, VarType::Operator, TriggerValue::Int(5));
    add_value(&mut script, 5, VarType::Bool, TriggerValue::Bool(true));
    add_cost(&mut script, 6, 5.0, 3.0, 0.0);
    let totals = Condition::new(50, ConditionType::CheckResourceTotals)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    let balance = Condition::new(51, ConditionType::CanPayCost)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    let comparison = Condition::new(52, ConditionType::CompareCost)
        .with_input_at(1, 3)
        .with_input_at(2, 4)
        .with_input_at(4, 6);
    let comparison_or = comparison.clone().with_input_at(3, 5);

    assert_condition(&totals, &mut script, &mut world, ConditionResult::True);
    assert_condition(&balance, &mut script, &mut world, ConditionResult::False);
    assert_condition(&comparison, &mut script, &mut world, ConditionResult::False);
    assert_condition(
        &comparison_or,
        &mut script,
        &mut world,
        ConditionResult::True,
    );
}

#[test]
fn tech_status_uses_database_context_and_retail_invalid_tech_fallback() {
    let mut database = Database::new();
    database.techs.push(Tech {
        name: "test_upgrade".to_owned(),
        ..Tech::default()
    });
    let mut world = World::new();
    world.init_players(1);
    let unit_id = world.create_unit(1);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(&mut script, 2, VarType::Tech, TriggerValue::Tech(0));
    add_value(&mut script, 3, VarType::TechStatus, TriggerValue::Int(2));
    add_value(&mut script, 4, VarType::Tech, TriggerValue::Tech(-1));
    add_value(&mut script, 5, VarType::TechStatus, TriggerValue::Int(0));
    add_value(&mut script, 6, VarType::TechStatus, TriggerValue::Int(4));
    add_value(&mut script, 7, VarType::Unit, TriggerValue::Unit(unit_id));
    let mut available = tech_status_condition(60, 2, 3);
    available.version = 1;

    assert_condition(&available, &mut script, &mut world, ConditionResult::False);
    assert_eq!(
        evaluate_condition_with_database(&available, 0, &mut script, &mut world, Some(&database)),
        ConditionResult::True
    );
    let mut invalid = tech_status_condition(61, 4, 5);
    invalid.version = 1;
    assert_eq!(
        evaluate_condition_with_database(&invalid, 0, &mut script, &mut world, Some(&database)),
        ConditionResult::True
    );

    assert!(
        world
            .activate_technology(1, &database, "test_upgrade")
            .unwrap()
    );
    let mut active = tech_status_condition(62, 2, 6).with_input_at(4, 7);
    active.version = 2;
    assert_eq!(
        evaluate_condition_with_database(&active, 0, &mut script, &mut world, Some(&database)),
        ConditionResult::True
    );
}

#[test]
fn under_attack_uses_nonzero_inclusive_squad_damage_times() {
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad(1);
    let unit_id = world.create_unit(1);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    world.game_time_ms = 100;
    let checksum_before = world.checksum();
    assert!(world.damage_unit(unit_id, 1.0));
    assert_eq!(world.get_squad(squad_id).unwrap().last_damaged_time, 100);
    assert_ne!(world.checksum(), checksum_before);
    world.advance_time(50);

    let stale = EntityId::new(EntityClass::Squad, 4_000);
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Squad,
        TriggerValue::Squad(squad_id),
    );
    add_value(&mut script, 2, VarType::Time, TriggerValue::Time(50));
    add_value(&mut script, 3, VarType::Time, TriggerValue::Time(49));
    add_value(
        &mut script,
        4,
        VarType::SquadList,
        TriggerValue::SquadList(vec![stale, squad_id]),
    );
    let mut version_one = under_attack_condition(70, 2).with_input_at(1, 1);
    version_one.version = 1;
    let mut too_old = under_attack_condition(71, 3).with_input_at(1, 1);
    too_old.version = 1;
    let mut version_two = under_attack_condition(72, 2).with_input_at(3, 4);
    version_two.version = 2;

    for condition in [&version_one, &version_two] {
        assert_condition(condition, &mut script, &mut world, ConditionResult::True);
    }
    assert_condition(&too_old, &mut script, &mut world, ConditionResult::False);
}

fn contained_world() -> (World, EntityId, EntityId) {
    let (world, container_squad, passenger_squad, _, _) = contained_world_with_units();
    (world, container_squad, passenger_squad)
}

fn contained_world_with_units() -> (World, EntityId, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "unsc_inf_marine_01".to_owned(),
        dbid: Some(17),
        object_types: vec!["Infantry".to_owned(), "UNSC".to_owned()],
        ..ProtoObject::default()
    });
    world.configure_prototype_catalogs(&database);
    world.init_players(2);
    let container_squad = world.create_squad_at(0, Vec3::ZERO);
    let container_unit = world.create_building_at(0, Vec3::ZERO);
    assert!(world.attach_unit_to_squad(container_unit, container_squad));
    world.get_unit_mut(container_unit).unwrap().garrison =
        UnitGarrison::container(0.0, false, false, Vec::new());
    let passenger_squad = world.create_squad_at(1, Vec3::ZERO);
    let passenger_unit = world.create_unit_at(1, Vec3::ZERO);
    assert!(world.attach_unit_to_squad(passenger_unit, passenger_squad));
    let passenger = world.get_unit_mut(passenger_unit).unwrap();
    passenger.proto_object_id = 17;
    passenger.proto_object_name = "unsc_inf_marine_01".to_owned();
    passenger.object_types = vec!["Infantry".to_owned(), "UNSC".to_owned()];
    world
        .issue_garrison_order(1, passenger_squad, container_squad, 0.0)
        .expect("garrison order");
    world.advance_time(50);
    world.update_entities(0.05);
    (
        world,
        container_squad,
        passenger_squad,
        container_unit,
        passenger_unit,
    )
}

fn roster_condition_fixture() -> (World, TriggerScript) {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "unsc_inf_marine_01".to_owned(),
        dbid: Some(17),
        object_types: vec!["Infantry".to_owned(), "UNSC".to_owned()],
        ..ProtoObject::default()
    });
    database.objects.push(ProtoObject {
        name: "unsc_bldg_barracks_01".to_owned(),
        dbid: Some(18),
        object_types: vec!["Building".to_owned(), "UNSC".to_owned()],
        ..ProtoObject::default()
    });
    database.squads.push(ProtoSquad {
        name: "unsc_marine_squad".to_owned(),
        dbid: Some(70),
        ..ProtoSquad::default()
    });
    database.squads.push(ProtoSquad {
        name: "unsc_warthog_squad".to_owned(),
        dbid: Some(71),
        ..ProtoSquad::default()
    });
    let mut world = World::new();
    world.configure_prototype_catalogs(&database);
    world.init_players(2);

    let marine = world.create_unit(1);
    set_unit_type(&mut world, marine, 17, "unsc_inf_marine_01", "Infantry");
    let producer = world.create_building(1);
    set_unit_type(
        &mut world,
        producer,
        18,
        "unsc_bldg_barracks_01",
        "Building",
    );
    enqueue_training(
        &mut world,
        producer,
        TrainingKind::Unit,
        17,
        "unsc_inf_marine_01",
    );
    enqueue_training(
        &mut world,
        producer,
        TrainingKind::Squad,
        0,
        "unsc_marine_squad",
    );
    let marine_squad = world.create_squad(1);
    world.get_squad_mut(marine_squad).unwrap().proto_squad_id = 70;
    let other_squad = world.create_squad(1);
    world.get_squad_mut(other_squad).unwrap().proto_squad_id = 71;

    (world, roster_script())
}

fn identity_condition_world() -> (World, EntityId) {
    let mut database = Database::new();
    database.squads.push(ProtoSquad {
        name: "unsc_marine_squad".to_owned(),
        dbid: Some(70),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                count: 2,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    });
    let mut world = World::new();
    world.configure_prototype_catalogs(&database);
    world.init_players(1);
    let player = world.get_player_mut(1).unwrap();
    player.civ_id = 4;
    player.leader_id = 7;
    player.state = PlayerState::Defeated;
    let squad_id = world.create_squad(1);
    world.get_squad_mut(squad_id).unwrap().proto_squad_id = 70;
    let first_member = world.create_unit(1);
    assert!(world.attach_unit_to_squad(first_member, squad_id));
    (world, squad_id)
}

fn diplomacy_condition_world() -> (World, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    world.init_players(3);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.get_player_mut(3).unwrap().team_id = 1;
    world.configure_standard_team_relations();
    let enemy_unit = world.create_unit(2);
    let allied_unit = world.create_unit(3);
    let enemy_squad = world.create_squad(2);
    (world, enemy_unit, allied_unit, enemy_squad)
}

fn identity_condition(id: i32, condition_type: ConditionType, identity_var: VarId) -> Condition {
    Condition::new(id, condition_type)
        .with_input_at(1, identity_var)
        .with_input_at(2, 4)
        .with_input_at(3, identity_var)
}

fn diplomacy_condition(id: i32, signature_id: u16, variable_id: VarId) -> Condition {
    Condition::new(id, ConditionType::CheckDiplomacy).with_input_at(signature_id, variable_id)
}

fn assert_condition(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
    expected: ConditionResult,
) {
    assert_eq!(evaluate_condition(condition, 0, script, world), expected);
}

fn roster_script() -> TriggerScript {
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(
        &mut script,
        2,
        VarType::ObjectType,
        TriggerValue::ObjectType("Infantry".to_owned()),
    );
    add_value(&mut script, 3, VarType::Operator, TriggerValue::Int(3));
    add_value(&mut script, 4, VarType::Integer, TriggerValue::Int(1));
    add_value(&mut script, 5, VarType::Bool, TriggerValue::Bool(true));
    add_value(&mut script, 6, VarType::Integer, TriggerValue::Int(2));
    add_value(
        &mut script,
        7,
        VarType::ProtoSquad,
        TriggerValue::ProtoSquad(70),
    );
    add_value(&mut script, 8, VarType::Integer, TriggerValue::Int(3));
    script
}

fn add_value(script: &mut TriggerScript, id: VarId, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn add_cost(script: &mut TriggerScript, id: VarId, supplies: f32, power: f32, population: f32) {
    add_value(
        script,
        id,
        VarType::Cost,
        TriggerValue::Cost(Cost {
            supplies,
            power,
            population,
            resource_3: 0.0,
        }),
    );
}

fn tech_status_condition(id: i32, tech_var: VarId, status_var: VarId) -> Condition {
    Condition::new(id, ConditionType::TechStatus)
        .with_input_at(1, 1)
        .with_input_at(2, tech_var)
        .with_input_at(3, status_var)
}

fn under_attack_condition(id: i32, interval_var: VarId) -> Condition {
    Condition::new(id, ConditionType::IsUnderAttack).with_input_at(2, interval_var)
}

fn roster_condition(id: i32, condition_type: ConditionType, expected_var: VarId) -> Condition {
    Condition::new(id, condition_type)
        .with_input_at(1, 1)
        .with_input_at(3, 3)
        .with_input_at(4, expected_var)
}

fn set_unit_type(
    world: &mut World,
    unit_id: EntityId,
    prototype_id: i32,
    prototype_name: &str,
    object_type: &str,
) {
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_id = prototype_id;
    unit.proto_object_name = prototype_name.to_owned();
    unit.object_types = vec![object_type.to_owned(), "UNSC".to_owned()];
}

fn enqueue_training(
    world: &mut World,
    producer_id: EntityId,
    kind: TrainingKind,
    prototype_id: i32,
    prototype_name: &str,
) {
    world
        .get_unit_mut(producer_id)
        .unwrap()
        .production
        .enqueue_training(TrainingTask {
            player_id: 1,
            kind,
            prototype_id,
            prototype_name: prototype_name.to_owned(),
            current_points: 0.0,
            total_points: 1.0,
            cost: Resources::default(),
            population_costs: Vec::new(),
            train_limit_bucket: None,
            trigger_state: None,
        });
}
