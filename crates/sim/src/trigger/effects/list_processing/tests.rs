use super::*;
use crate::trigger::{TriggerVar, VarType};

#[test]
fn proto_squad_shuffle_uses_the_shared_retail_swap_sequence() {
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::ProtoSquadList)
            .with_value(TriggerValue::ProtoSquadList(vec![10, 20, 30, 40])),
    );
    let effect = Effect::new(1, EffectType::ProtoSquadListShuffle).with_input_at(1, 1);
    let mut world = World::with_seed(1);

    assert_eq!(
        shuffle(&effect, &mut script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(1).unwrap().value,
        TriggerValue::ProtoSquadList(vec![20, 30, 10, 40])
    );
}

#[test]
fn every_retail_typed_shuffle_dbid_decodes() {
    let expected = [
        (298, EffectType::LocationListShuffle),
        (299, EffectType::EntityListShuffle),
        (300, EffectType::PlayerListShuffle),
        (301, EffectType::TeamListShuffle),
        (302, EffectType::UnitListShuffle),
        (303, EffectType::ProtoObjectListShuffle),
        (304, EffectType::ObjectTypeListShuffle),
        (305, EffectType::ProtoSquadListShuffle),
        (306, EffectType::TechListShuffle),
    ];
    for (raw_type, effect_type) in expected {
        assert_eq!(EffectType::from_u16(raw_type), Some(effect_type));
    }
}
