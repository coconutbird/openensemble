use super::*;
use crate::gameplay::GameplayCatalog;
use crate::trigger::{AISquadAnalysisComponent, EffectType, TriggerVar, VarType};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule, Weapon};

fn fixture() -> (Database, GameplayCatalog, World, crate::EntityId) {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "test_unit".to_owned(),
        dbid: Some(11),
        tactics: Some("test_unit.tactics".to_owned()),
        damage_type: Some("Light".to_owned()),
        combat_value: Some(100.0),
        hitpoints: Some(100.0),
        attack_grade_dps: Some("10".to_owned()),
        ..ProtoObject::default()
    });
    database.squads.push(ProtoSquad {
        name: "test_squad".to_owned(),
        dbid: Some(21),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: "test_unit".to_owned(),
                count: 1,
                role: Some("normal".to_owned()),
            }],
        }),
        ..ProtoSquad::default()
    });
    let tactics = TacticData {
        weapons: vec![Weapon {
            name: "Gun".to_owned(),
            damage_per_second: Some(10.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "Attack".to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some("Gun".to_owned()),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                action: Some("Attack".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("test_unit".to_owned(), tactics)]);
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad(1);
    let unit_id = world.create_unit(1);
    {
        let squad = world.get_squad_mut(squad_id).unwrap();
        squad.proto_squad_id = 21;
        squad.proto_squad_name = "test_squad".to_owned();
    }
    {
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_id = 11;
        unit.proto_object_name = "test_unit".to_owned();
        unit.hitpoints = 50.0;
        unit.max_hitpoints = 100.0;
    }
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (database, gameplay, world, squad_id)
}

fn script_for_live_analysis(squad_id: crate::EntityId) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::SquadList).with_value(TriggerValue::SquadList(vec![squad_id])),
    );
    script.add_variable(
        TriggerVar::new(2, VarType::AISquadAnalysis)
            .with_value(TriggerValue::AISquadAnalysis(AISquadAnalysis::default())),
    );
    script.add_variable(
        TriggerVar::new(3, VarType::AISquadAnalysisComponent).with_value(
            TriggerValue::AISquadAnalysisComponent(AISquadAnalysisComponent::CVLight),
        ),
    );
    script.add_variable(TriggerVar::new(4, VarType::Float).with_value(TriggerValue::Float(0.0)));
    script
}

#[test]
fn live_analysis_scales_combat_value_by_current_health() {
    let (database, gameplay, world, squad_id) = fixture();
    let mut script = script_for_live_analysis(squad_id);
    let analyze = Effect::new(1, EffectType::AIAnalyzeSquadList)
        .with_input_at(1, 1)
        .with_output_at(2, 2);
    assert_eq!(
        analyze_squad_list(
            &analyze,
            &mut script,
            &world,
            Some(&database),
            Some(&gameplay),
        ),
        EffectOutcome::Applied
    );
    let component = Effect::new(2, EffectType::AISAGetComponent)
        .with_input_at(1, 2)
        .with_input_at(2, 3)
        .with_output_at(3, 4);
    assert_eq!(
        get_component(&component, &mut script),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(4).map(|variable| &variable.value),
        Some(&TriggerValue::Float(50.0))
    );
}

#[test]
fn prototype_analysis_uses_full_health_and_typed_copy_id_decodes() {
    let (database, gameplay, world, _) = fixture();
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::ProtoSquadList)
            .with_value(TriggerValue::ProtoSquadList(vec![21])),
    );
    script.add_variable(TriggerVar::new(2, VarType::Player).with_value(TriggerValue::Player(1)));
    script.add_variable(
        TriggerVar::new(3, VarType::AISquadAnalysis)
            .with_value(TriggerValue::AISquadAnalysis(AISquadAnalysis::default())),
    );
    let analyze = Effect::new(1, EffectType::AIAnalyzeProtoSquadList)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3);
    assert_eq!(
        analyze_proto_squad_list(
            &analyze,
            &mut script,
            &world,
            Some(&database),
            Some(&gameplay),
        ),
        EffectOutcome::Applied
    );
    let TriggerValue::AISquadAnalysis(analysis) = &script.get_variable(3).unwrap().value else {
        panic!("analysis output should stay typed");
    };
    assert!((analysis.component(AISquadAnalysisComponent::CVTotal) - 100.0).abs() < f32::EPSILON);
    assert_eq!(
        EffectType::from_u16(921),
        Some(EffectType::CopyAISquadAnalysis)
    );
}
