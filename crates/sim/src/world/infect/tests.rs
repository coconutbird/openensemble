use super::*;
use crate::entities::{InfectionPhase, SquadMode};
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use glam::Vec3;
use pipeline::database::hw1::gamedata::{InfectionMapEntry, InfectionMapWrapper};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, ProtoObjectRef, TacticData, TacticRules};
use pipeline::database::hw1::{Database, GameData, ProtoObject, Squad as ProtoSquad};

#[test]
fn exposure_visuals_freeze_while_disabled_and_disconnect_with_the_source() {
    let (mut world, database, gameplay, source_squad, target_squad) =
        fixture(infect_action(10.0, 15.0, 5.0), None);
    let source_id = leader(&world, source_squad);

    update(&mut world, &database, &gameplay, 0.05);
    let source = world.get_unit(source_id).unwrap();
    assert_eq!(source.infection_exposures().len(), 1);
    assert_eq!(source.infection_exposures()[0].squad_id(), target_squad);
    assert_eq!(source.infection_exposures()[0].visual_count(), 2);
    assert_eq!(world.objects.len(), 2);

    assert!(world.teleport_squad(target_squad, Vec3::X * 100.0));
    world
        .get_unit_mut(source_id)
        .unwrap()
        .actions
        .set_enabled("InfectAction", false);
    update(&mut world, &database, &gameplay, 0.25);
    assert_close(
        world.get_unit(source_id).unwrap().infection_exposures()[0].elapsed_seconds(),
        0.0,
    );
    assert_eq!(world.objects.len(), 2);

    world
        .get_unit_mut(source_id)
        .unwrap()
        .actions
        .set_enabled("InfectAction", true);
    update(&mut world, &database, &gameplay, 0.25);
    assert_close(
        world.get_unit(source_id).unwrap().infection_exposures()[0].elapsed_seconds(),
        0.25,
    );
    assert!(world.remove_unit(source_id).is_some());
    assert!(world.objects.is_empty());
}

#[test]
fn infection_preserves_unit_id_and_stages_gaia_before_final_ownership() {
    let (mut world, database, gameplay, source_squad, target_squad) =
        fixture(infect_action(10.0, 15.0, 0.1), None);
    let source_id = leader(&world, source_squad);
    let original_members = world.get_squad(target_squad).unwrap().unit_ids.clone();
    let converted_id = original_members[0];

    for _ in 0..6 {
        update(&mut world, &database, &gameplay, 0.05);
    }
    let marked = world.get_unit(converted_id).unwrap();
    assert_eq!(marked.infection_phase(), InfectionPhase::Marked);
    assert_eq!(marked.infection_player_id(), Some(1));
    assert_close(marked.hitpoints, 0.0);
    assert_eq!(world.get_unit(source_id).unwrap().infected_count(), 1);
    assert!(world.objects.is_empty());

    update(&mut world, &database, &gameplay, 0.05);
    let transformed = world.get_unit(converted_id).unwrap();
    let infected_squad = transformed.squad_id.unwrap();
    assert_eq!(transformed.infection_phase(), InfectionPhase::Transforming);
    assert_eq!(transformed.proto_object_name, "infected_victim");
    assert_eq!(transformed.base.player_id, GAIA_PLAYER);
    assert_close(transformed.hitpoints, 75.0);
    assert_ne!(infected_squad, target_squad);
    assert_eq!(
        world.get_squad(infected_squad).unwrap().proto_squad_name,
        "infected_single"
    );
    assert_eq!(
        world.get_squad(target_squad).unwrap().unit_ids,
        vec![original_members[1]]
    );

    update(&mut world, &database, &gameplay, 0.05);
    let converted = world.get_unit(converted_id).unwrap();
    assert_eq!(converted.infection_phase(), InfectionPhase::None);
    assert_eq!(converted.infection_player_id(), None);
    assert_eq!(converted.base.player_id, 1);
    assert_eq!(world.get_squad(infected_squad).unwrap().base.player_id, 1);
}

#[test]
fn conversion_limit_kills_the_infector_and_cleans_remaining_visuals() {
    let (mut world, database, gameplay, source_squad, target_squad) =
        fixture(infect_action(40.0, 15.0, 0.0), Some(1));
    let source_id = leader(&world, source_squad);
    let target_id = leader(&world, target_squad);

    update(&mut world, &database, &gameplay, 0.05);
    assert_eq!(world.objects.len(), 2);
    update(&mut world, &database, &gameplay, 0.05);

    assert!(
        world
            .get_squad(source_squad)
            .is_some_and(|squad| !squad.is_alive())
    );
    assert!(
        world
            .get_unit(source_id)
            .is_some_and(|unit| !unit.is_alive())
    );
    assert_eq!(
        world.get_unit(target_id).unwrap().infection_phase(),
        InfectionPhase::Marked
    );
    assert!(world.objects.is_empty());
}

#[test]
fn cover_turns_an_infection_mark_into_an_ordinary_death() {
    let (mut world, database, gameplay, _source_squad, target_squad) =
        fixture(infect_action(40.0, 15.0, 0.0), None);
    let target_id = leader(&world, target_squad);

    update(&mut world, &database, &gameplay, 0.05);
    update(&mut world, &database, &gameplay, 0.05);
    assert_eq!(
        world.get_unit(target_id).unwrap().infection_phase(),
        InfectionPhase::Marked
    );
    world.get_squad_mut(target_squad).unwrap().mode = SquadMode::Cover;
    update(&mut world, &database, &gameplay, 0.05);

    let target = world.get_unit(target_id).unwrap();
    assert!(!target.is_alive());
    assert_eq!(target.infection_phase(), InfectionPhase::None);
    assert_eq!(target.proto_object_name, "victim");
    assert!(
        !world
            .squads
            .iter()
            .any(|(_, squad)| squad.proto_squad_name == "infected_single")
    );
}

#[test]
fn invalid_target_types_prevent_initial_squad_exposure() {
    let mut action = infect_action(40.0, 15.0, 0.0);
    action.invalid_targets.push("Infantry".to_owned());
    let (mut world, database, gameplay, source_squad, _target_squad) = fixture(action, None);
    let source_id = leader(&world, source_squad);

    update(&mut world, &database, &gameplay, 0.05);

    assert!(
        world
            .get_unit(source_id)
            .unwrap()
            .infection_exposures()
            .is_empty()
    );
    assert!(world.objects.is_empty());
}

#[test]
fn infection_runtime_without_visuals_participates_in_the_world_checksum() {
    let mut action = infect_action(10.0, 15.0, 5.0);
    action.proto_object = None;
    let (mut world, database, gameplay, _source_squad, _target_squad) = fixture(action, None);
    let before = world.checksum();

    update(&mut world, &database, &gameplay, 0.05);

    assert!(world.objects.is_empty());
    assert_ne!(world.checksum(), before);
}

fn fixture(
    action: Action,
    conversion_limit: Option<i32>,
) -> (World, Database, GameplayCatalog, EntityId, EntityId) {
    let database = database(conversion_limit);
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("spore".to_owned(), infect_tactic(action))]);
    let mut world = World::new();
    world.init_players(2);
    world.configure_prototype_catalogs(&database);
    let source_squad = spawn(&mut world, &database, 1, "spore_squad", Vec3::ZERO);
    let target_squad = spawn(&mut world, &database, 2, "victim_squad", Vec3::X * 5.0);
    (world, database, gameplay, source_squad, target_squad)
}

fn infect_action(work_rate: f32, work_range: f32, min_idle_duration: f32) -> Action {
    Action {
        name: "InfectAction".to_owned(),
        action_type: Some("Infect".to_owned()),
        work_rate: Some(work_rate),
        work_range: Some(work_range),
        min_idle_duration: Some(min_idle_duration),
        proto_object: Some(ProtoObjectRef {
            name: "infection_fx".to_owned(),
            ..ProtoObjectRef::default()
        }),
        ..Action::default()
    }
}

fn infect_tactic(action: Action) -> TacticData {
    TacticData {
        actions: vec![action],
        tactic: Some(TacticRules {
            persistent_actions: vec!["InfectAction".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}

fn database(conversion_limit: Option<i32>) -> Database {
    Database {
        objects: vec![
            unit_object("spore", 1, 1.0, 20.0, true, conversion_limit),
            unit_object("victim", 2, 100.0, 2.0, false, None),
            unit_object("infected_victim", 3, 75.0, 3.0, false, None),
            ProtoObject {
                name: "infection_fx".to_owned(),
                dbid: Some(4),
                object_class: Some("Object".to_owned()),
                ..ProtoObject::default()
            },
        ],
        squads: vec![
            squad("spore_squad", 11, &[("spore", 1)]),
            squad("victim_squad", 12, &[("victim", 2)]),
            squad("infected_single", 13, &[("infected_victim", 1)]),
        ],
        game_data: Some(GameData {
            infection_map: Some(InfectionMapWrapper {
                entries: vec![InfectionMapEntry {
                    base: "victim".to_owned(),
                    infected: "infected_victim".to_owned(),
                    infected_squad: "infected_single".to_owned(),
                }],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn unit_object(
    name: &str,
    dbid: i32,
    hitpoints: f32,
    combat_value: f32,
    tactics: bool,
    num_conversions: Option<i32>,
) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Unit".to_owned()),
        object_types: vec!["Infantry".to_owned()],
        tactics: tactics.then(|| "spore.tactics".to_owned()),
        hitpoints: Some(hitpoints),
        combat_value: Some(combat_value),
        num_conversions,
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

fn spawn(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    prototype: &str,
    position: Vec3,
) -> EntityId {
    spawn_squad_at(
        world,
        database,
        player_id,
        squad_prototype_id(database, prototype).unwrap(),
        position,
        Vec3::Z,
    )
    .unwrap()
}

fn leader(world: &World, squad_id: EntityId) -> EntityId {
    world.get_squad(squad_id).unwrap().unit_ids[0]
}

fn update(world: &mut World, database: &Database, gameplay: &GameplayCatalog, dt: f32) {
    world.update_infections(dt, database, gameplay);
}

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= 0.000_1,
        "expected {left} to equal {right}"
    );
}
