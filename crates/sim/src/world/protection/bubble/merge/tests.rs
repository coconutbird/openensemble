use super::*;
use crate::entities::{SquadArchetype, SquadFormation};
use crate::player::PopulationCost;
use crate::scenario::create_squad_from_prototype;
use glam::Vec3;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, DamageModifiers, JoinType, TacticData};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use pipeline::xmb::Document;

#[test]
fn merge_moves_the_joiner_transforms_the_target_and_preserves_population() {
    let (database, gameplay) = merge_gameplay();
    let (mut world, source_id, target_id, joining_unit_id, marine_ids) = merge_world(&database);
    let source_population = vec![PopulationCost::new(0, 1.0)];
    let target_population = vec![PopulationCost::new(0, 2.0)];
    world.get_squad_mut(source_id).unwrap().population_costs = source_population.clone();
    world.get_squad_mut(target_id).unwrap().population_costs = target_population.clone();

    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert!(world.get_squad(source_id).is_none());
    let target = world.get_squad(target_id).unwrap();
    assert_eq!(target.proto_squad_name, "merged_marine_squad_spartan_squad");
    assert_eq!(target.proto_squad_id, 2);
    assert_eq!(target.unit_ids.len(), 3);
    assert!(target.contains_unit(joining_unit_id));
    assert_eq!(
        target.merge_state().unwrap().joining_unit_id(),
        joining_unit_id
    );
    assert_eq!(
        target.population_costs,
        [target_population, source_population].concat()
    );
    assert_eq!(
        world.get_unit(joining_unit_id).unwrap().squad_id,
        Some(target_id)
    );
    assert!(nearly_equal(
        world
            .get_unit(joining_unit_id)
            .unwrap()
            .effective_damage_multiplier(),
        1.0
    ));
    for marine_id in &marine_ids {
        let marine = world.get_unit(*marine_id).unwrap();
        assert!(nearly_equal(marine.effective_damage_multiplier(), 1.4));
        assert!(nearly_equal(
            marine.effective_damage_taken_multiplier(),
            0.714
        ));
    }
}

#[test]
fn merge_reverts_to_the_joining_profile_when_only_the_joiner_survives() {
    let (database, gameplay) = merge_gameplay();
    let (mut world, source_id, target_id, joining_unit_id, marine_ids) = merge_world(&database);
    {
        let source = world.get_squad_mut(source_id).unwrap();
        source.archetype = SquadArchetype::Warthog;
        source.formation = SquadFormation::Flock;
        source.population_costs = vec![PopulationCost::new(0, 1.0)];
    }
    world.get_squad_mut(target_id).unwrap().population_costs = vec![PopulationCost::new(0, 2.0)];
    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);

    for marine_id in marine_ids {
        let _removed = world.remove_unit(marine_id);
    }
    world.update_entities_with_gameplay(0.05, &gameplay);

    let squad = world.get_squad(target_id).unwrap();
    assert_eq!(squad.unit_ids, vec![joining_unit_id]);
    assert_eq!(squad.proto_squad_name, "spartan_squad");
    assert_eq!(squad.proto_squad_id, 0);
    assert!(squad.merge_state().is_none());
    assert_eq!(squad.population_costs, vec![PopulationCost::new(0, 1.0)]);
    assert_eq!(squad.archetype, SquadArchetype::Warthog);
    assert_eq!(squad.formation, SquadFormation::Flock);
}

#[test]
fn merge_reverts_target_profile_and_removes_buffs_when_the_joiner_dies() {
    let (database, gameplay) = merge_gameplay();
    let (mut world, source_id, target_id, joining_unit_id, marine_ids) = merge_world(&database);
    world.get_squad_mut(source_id).unwrap().population_costs = vec![PopulationCost::new(0, 1.0)];
    world.get_squad_mut(target_id).unwrap().population_costs = vec![PopulationCost::new(0, 2.0)];
    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert!(world.kill_unit(joining_unit_id, false));
    world.update_entities_with_gameplay(0.05, &gameplay);

    let target = world.get_squad(target_id).unwrap();
    assert_eq!(target.proto_squad_name, "marine_squad");
    assert_eq!(target.proto_squad_id, 1);
    assert!(target.merge_state().is_none());
    assert_eq!(target.population_costs, vec![PopulationCost::new(0, 2.0)]);
    for marine_id in marine_ids {
        let marine = world.get_unit(marine_id).unwrap();
        assert!(nearly_equal(marine.effective_damage_multiplier(), 1.0));
        assert!(nearly_equal(
            marine.effective_damage_taken_multiplier(),
            1.0
        ));
    }
}

#[test]
fn merge_derives_buffs_from_authored_proto_combat_values() {
    let (mut database, _) = merge_gameplay();
    database.objects[0].combat_value = Some(30.0);
    database.objects[1].combat_value = Some(20.0);
    let gameplay = merge_catalog(
        &database,
        DamageModifiers {
            damage: Some(0.8),
            damage_taken: Some(2.0),
            by_combat_value: Some(true),
        },
    );
    let (mut world, source_id, target_id, joining_unit_id, marine_ids) = merge_world(&database);

    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert!(nearly_equal(
        world
            .get_unit(joining_unit_id)
            .unwrap()
            .effective_damage_multiplier(),
        1.0
    ));
    for marine_id in marine_ids {
        let marine = world.get_unit(marine_id).unwrap();
        assert!(nearly_equal(marine.effective_damage_multiplier(), 1.6));
        assert!(nearly_equal(
            marine.effective_damage_taken_multiplier(),
            1.0 / 1.375
        ));
    }
}

fn merge_world(database: &Database) -> (World, EntityId, EntityId, EntityId, Vec<EntityId>) {
    let mut world = World::new();
    world.init_players(1);
    let target_id =
        create_squad_from_prototype(&mut world, 1, Vec3::ZERO, Vec3::Z, "marine_squad", database);
    let source_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::ZERO,
        Vec3::Z,
        "spartan_squad",
        database,
    );
    let joining_unit_id = world.get_squad(source_id).unwrap().unit_ids[0];
    let marine_ids = world.get_squad(target_id).unwrap().unit_ids.clone();
    (world, source_id, target_id, joining_unit_id, marine_ids)
}

fn merge_gameplay() -> (Database, GameplayCatalog) {
    let database = Database {
        objects: vec![
            ProtoObject {
                name: "spartan".to_owned(),
                tactics: Some("spartan.tactics".to_owned()),
                hitpoints: Some(100.0),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "marine".to_owned(),
                hitpoints: Some(100.0),
                ..ProtoObject::default()
            },
        ],
        squads: vec![
            proto_squad("spartan_squad", "spartan", 1),
            proto_squad("marine_squad", "marine", 2),
        ],
        ..Database::default()
    };
    let gameplay = merge_catalog(
        &database,
        DamageModifiers {
            damage: Some(1.4),
            damage_taken: Some(0.714),
            ..DamageModifiers::default()
        },
    );
    (database, gameplay)
}

fn merge_catalog(database: &Database, damage_modifiers: DamageModifiers) -> GameplayCatalog {
    let tactics = TacticData {
        actions: vec![Action {
            name: "InfantryJoin".to_owned(),
            action_type: Some("Join".to_owned()),
            work_range: Some(1.0),
            join_type: Some(JoinType {
                kind: "Merge".to_owned(),
                ..JoinType::default()
            }),
            merge_type: Some("Ground".to_owned()),
            damage_modifiers: Some(damage_modifiers),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    let mut gameplay = GameplayCatalog::from_tactics(database, [("spartan".to_owned(), tactics)]);
    gameplay.load_test_merged_squads_document(
        database,
        &Document::from_xml(
            "<Squads><MergedSquads>spartan_squad<MergedSquad>marine_squad</MergedSquad>\
             </MergedSquads></Squads>",
        )
        .unwrap(),
    );
    gameplay
}

fn proto_squad(name: &str, member: &str, count: i32) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: member.to_owned(),
                count,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() < f32::EPSILON
}
