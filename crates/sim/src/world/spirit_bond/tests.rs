use super::*;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use pipeline::database::hw1::Squad as ProtoSquad;
use pipeline::database::hw1::objects::ProtoObject;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{
    Action, DamageModifiers, ProtoObjectRef, TacticData, TacticRules,
};

#[test]
fn active_bond_buffs_exactly_two_members_rebuilds_beam_and_finishes_once() {
    let (database, gameplay) = fixture(false);
    let mut world = World::new();
    world.init_players(1);
    let squad_id = spawn_hunters(&mut world, &database);
    let members = world.get_squad(squad_id).unwrap().unit_ids.clone();

    advance(&mut world, &database, &gameplay);

    let squad = world.get_squad(squad_id).unwrap();
    assert!(squad.spirit_bond_active());
    let first_beam = squad.spirit_bond_beam().expect("bond beam");
    let beam = world.get_object(first_beam).unwrap();
    assert_eq!(beam.proto_object_name, "bond_beam");
    assert_eq!(
        beam.visual_secondary_position(),
        Some(world.get_unit(members[1]).unwrap().simulation_center())
    );
    for member_id in &members {
        assert_eq!(
            world
                .get_unit(*member_id)
                .unwrap()
                .spirit_bond_damage_multiplier()
                .to_bits(),
            1.5_f32.to_bits()
        );
    }

    assert!(world.remove_object(first_beam).is_some());
    advance(&mut world, &database, &gameplay);
    let replacement_beam = world
        .get_squad(squad_id)
        .unwrap()
        .spirit_bond_beam()
        .expect("removed beam should be recreated");
    assert_ne!(replacement_beam, first_beam);

    assert!(world.kill_unit(members[1], false));
    advance(&mut world, &database, &gameplay);
    assert!(!world.get_squad(squad_id).unwrap().spirit_bond_active());
    assert!(world.get_object(replacement_beam).is_none());
    assert_eq!(
        world
            .get_unit(members[0])
            .unwrap()
            .spirit_bond_damage_multiplier()
            .to_bits(),
        1.0_f32.to_bits()
    );

    let replacement_member = world.create_unit(1);
    assert!(world.attach_unit_to_squad(replacement_member, squad_id));
    advance(&mut world, &database, &gameplay);
    assert!(!world.get_squad(squad_id).unwrap().spirit_bond_active());
    assert!(
        world
            .get_squad(squad_id)
            .unwrap()
            .spirit_bond_beam()
            .is_none()
    );
}

#[test]
fn start_disabled_bond_waits_for_live_enablement() {
    let (database, gameplay) = fixture(true);
    let mut world = World::new();
    world.init_players(1);
    let squad_id = spawn_hunters(&mut world, &database);
    let leader_id = world.get_squad(squad_id).unwrap().unit_ids[0];

    advance(&mut world, &database, &gameplay);
    assert!(!world.get_squad(squad_id).unwrap().spirit_bond_active());
    assert_eq!(
        world
            .get_unit(leader_id)
            .unwrap()
            .spirit_bond_damage_multiplier()
            .to_bits(),
        1.0_f32.to_bits()
    );

    world
        .get_unit_mut(leader_id)
        .unwrap()
        .actions
        .set_enabled("Bond", true);
    advance(&mut world, &database, &gameplay);
    assert!(world.get_squad(squad_id).unwrap().spirit_bond_active());
}

#[test]
fn membership_disconnect_immediately_releases_the_bond() {
    let (database, gameplay) = fixture(false);
    let mut world = World::new();
    world.init_players(1);
    let squad_id = spawn_hunters(&mut world, &database);
    let members = world.get_squad(squad_id).unwrap().unit_ids.clone();
    advance(&mut world, &database, &gameplay);
    let beam_id = world
        .get_squad(squad_id)
        .unwrap()
        .spirit_bond_beam()
        .unwrap();

    assert!(world.detach_unit_from_squad(members[1]));
    assert!(!world.get_squad(squad_id).unwrap().spirit_bond_active());
    assert!(world.get_object(beam_id).is_none());
    for member_id in members {
        assert_eq!(
            world
                .get_unit(member_id)
                .unwrap()
                .spirit_bond_damage_multiplier()
                .to_bits(),
            1.0_f32.to_bits()
        );
    }
}

fn advance(world: &mut World, database: &Database, gameplay: &GameplayCatalog) {
    world.game_time_ms = world.game_time_ms.wrapping_add(50);
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}

fn spawn_hunters(world: &mut World, database: &Database) -> EntityId {
    let prototype = squad_prototype_id(database, "hunter_squad").unwrap();
    spawn_squad_at(world, database, 1, prototype, Vec3::ZERO, Vec3::Z).unwrap()
}

fn fixture(starts_disabled: bool) -> (Database, GameplayCatalog) {
    let database = Database {
        objects: vec![
            ProtoObject {
                name: "hunter".to_owned(),
                object_class: Some("Unit".to_owned()),
                tactics: Some("hunter.tactics".to_owned()),
                hitpoints: Some(100.0),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "bond_beam".to_owned(),
                dbid: Some(77),
                ..ProtoObject::default()
            },
        ],
        squads: vec![ProtoSquad {
            name: "hunter_squad".to_owned(),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "hunter".to_owned(),
                    count: 2,
                    ..UnitEntry::default()
                }],
            }),
            ..ProtoSquad::default()
        }],
        ..Database::default()
    };
    let tactics = TacticData {
        actions: vec![Action {
            name: "Bond".to_owned(),
            action_type: Some("SpiritBond".to_owned()),
            damage_modifiers: Some(DamageModifiers {
                damage: Some(1.5),
                ..DamageModifiers::default()
            }),
            proto_object: Some(ProtoObjectRef {
                name: "bond_beam".to_owned(),
                ..ProtoObjectRef::default()
            }),
            start_disabled: Some(starts_disabled),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_squad_actions: vec!["Bond".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("hunter".to_owned(), tactics)]);
    (database, gameplay)
}
