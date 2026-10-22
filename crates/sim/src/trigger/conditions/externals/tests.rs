use super::*;
use crate::EntityId;
use crate::entity_id::EntityClass;
use crate::trigger::{ConditionType, TriggerExternals, TriggerVar, TriggerVec3, VarType};

fn retrieve(version: u8) -> Condition {
    let mut condition = Condition::new(0, ConditionType::CanRetrieveExternals)
        .with_output_at(1, 0)
        .with_output_at(3, 1)
        .with_output_at(4, 2);
    condition.version = version;
    condition
}

fn script_with_outputs(externals: TriggerExternals) -> TriggerScript {
    let mut script = TriggerScript::new(0);
    script.add_variable(TriggerVar::new(0, VarType::Player).with_value(TriggerValue::Player(-1)));
    script.add_variable(
        TriggerVar::new(1, VarType::Squad).with_value(TriggerValue::Squad(EntityId::INVALID)),
    );
    script.add_variable(TriggerVar::new(2, VarType::Unit).null());
    script.externals = externals;
    script
}

/// `BTriggerCondition_tcCanRetrieveExternals`: each non-null output gets its external. The
/// null unit output is not asked for, so its missing external does not matter.
#[test]
fn can_retrieve_externals_copies_the_requested_values() {
    let squad = EntityId::new(EntityClass::Squad, 3);
    let mut script = script_with_outputs(TriggerExternals {
        player: Some(2),
        squad: Some(squad),
        ..TriggerExternals::default()
    });
    assert!(can_retrieve_externals(&retrieve(2), &mut script));
    assert_eq!(
        script.get_variable(0).unwrap().value,
        TriggerValue::Player(2)
    );
    assert_eq!(
        script.get_variable(1).unwrap().value,
        TriggerValue::Squad(squad)
    );
    assert!(can_retrieve_externals(&retrieve(3), &mut script));
    assert!(!can_retrieve_externals(&retrieve(1), &mut script));
}

/// A requested external without its flag gives false, but the other outputs still get values.
#[test]
fn can_retrieve_externals_fails_for_a_missing_external() {
    let mut script = script_with_outputs(TriggerExternals::with_player(4));
    assert!(!can_retrieve_externals(&retrieve(2), &mut script));
    assert_eq!(
        script.get_variable(0).unwrap().value,
        TriggerValue::Player(4)
    );
    assert_eq!(
        script.get_variable(1).unwrap().value,
        TriggerValue::Squad(EntityId::INVALID)
    );
}

/// `tcCanRetrieveExternalLocation` (flag `+132 & 0x80`) and `tcCanRetrieveExternalFloat`
/// (flag `+133 & 4`) write signature 1 only when the launcher set the value.
#[test]
fn location_and_float_externals_need_their_flags() {
    let mut script = TriggerScript::new(0);
    script.add_variable(TriggerVar::new(0, VarType::Vector));
    script.add_variable(TriggerVar::new(1, VarType::Float).with_value(TriggerValue::Float(0.0)));
    let location =
        Condition::new(0, ConditionType::CanRetrieveExternalLocation).with_output_at(1, 0);
    let float = Condition::new(1, ConditionType::CanRetrieveExternalFloat).with_output_at(1, 1);
    assert!(!can_retrieve_external_location(&location, &mut script));
    assert!(!can_retrieve_external_float(&float, &mut script));

    let point = TriggerVec3 {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    script.externals.location = Some(point);
    script.externals.float = Some(0.75);
    assert!(can_retrieve_external_location(&location, &mut script));
    assert!(can_retrieve_external_float(&float, &mut script));
    assert_eq!(
        script.get_variable(0).unwrap().value,
        TriggerValue::Vector(point)
    );
    assert_eq!(
        script.get_variable(1).unwrap().value,
        TriggerValue::Float(0.75)
    );
}
