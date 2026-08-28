use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};
use pipeline::database::hw1::Power;
use pipeline::database::hw1::powers::PowerAttributes;

fn variable(id: u32, var_type: VarType, value: TriggerValue) -> TriggerVar {
    TriggerVar::new(id, var_type)
        .with_name(format!("v{id}"))
        .with_value(value)
}

fn database() -> Database {
    Database {
        powers: vec![Power {
            name: "UnscLeaderCarpetBombing".to_owned(),
            attributes: Some(PowerAttributes {
                infinite_uses: Some(true),
                ..PowerAttributes::default()
            }),
            ..Power::default()
        }],
        ..Database::default()
    }
}

fn script() -> TriggerScript {
    let mut script = TriggerScript::new(1).with_name("powers");
    script.add_variable(variable(1, VarType::Player, TriggerValue::Player(1)));
    script.add_variable(variable(
        2,
        VarType::Power,
        TriggerValue::String("UnscLeaderCarpetBombing".to_owned()),
    ));
    script.add_variable(variable(3, VarType::Integer, TriggerValue::Int(4)));
    script.add_variable(variable(
        4,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![1, 2]),
    ));
    script.add_variable(variable(5, VarType::Integer, TriggerValue::Int(2)));
    script.add_variable(variable(6, VarType::Bool, TriggerValue::Bool(true)));
    script.add_variable(variable(7, VarType::Bool, TriggerValue::Bool(true)));
    script.add_variable(variable(8, VarType::Bool, TriggerValue::Bool(true)));
    script
}

fn grant_effect(bind_squad: Option<u32>) -> Effect {
    let mut effect = Effect::new(1, EffectType::PowerGrant);
    effect.version = 3;
    for &(signature_id, variable_id) in &[
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 5),
        (6, 6),
        (7, 7),
        (8, 8),
    ] {
        effect = effect.with_input_at(signature_id, variable_id);
    }
    if let Some(variable_id) = bind_squad {
        effect = effect.with_input_at(9, variable_id);
    }
    effect
}

#[test]
fn version_three_grant_unions_players_and_preserves_every_option() {
    let database = database();
    let mut world = World::new();
    world.init_players(2);
    let squad_id = world.create_squad(1);
    let mut script = script();
    script.add_variable(variable(9, VarType::Squad, TriggerValue::Squad(squad_id)));

    assert_eq!(
        grant(&grant_effect(Some(9)), &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    for player_id in [1, 2] {
        let entry = world.get_player(player_id).unwrap().power_entry(0).unwrap();
        assert_eq!(entry.icon_location(), 4);
        assert!(entry.ignores_cost());
        assert!(entry.ignores_tech_prerequisites());
        assert!(entry.ignores_population());
        assert_eq!(entry.items()[0].squad_id(), squad_id);
        assert!(entry.has_available_uses());
    }
}

#[test]
fn revoke_removes_only_the_requested_squad_source() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let first = world.create_squad(1);
    let second = world.create_squad(1);
    for squad_id in [first, second] {
        let mut script = script();
        script.get_variable_mut(3).unwrap().value = TriggerValue::Int(-1);
        script.add_variable(variable(9, VarType::Squad, TriggerValue::Squad(squad_id)));
        assert_eq!(
            grant(&grant_effect(Some(9)), &script, &mut world, Some(&database)),
            EffectOutcome::Applied
        );
    }
    let mut script = script();
    script.get_variable_mut(3).unwrap().value = TriggerValue::Int(-1);
    script.add_variable(variable(9, VarType::Squad, TriggerValue::Squad(first)));
    let mut effect = Effect::new(2, EffectType::PowerRevoke)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(5, 9);
    effect.version = 3;

    assert_eq!(
        revoke(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let entry = world.get_player(1).unwrap().power_entry(0).unwrap();
    assert_eq!(entry.items().len(), 1);
    assert_eq!(entry.items()[0].squad_id(), second);
}

#[test]
fn empty_resolved_player_list_is_an_applied_noop_and_unknown_version_is_visible() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let mut script = script();
    script.get_variable_mut(4).unwrap().value = TriggerValue::PlayerList(Vec::new());
    let mut effect = grant_effect(None);
    effect.inputs.retain(|binding| binding.signature_id != 1);
    assert_eq!(
        grant(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    assert!(world.get_player(1).unwrap().power_entries().is_empty());

    effect.version = 1;
    assert_eq!(
        grant(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Unsupported(456)
    );
}
