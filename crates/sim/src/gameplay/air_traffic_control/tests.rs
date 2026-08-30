use super::*;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn only_persistent_air_traffic_control_actions_become_profiles() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "air_pad".to_owned(),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let tactics = TacticData {
        actions: vec![
            Action {
                name: "Controller".to_owned(),
                action_type: Some("AirTrafficControl".to_owned()),
                start_disabled: Some(true),
                ..Action::default()
            },
            Action {
                name: "DecorativeController".to_owned(),
                action_type: Some("AirTrafficControl".to_owned()),
                ..Action::default()
            },
        ],
        tactic: Some(TacticRules {
            persistent_actions: vec!["Controller".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database, [("air_pad".to_owned(), tactics)]);

    let profiles = catalog.air_traffic_control_actions("AIR_PAD");
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].action_name(), "Controller");
    assert!(profiles[0].starts_disabled());
}
