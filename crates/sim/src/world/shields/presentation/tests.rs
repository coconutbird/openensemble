use super::*;
use crate::entities::ShieldCoverage;
use pipeline::database::hw1::tactics::{
    Action, ActionDuration, ProtoObjectRef, TacticData, TacticRules,
};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn external_shield_raises_hits_lowers_and_removes_its_attachment() {
    let (mut gameplay, mut world, unit_id) = fixture(external_tactics(false));
    gameplay.insert_test_scripted_animation_clip(
        "shield_fx",
        "Incoming",
        "art/shield_hit.uax",
        200,
    );
    gameplay.insert_test_scripted_animation_clip("shield_fx", "Death", "art/shield_death.uax", 300);

    world.update_entities_with_gameplay(0.05, &gameplay);
    let attachment_id = shield_action(&world, unit_id)
        .attachment_entity_id()
        .unwrap();
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Up
    );
    assert_eq!(world.get_object(attachment_id).unwrap().proto_object_id, 71);
    assert_eq!(animation_type(&world, attachment_id), "Idle");

    assert!(world.damage_unit(unit_id, 2.0));
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Hit
    );
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(animation_type(&world, attachment_id), "Incoming");
    assert_eq!(
        world
            .get_object(attachment_id)
            .unwrap()
            .object_state
            .scripted_animation()
            .unwrap()
            .asset_path(),
        Some("art/shield_hit.uax")
    );
    world.update_entities_with_gameplay(0.2, &gameplay);
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Up
    );
    assert_eq!(animation_type(&world, attachment_id), "Idle");

    assert!(world.damage_unit(unit_id, 20.0));
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Lowering
    );
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(animation_type(&world, attachment_id), "Death");
    world.update_entities_with_gameplay(0.3, &gameplay);
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Down
    );
    assert!(
        shield_action(&world, unit_id)
            .attachment_entity_id()
            .is_none()
    );
    assert!(world.get_object(attachment_id).is_none());
}

#[test]
fn disabled_external_shield_connects_then_raises_after_a_live_override() {
    let (gameplay, mut world, unit_id) = fixture(external_tactics(true));

    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Down
    );
    assert!(
        shield_action(&world, unit_id)
            .attachment_entity_id()
            .is_none()
    );

    world
        .get_unit_mut(unit_id)
        .unwrap()
        .actions
        .set_enabled("ShieldAction", true);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Up
    );
    assert!(
        shield_action(&world, unit_id)
            .attachment_entity_id()
            .is_some()
    );
}

#[test]
fn infantry_shield_owns_hidden_component_and_authored_hit_timer() {
    let (gameplay, mut world, unit_id) = fixture(infantry_tactics());

    world.update_entities_with_gameplay(0.05, &gameplay);
    let unit = world.get_unit(unit_id).unwrap();
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Down
    );
    assert!(!unit.visual_mesh_mask().is_component_visible("Shield"));

    assert!(world.damage_unit(unit_id, 1.0));
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Hit
    );
    assert_eq!(
        shield_action(&world, unit_id)
            .transition_seconds_remaining()
            .unwrap()
            .to_bits(),
        0.1_f32.to_bits()
    );
    world.update_entities_with_gameplay(0.1, &gameplay);
    assert_eq!(
        shield_action(&world, unit_id).phase(),
        EnergyShieldPhase::Down
    );
    assert!(
        !world
            .get_unit(unit_id)
            .unwrap()
            .visual_mesh_mask()
            .is_component_visible("shield")
    );
}

fn fixture(tactics: TacticData) -> (GameplayCatalog, World, EntityId) {
    let database = Database {
        objects: vec![
            ProtoObject {
                name: "shield_fx".to_owned(),
                dbid: Some(71),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "carrier".to_owned(),
                ..ProtoObject::default()
            },
        ],
        ..Database::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("carrier".to_owned(), tactics)]);
    let mut world = World::new();
    world.init_players(1);
    let unit_id = world.create_unit(1);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_name = "carrier".to_owned();
    unit.shields.configure(ShieldCoverage::Full, 10.0);
    unit.shields.set_current(10.0);
    (gameplay, world, unit_id)
}

fn external_tactics(starts_disabled: bool) -> TacticData {
    tactics(Action {
        name: "ShieldAction".to_owned(),
        action_type: Some("EnergyShield".to_owned()),
        proto_object: Some(ProtoObjectRef {
            name: "shield_fx".to_owned(),
            bone: Some("bone_chair".to_owned()),
            ..ProtoObjectRef::default()
        }),
        start_disabled: Some(starts_disabled),
        ..Action::default()
    })
}

fn infantry_tactics() -> TacticData {
    tactics(Action {
        name: "InfantryShield".to_owned(),
        action_type: Some("InfantryEnergyShield".to_owned()),
        duration: Some(ActionDuration {
            seconds: 0.1,
            ..ActionDuration::default()
        }),
        ..Action::default()
    })
}

fn tactics(action: Action) -> TacticData {
    let action_name = action.name.clone();
    TacticData {
        actions: vec![action],
        tactic: Some(TacticRules {
            persistent_actions: vec![action_name],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}

fn shield_action(world: &World, unit_id: EntityId) -> &UnitEnergyShieldAction {
    world
        .get_unit(unit_id)
        .unwrap()
        .shields
        .energy_shield_actions()
        .first()
        .unwrap()
}

fn animation_type(world: &World, entity_id: EntityId) -> &str {
    world
        .get_object(entity_id)
        .unwrap()
        .object_state
        .scripted_animation()
        .unwrap()
        .animation_type()
}
