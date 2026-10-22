use super::*;
use pipeline::database::hw1::GameData;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};

fn database(names: &[&str]) -> Database {
    Database {
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: names
                    .iter()
                    .map(|name| ResourceDef {
                        name: (*name).to_owned(),
                        ..ResourceDef::default()
                    })
                    .collect(),
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

#[test]
fn empty_costs_do_not_need_a_resource_table() {
    let cost = resolve_resource_cost(&Database::default(), []).expect("empty costs resolve");
    assert_eq!(cost, Resources::new());

    assert_eq!(
        resolve_resource_cost(&Database::default(), [("Supplies", 0.0)]),
        Err(ResourceCostError::MissingResourceTable)
    );
}

#[test]
fn duplicate_entries_add_in_order_to_the_first_matching_slot() {
    let database = database(&["Supplies", "supplies", "Power"]);

    let cost = resolve_resource_cost(&database, [(" SUPPLIES ", 10.0), ("supplies", 2.5)])
        .expect("costs resolve");

    assert!((cost.get(0) - 12.5).abs() <= f32::EPSILON);
    assert!(cost.get(1).abs() <= f32::EPSILON);
}

#[test]
fn invalid_amount_is_checked_before_the_resource_name() {
    let database = database(&["Supplies"]);

    for amount in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            resolve_resource_cost(&database, [("Unknown", amount)]),
            Err(ResourceCostError::InvalidCost("Unknown"))
        );
    }
    assert_eq!(
        resolve_resource_cost(&database, [(" Unknown ", 1.0)]),
        Err(ResourceCostError::UnknownResource(" Unknown "))
    );
}

#[test]
fn resources_past_the_slot_limit_are_unknown() {
    let names = (0..=MAX_RESOURCES)
        .map(|index| format!("Resource{index}"))
        .collect::<Vec<_>>();
    let names = names.iter().map(String::as_str).collect::<Vec<_>>();
    let database = database(&names);
    let last = names[MAX_RESOURCES];

    assert_eq!(
        resolve_resource_cost(&database, [(last, 1.0)]),
        Err(ResourceCostError::UnknownResource(last))
    );
}
