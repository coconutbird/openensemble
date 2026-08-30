use super::*;
use crate::entities::{CapturePhase, RepairOtherPhase};
use crate::gameplay::GameplayCatalog;
use crate::scenario::configure_unit_from_proto;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule};
use pipeline::database::hw1::{Ability, Database, ProtoObject, Squad as ProtoSquad};

#[test]
fn contextual_location_ability_issues_mines_instead_of_renderer_side_work() {
    let mut database = Database::new();
    database.abilities.extend([
        Ability {
            name: "Command".to_owned(),
            ..Ability::default()
        },
        Ability {
            name: "LayMines".to_owned(),
            objects: vec!["mine".to_owned()],
            ammo_cost: Some(1.0),
            ..Ability::default()
        },
    ]);
    database.objects.push(ProtoObject {
        name: "minelayer".to_owned(),
        tactics: Some("minelayer.tactics".to_owned()),
        ability_command: Some("LayMines".to_owned()),
        ..ProtoObject::default()
    });
    let tactics = TacticData {
        actions: vec![Action {
            name: "PlaceMine".to_owned(),
            action_type: Some("Mines".to_owned()),
            work_range: Some(4.0),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                relation: Some("Any".to_owned()),
                action: Some("PlaceMine".to_owned()),
                ability: Some("Command".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("minelayer".to_owned(), tactics)]);
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let unit_id = world.create_unit_at(1, Vec3::ZERO);
    world.get_unit_mut(unit_id).unwrap().proto_object_name = "minelayer".to_owned();
    assert!(world.attach_unit_to_squad(unit_id, squad_id));

    let order =
        resolve_contextual_location_work(&world, squad_id, Vec3::ZERO, true, Some(&gameplay));
    assert!(issue_contextual_work(
        &mut world,
        1,
        squad_id,
        order,
        false,
        false,
        WorkAssets {
            database: Some(&database),
            gameplay: Some(&gameplay),
        },
    ));
    assert!(world.get_squad(squad_id).unwrap().is_placing_mines());
}

#[test]
fn contextual_entity_work_issues_capture_through_the_simulation() {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "capturer".to_owned(),
            tactics: Some("capturer.tactics".to_owned()),
            hitpoints: Some(100.0),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "capture_node".to_owned(),
            object_class: Some("Building".to_owned()),
            build_points: Some(20.0),
            hitpoints: Some(100.0),
            flags: vec!["Capturable".to_owned(), "Invulnerable".to_owned()],
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        actions: vec![Action {
            name: "Capture".to_owned(),
            action_type: Some("Capture".to_owned()),
            work_rate: Some(1.0),
            work_range: Some(0.1),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("capturer".to_owned(), tactics)]);
    let mut world = World::new();
    world.init_players(2);
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let unit_id = world.create_unit_at(1, Vec3::ZERO);
    configure_unit_from_proto(&mut world, unit_id, "capturer", 0, &database.objects[0]);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    let target_id = world.create_building_at(0, Vec3::ZERO);
    configure_unit_from_proto(
        &mut world,
        target_id,
        "capture_node",
        1,
        &database.objects[1],
    );

    let order = resolve_contextual_work(&world, squad_id, target_id, false, Some(&gameplay));
    assert!(issue_contextual_work(
        &mut world,
        1,
        squad_id,
        order,
        false,
        false,
        WorkAssets {
            database: Some(&database),
            gameplay: Some(&gameplay),
        },
    ));
    assert_eq!(
        world.get_squad(squad_id).unwrap().capture_phase(),
        CapturePhase::Moving
    );
    assert_eq!(
        world.get_squad(squad_id).unwrap().capture_target(),
        Some(target_id)
    );
}

#[test]
fn contextual_entity_work_issues_repair_other_through_the_simulation() {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "repairer".to_owned(),
            tactics: Some("repairer.tactics".to_owned()),
            hitpoints: Some(100.0),
            combat_value: Some(10.0),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "vehicle".to_owned(),
            hitpoints: Some(100.0),
            combat_value: Some(10.0),
            object_types: vec!["Military".to_owned()],
            ..ProtoObject::default()
        },
    ]);
    database.squads.extend([
        repair_squad("repair_source", 10, "repairer"),
        repair_squad("repair_target", 11, "vehicle"),
    ]);
    let tactics = TacticData {
        actions: vec![Action {
            name: "RepairOther".to_owned(),
            action_type: Some("RepairOther".to_owned()),
            work_rate: Some(1.0),
            work_range: Some(3.0),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                relation: Some("Ally".to_owned()),
                action: Some("RepairOther".to_owned()),
                target_types: vec!["Military".to_owned()],
                target_states: vec!["Damaged".to_owned()],
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("repairer".to_owned(), tactics)]);
    let mut world = World::new();
    world.init_players(1);
    world.configure_prototype_catalogs(&database);
    let source_id = spawn_repair_squad(&mut world, &database, "repair_source");
    let target_id = spawn_repair_squad(&mut world, &database, "repair_target");
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 50.0;

    let order = resolve_contextual_work(&world, source_id, target_unit_id, false, Some(&gameplay));
    assert!(issue_contextual_work(
        &mut world,
        1,
        source_id,
        order,
        false,
        false,
        WorkAssets {
            database: Some(&database),
            gameplay: Some(&gameplay),
        },
    ));
    let source = world.get_squad(source_id).unwrap();
    assert_eq!(source.repair_other_phase(), RepairOtherPhase::Moving);
    assert_eq!(source.repair_other_target(), Some(target_id));
}

fn repair_squad(name: &str, dbid: i32, object: &str) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        dbid: Some(dbid),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: object.to_owned(),
                count: 1,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn spawn_repair_squad(world: &mut World, database: &Database, name: &str) -> EntityId {
    let prototype_id = squad_prototype_id(database, name).unwrap();
    spawn_squad_at(world, database, 1, prototype_id, Vec3::ZERO, Vec3::Z).unwrap()
}
