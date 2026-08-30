use super::*;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::tactics::TacticData;
use pipeline::database::hw1::{GameData, ProtoObject};

#[test]
fn authored_gather_fields_and_resource_slots_are_preserved() {
    let database = database();
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [("worker".to_owned(), tactic(Some("supplies"), Some(3.5)))],
    );
    let profile = gameplay
        .gather_action("WORKER", "SUPPLIES")
        .expect("Gather profile");

    assert_eq!(profile.action_name(), "GatherSupplies");
    assert_eq!(profile.resource_name(), "Supplies");
    assert_eq!(profile.resource_id(), 0);
    assert_eq!(profile.work_rate().to_bits(), 3.5_f32.to_bits());
    assert_eq!(profile.work_range().to_bits(), 2.0_f32.to_bits());
    assert!(profile.team_share());
    assert!(profile.starts_disabled());
}

#[test]
fn shipped_shorthand_names_receive_runtime_fallbacks() {
    let database = database();
    let mut shorthand = tactic(None, None);
    shorthand.actions[0].work_range = None;
    let gameplay = GameplayCatalog::from_tactics(&database, [("worker".to_owned(), shorthand)]);
    let profile = gameplay
        .gather_action("worker", "Supplies")
        .expect("conventional GatherSupplies profile");

    assert_eq!(profile.work_rate().to_bits(), DEFAULT_WORK_RATE.to_bits());
    assert_eq!(profile.work_range().to_bits(), DEFAULT_WORK_RANGE.to_bits());
}

fn tactic(resource: Option<&str>, work_rate: Option<f32>) -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "GatherSupplies".to_owned(),
            action_type: Some("Gather".to_owned()),
            resource: resource.map(str::to_owned),
            work_rate,
            work_range: Some(2.0),
            team_share: Some(true),
            start_disabled: Some(true),
            ..Action::default()
        }],
        ..TacticData::default()
    }
}

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "worker".to_owned(),
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![
                    ResourceDef {
                        name: "Supplies".to_owned(),
                        ..ResourceDef::default()
                    },
                    ResourceDef {
                        name: "Collectable".to_owned(),
                        ..ResourceDef::default()
                    },
                ],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}
