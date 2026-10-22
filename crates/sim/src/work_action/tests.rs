use super::{DefaultWorkAction, default_work_action};
use crate::gameplay::GameplayCatalog;
use crate::{EntityId, World};
use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn action_types_map_like_the_retail_order_type_table() {
    let cases = [
        ("RangedAttack", Some(DefaultWorkAction::Attack)),
        ("handattack", Some(DefaultWorkAction::Attack)),
        ("SecondaryTurretAttack", Some(DefaultWorkAction::Attack)),
        ("Gather", Some(DefaultWorkAction::Gather)),
        ("Capture", Some(DefaultWorkAction::Capture)),
        ("Garrison", Some(DefaultWorkAction::Garrison)),
        ("Join", Some(DefaultWorkAction::Join)),
        ("Hitch", Some(DefaultWorkAction::Hitch)),
        ("RepairOther", Some(DefaultWorkAction::RepairOther)),
        ("Move", None),
        ("GaggleMove", None),
        ("Ungarrison", None),
        ("Unhitch", None),
        ("Mines", None),
    ];
    for (action_type, expected) in cases {
        assert_eq!(
            DefaultWorkAction::from_action_type(action_type),
            expected,
            "{action_type}"
        );
    }
}

#[test]
fn tactic_rules_pick_the_action_for_each_target() {
    let mut world = two_player_world();
    let source = squad_with_unit(&mut world, 1, "marine");
    let enemy = squad_with_unit(&mut world, 2, "enemy");
    let enemy_member = world.get_squad(enemy).unwrap().unit_ids[0];
    let own_building = squad_with_unit(&mut world, 1, "bunker");
    let own_other = squad_with_unit(&mut world, 1, "enemy");
    let supplies = squad_with_unit(&mut world, 0, "supplies");
    let gameplay = marine_catalog(marine_tactics());

    let action = |target| default_work_action(&world, &gameplay, source, target);
    assert_eq!(action(enemy), Some(DefaultWorkAction::Attack));
    assert_eq!(action(enemy_member), Some(DefaultWorkAction::Attack));
    assert_eq!(action(own_building), Some(DefaultWorkAction::Garrison));
    assert_eq!(action(supplies), Some(DefaultWorkAction::Gather));
    assert_eq!(action(own_other), None);
    assert_eq!(action(EntityId::INVALID), None);
}

#[test]
fn disabled_actions_are_skipped() {
    let mut world = two_player_world();
    let source = squad_with_unit(&mut world, 1, "marine");
    let enemy = squad_with_unit(&mut world, 2, "enemy");
    let mut tactics = marine_tactics();
    for action in &mut tactics.actions {
        action.start_disabled = Some(true);
    }
    let gameplay = marine_catalog(tactics);

    assert_eq!(default_work_action(&world, &gameplay, source, enemy), None);
}

#[test]
fn tactic_default_action_applies_when_no_rule_matches() {
    let mut world = two_player_world();
    let source = squad_with_unit(&mut world, 1, "marine");
    let own_other = squad_with_unit(&mut world, 1, "enemy");
    let mut tactics = marine_tactics();
    tactics.actions[0].default = Some(true);
    let gameplay = marine_catalog(tactics);

    assert_eq!(
        default_work_action(&world, &gameplay, source, own_other),
        Some(DefaultWorkAction::Attack)
    );
}

pub(crate) fn two_player_world() -> World {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world
}

pub(crate) fn squad_with_unit(world: &mut World, player_id: u8, proto: &str) -> EntityId {
    let squad_id = world.create_squad_at(player_id, Vec3::ZERO);
    let unit_id = world.create_unit_at(player_id, Vec3::ZERO);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    proto.clone_into(&mut world.get_unit_mut(unit_id).unwrap().proto_object_name);
    squad_id
}

pub(crate) fn marine_catalog(tactics: TacticData) -> GameplayCatalog {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "marine".to_owned(),
            tactics: Some("marine.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "enemy".to_owned(),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "bunker".to_owned(),
            object_types: vec!["Garrison".to_owned()],
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "supplies".to_owned(),
            object_types: vec!["Resource_Supplies".to_owned()],
            ..ProtoObject::default()
        },
    ]);
    GameplayCatalog::from_tactics(&database, [("marine".to_owned(), tactics)])
}

/// Rules in the order of the shipped `unsc_inf_marine_01` tactic.
pub(crate) fn marine_tactics() -> TacticData {
    let action = |name: &str, action_type: &str| Action {
        name: name.to_owned(),
        action_type: Some(action_type.to_owned()),
        ..Action::default()
    };
    let rule = |relation: Option<&str>, target_type: Option<&str>, action: &str| TargetRule {
        relation: relation.map(str::to_owned),
        target_types: target_type.map(str::to_owned).into_iter().collect(),
        gaia_owned: relation.is_none().then_some(true),
        action: Some(action.to_owned()),
        ..TargetRule::default()
    };
    TacticData {
        actions: vec![
            action("AssaultRifleAttackAction", "RangedAttack"),
            action("Garrison", "Garrison"),
            action("GatherSupplies", "Gather"),
        ],
        tactic: Some(TacticRules {
            target_rules: vec![
                rule(Some("Ally"), Some("Garrison"), "Garrison"),
                rule(Some("Enemy"), None, "AssaultRifleAttackAction"),
                rule(None, Some("Resource_Supplies"), "GatherSupplies"),
            ],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}
