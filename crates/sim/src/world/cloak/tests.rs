use super::*;
use crate::entities::UnitDataScalar;
use pipeline::database::hw1::tactics::{Action, ProtoObjectRef, TacticData, TacticRules};
use pipeline::database::hw1::{Ability, Database, GameData, ProtoObject};

const COMMAND_ID: u8 = 0;
const CLOAK_ID: u8 = 1;

#[test]
fn manual_cloak_owns_delay_modifiers_detection_visibility_and_recovery() {
    let (database, gameplay) = cloak_catalog(false, false, true);
    let (mut world, attacker_squad, target_squad, target_unit) = cloak_world();
    assert!(world.issue_cloak_order(2, target_squad, Some(COMMAND_ID)));

    update(&mut world, 0.5, &database, &gameplay);
    assert!(!world.get_squad(target_squad).unwrap().is_cloaked());
    update(&mut world, 0.5, &database, &gameplay);
    assert!(world.get_squad(target_squad).unwrap().is_cloaked());
    let target = world.get_unit(target_unit).unwrap();
    assert_eq!(
        target.data_scalar(UnitDataScalar::DamageTaken).to_bits(),
        0.5_f32.to_bits()
    );
    assert_eq!(target.dodge_scalar.to_bits(), 0.25_f32.to_bits());
    assert_eq!(
        target.visual_mesh_mask().section_overrides(),
        &[Some(false), Some(true)]
    );
    let effect_id = cloak_effect(&world);

    world.set_fog_of_war_enabled(false);
    assert!(!world.is_entity_visible_to_team(1, target_unit));
    assert!(!world.is_entity_visible_to_team(1, effect_id));
    assert!(world.is_entity_visible_to_team(2, target_unit));
    assert!(!world.issue_attack_order(1, attacker_squad, target_squad, 20.0));

    let health_before = world.get_unit(target_unit).unwrap().hitpoints;
    let _dealt = world.apply_weapon_damage(1, target_unit, 10.0, None, Some(&gameplay));
    assert_eq!(
        world.get_unit(target_unit).unwrap().hitpoints.to_bits(),
        (health_before - 5.0).to_bits()
    );
    assert!(world.get_squad(target_squad).unwrap().is_cloak_detected());
    assert!(world.is_entity_visible_to_team(1, target_unit));
    assert!(world.is_entity_visible_to_team(1, effect_id));
    assert!(world.issue_attack_order(1, attacker_squad, target_squad, 20.0));

    update(&mut world, 5.1, &database, &gameplay);
    assert!(world.get_squad(target_squad).unwrap().is_cloaked());
    assert!(!world.get_squad(target_squad).unwrap().is_cloak_detected());
    update(&mut world, 3.0, &database, &gameplay);
    let squad = world.get_squad(target_squad).unwrap();
    assert!(!squad.is_cloaked());
    assert_eq!(squad.recovery.recovery_type(), Some(RecoveryType::Ability));
    assert_eq!(squad.recovery.ability_id(), Some(CLOAK_ID));
    assert!(squad.recovery.remaining() > 0.0);
    let target = world.get_unit(target_unit).unwrap();
    assert_eq!(
        target.data_scalar(UnitDataScalar::DamageTaken).to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(target.dodge_scalar.to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        target.visual_mesh_mask().section_overrides(),
        &[Some(true), Some(false)]
    );
    assert!(world.get_object(effect_id).is_none());
}

#[test]
fn start_disabled_permanent_cloak_activates_from_live_action_enablement() {
    let (database, gameplay) = cloak_catalog(true, true, false);
    let (mut world, _attacker_squad, target_squad, target_unit) = cloak_world();

    update(&mut world, 0.05, &database, &gameplay);
    assert!(!world.get_squad(target_squad).unwrap().is_cloaked());
    world
        .get_unit_mut(target_unit)
        .unwrap()
        .actions
        .set_enabled("Stealth", true);
    update(&mut world, 0.05, &database, &gameplay);
    assert!(
        world
            .get_squad(target_squad)
            .unwrap()
            .is_permanently_cloaked()
    );
    assert!(!world.get_squad(target_squad).unwrap().wants_to_cloak());

    assert!(world.issue_move_order(2, target_squad, glam::Vec3::X * 20.0));
    update(&mut world, 0.5, &database, &gameplay);
    assert!(
        world
            .get_squad(target_squad)
            .unwrap()
            .is_permanently_cloaked()
    );
    assert!(
        !world
            .get_squad(target_squad)
            .unwrap()
            .recovery
            .is_recovering()
    );
}

#[test]
fn membership_change_disconnects_effects_and_exactly_reverses_scalars() {
    let (database, gameplay) = cloak_catalog(false, false, true);
    let (mut world, _attacker_squad, target_squad, target_unit) = cloak_world();
    assert!(world.issue_cloak_order(2, target_squad, None));
    update(&mut world, 1.0, &database, &gameplay);
    let effect_id = cloak_effect(&world);

    assert!(world.detach_unit_from_squad(target_unit));

    let target = world.get_unit(target_unit).unwrap();
    assert_eq!(
        target.data_scalar(UnitDataScalar::DamageTaken).to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(target.dodge_scalar.to_bits(), 1.0_f32.to_bits());
    assert!(world.get_object(effect_id).is_none());
}

fn cloak_catalog(
    starts_disabled: bool,
    permanent: bool,
    move_while_cloaked: bool,
) -> (Database, GameplayCatalog) {
    let mut flags = Vec::new();
    if move_while_cloaked {
        flags.push("MoveWhileCloaked".to_owned());
    }
    let database = Database {
        objects: vec![
            ProtoObject {
                name: "commando".to_owned(),
                ability_command: Some("StealthAbility".to_owned()),
                flags,
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "fx_cloak".to_owned(),
                object_class: Some("Object".to_owned()),
                dbid: Some(91),
                ..ProtoObject::default()
            },
        ],
        abilities: vec![
            Ability {
                name: "Command".to_owned(),
                ..Ability::default()
            },
            Ability {
                name: "StealthAbility".to_owned(),
                duration: Some(8.0),
                recover_type: Some("Ability".to_owned()),
                recover_time: Some(60.0),
                damage_taken_modifier: Some(0.5),
                dodge_modifier: Some(0.25),
                ..Ability::default()
            },
        ],
        game_data: Some(GameData {
            cloaking_delay: Some(1.0),
            recloak_delay: Some(5.0),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let tactics = TacticData {
        actions: vec![Action {
            name: "Stealth".to_owned(),
            action_type: Some("Cloak".to_owned()),
            no_auto_target: Some(permanent),
            start_disabled: Some(starts_disabled),
            proto_object: Some(ProtoObjectRef {
                name: "fx_cloak".to_owned(),
                ..ProtoObjectRef::default()
            }),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_squad_actions: vec!["Stealth".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("commando".to_owned(), tactics)]);
    (database, gameplay)
}

fn cloak_world() -> (World, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let attacker_squad = squad_with_unit(&mut world, 1, "attacker", glam::Vec3::ZERO).0;
    let (target_squad, target_unit) =
        squad_with_unit(&mut world, 2, "commando", glam::Vec3::X * 5.0);
    (world, attacker_squad, target_squad, target_unit)
}

fn squad_with_unit(
    world: &mut World,
    player_id: PlayerId,
    proto_object_name: &str,
    position: glam::Vec3,
) -> (EntityId, EntityId) {
    let squad_id = world.create_squad_at(player_id, position);
    let unit_id = world.create_unit_at(player_id, position);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_name = proto_object_name.to_owned();
    unit.set_max_hitpoints(100.0);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

fn cloak_effect(world: &World) -> EntityId {
    world
        .objects
        .iter()
        .find_map(|(id, object)| {
            object
                .proto_object_name
                .eq_ignore_ascii_case("fx_cloak")
                .then_some(id)
        })
        .expect("active cloak effect attachment")
}

fn update(world: &mut World, dt: f32, database: &Database, gameplay: &GameplayCatalog) {
    world.update_entities_with_database_and_gameplay(dt, database, gameplay);
}
