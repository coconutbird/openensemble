use super::*;
use crate::entities::HealPhase;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use glam::Vec3;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper, TechEffect};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad, Tech};

#[test]
fn self_heal_waits_one_action_update_and_ignores_unit_work_scalar() {
    let (mut world, database, gameplay, squad_id) = fixture(heal_action(10.0, 0.0));
    let (healer_id, patient_id) = members(&world, squad_id);
    world.get_unit_mut(healer_id).unwrap().work_rate_scalar = 100.0;
    world.get_unit_mut(patient_id).unwrap().hitpoints = 50.0;

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert_eq!(
        world.get_unit(healer_id).unwrap().heal_phase(),
        HealPhase::Working
    );
    assert_eq!(
        world.get_unit(healer_id).unwrap().heal_target(),
        Some(squad_id)
    );
    assert_close(world.get_unit(patient_id).unwrap().hitpoints, 50.0);

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert_close(world.get_unit(patient_id).unwrap().hitpoints, 50.5);
}

#[test]
fn a_disabled_working_action_freezes_until_live_enablement_returns() {
    let (mut world, database, gameplay, squad_id) = fixture(heal_action(10.0, 0.0));
    let (healer_id, patient_id) = members(&world, squad_id);
    world.get_unit_mut(patient_id).unwrap().hitpoints = 50.0;
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    world
        .get_unit_mut(healer_id)
        .unwrap()
        .actions
        .set_enabled("MedicHeal", false);

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert_eq!(
        world.get_unit(healer_id).unwrap().heal_phase(),
        HealPhase::Working
    );
    assert_close(world.get_unit(patient_id).unwrap().hitpoints, 50.0);

    world
        .get_unit_mut(healer_id)
        .unwrap()
        .actions
        .set_enabled("MedicHeal", true);
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert_close(world.get_unit(patient_id).unwrap().hitpoints, 50.5);
}

#[test]
fn player_technology_enables_and_scales_heal_work_rate() {
    let mut action = heal_action(10.0, 0.0);
    action.start_disabled = Some(true);
    let (mut world, mut database, gameplay, squad_id) = fixture(action);
    database.techs.push(heal_technology());
    let (healer_id, patient_id) = members(&world, squad_id);
    world.get_unit_mut(patient_id).unwrap().hitpoints = 50.0;

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert_eq!(
        world.get_unit(healer_id).unwrap().heal_phase(),
        HealPhase::Waiting
    );
    assert_eq!(
        world.activate_technology(1, &database, "FieldMedicine"),
        Ok(true)
    );
    update_twice(&mut world, &database, &gameplay);

    assert_close(world.get_unit(patient_id).unwrap().hitpoints, 51.0);
}

#[test]
fn idle_damage_and_attack_clocks_gate_the_heal_opportunity() {
    let (mut world, database, gameplay, squad_id) = fixture(heal_action(10.0, 1.0));
    let (healer_id, patient_id) = members(&world, squad_id);
    world.advance_time(1_000);
    assert!(world.damage_unit(patient_id, 20.0));

    for _ in 0..21 {
        tick(&mut world, &database, &gameplay);
    }
    assert_eq!(
        world.get_unit(healer_id).unwrap().heal_phase(),
        HealPhase::Waiting
    );
    tick(&mut world, &database, &gameplay);
    assert_eq!(
        world.get_unit(healer_id).unwrap().heal_phase(),
        HealPhase::Working
    );

    let before = world.get_unit(patient_id).unwrap().hitpoints;
    let now = world.game_time();
    world.get_squad_mut(squad_id).unwrap().last_attacked_time = now;
    tick(&mut world, &database, &gameplay);
    assert_eq!(
        world.get_unit(healer_id).unwrap().heal_phase(),
        HealPhase::Waiting
    );
    assert_close(world.get_unit(patient_id).unwrap().hitpoints, before);
}

#[test]
fn heal_target_follows_the_parent_join_target() {
    let mut action = heal_action(20.0, 0.0);
    action.heal_target = Some(true);
    let (mut world, database, gameplay, source_id) = fixture(action);
    let target_id = spawn(&mut world, &database, "patient_squad");
    let healer_id = members(&world, source_id).0;
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 25.0;
    world
        .get_squad_mut(source_id)
        .unwrap()
        .begin_join(target_id, None, true);

    world.update_heals(0.05, &database, &gameplay);
    assert_eq!(
        world.get_unit(healer_id).unwrap().heal_target(),
        Some(target_id)
    );
    world.update_heals(0.05, &database, &gameplay);
    assert_close(world.get_unit(target_unit_id).unwrap().hitpoints, 26.0);
}

#[test]
fn allow_reinforce_controls_missing_member_recreation() {
    let (mut no_world, no_database, no_gameplay, no_squad) = fixture(heal_action(200.0, 0.0));
    let missing_id = members(&no_world, no_squad).1;
    assert!(no_world.remove_unit(missing_id).is_some());
    update_twice(&mut no_world, &no_database, &no_gameplay);
    assert_eq!(no_world.get_squad(no_squad).unwrap().unit_ids.len(), 1);

    let mut reinforce = heal_action(200.0, 0.0);
    reinforce.allow_reinforce = Some(true);
    let (mut world, database, gameplay, squad_id) = fixture(reinforce);
    let missing_id = members(&world, squad_id).1;
    assert!(world.remove_unit(missing_id).is_some());
    update_twice(&mut world, &database, &gameplay);
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.unit_ids.len(), 2);
    let restored = world.get_unit(squad.unit_ids[1]).unwrap();
    assert_eq!(restored.proto_object_name, "patient_unit");
    assert_close(restored.hitpoints, 10.0);
}

fn fixture(action: Action) -> (World, Database, GameplayCatalog, EntityId) {
    let database = database();
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("healer_unit".to_owned(), heal_tactic(action))]);
    let mut world = World::new();
    world.init_players(1);
    world.configure_prototype_catalogs(&database);
    let squad_id = spawn(&mut world, &database, "medical_team");
    (world, database, gameplay, squad_id)
}

fn heal_action(work_rate: f32, min_idle_duration: f32) -> Action {
    Action {
        name: "MedicHeal".to_owned(),
        action_type: Some("Heal".to_owned()),
        work_rate: Some(work_rate),
        min_idle_duration: Some(min_idle_duration),
        ..Action::default()
    }
}

fn heal_tactic(action: Action) -> TacticData {
    TacticData {
        actions: vec![action],
        tactic: Some(TacticRules {
            persistent_actions: vec!["MedicHeal".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}

fn heal_technology() -> Tech {
    Tech {
        name: "FieldMedicine".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![
                heal_technology_effect("ActionEnable", 1.0, "Absolute"),
                heal_technology_effect("WorkRate", 2.0, "Percent"),
            ],
        }),
        ..Tech::default()
    }
}

fn heal_technology_effect(subtype: &str, amount: f32, relativity: &str) -> TechEffect {
    TechEffect {
        effect_type: "Data".to_owned(),
        subtype: Some(subtype.to_owned()),
        amount: Some(amount),
        relativity: Some(relativity.to_owned()),
        action: Some("MedicHeal".to_owned()),
        target: Some(EffectTarget {
            target_type: Some("ProtoUnit".to_owned()),
            value: Some("healer_unit".to_owned()),
        }),
        ..TechEffect::default()
    }
}

fn database() -> Database {
    Database {
        objects: vec![
            object("healer_unit", 10, true),
            object("patient_unit", 11, false),
        ],
        squads: vec![
            squad(
                "medical_team",
                20,
                &[("healer_unit", 1), ("patient_unit", 1)],
            ),
            squad("patient_squad", 21, &[("patient_unit", 1)]),
        ],
        ..Database::default()
    }
}

fn object(name: &str, dbid: i32, has_tactics: bool) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        tactics: has_tactics.then(|| "healer_unit.tactics".to_owned()),
        hitpoints: Some(100.0),
        combat_value: Some(10.0),
        ..ProtoObject::default()
    }
}

fn squad(name: &str, dbid: i32, units: &[(&str, i32)]) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        dbid: Some(dbid),
        units: Some(UnitsWrapper {
            entries: units
                .iter()
                .map(|(prototype, count)| UnitEntry {
                    proto_object: (*prototype).to_owned(),
                    count: *count,
                    ..UnitEntry::default()
                })
                .collect(),
        }),
        ..ProtoSquad::default()
    }
}

fn spawn(world: &mut World, database: &Database, prototype: &str) -> EntityId {
    let prototype_id = squad_prototype_id(database, prototype).unwrap();
    spawn_squad_at(world, database, 1, prototype_id, Vec3::ZERO, Vec3::Z).unwrap()
}

fn members(world: &World, squad_id: EntityId) -> (EntityId, EntityId) {
    let members = &world.get_squad(squad_id).unwrap().unit_ids;
    (members[0], members[1])
}

fn tick(world: &mut World, database: &Database, gameplay: &GameplayCatalog) {
    world.advance_time(50);
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}

fn update_twice(world: &mut World, database: &Database, gameplay: &GameplayCatalog) {
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= 0.000_1,
        "expected {left} to equal {right}"
    );
}
