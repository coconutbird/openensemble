use sim::{BuildingCommandType, EntityId, TechStatus, TrainingKind};

use super::super::fixture::{
    DISABLE_MARINES, MARINE, MARINE_NAME_ID, PLAYER, UPGRADE, menu_database, menu_world,
};
use super::{ItemRule, MenuItemKind, MenuItemState, UnavailableReason, item_state, menu_items};

#[test]
fn items_follow_the_button_slots_and_queue_with_count_plus_one() {
    let database = menu_database(false);
    let (world, barracks_id) = menu_world(&database, 10.0, 1_000.0);

    let items = menu_items(&world, &database, PLAYER, barracks_id);

    let kinds = items.iter().map(|item| item.kind).collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            MenuItemKind::Research,
            MenuItemKind::TrainSquad,
            MenuItemKind::TrainUnit
        ]
    );
    assert!(
        items
            .iter()
            .all(|item| item.state == MenuItemState::Available)
    );
    assert!(
        items
            .iter()
            .all(|item| !item.can_cancel && item.queued == 0)
    );
    let marine = &items[1];
    assert_eq!(marine.target_name, MARINE);
    assert_eq!(marine.display_name_id, Some(MARINE_NAME_ID));
    assert!((marine.cost.get(0) - 100.0).abs() < f32::EPSILON);
    assert!((marine.total_points - 0.1).abs() < f32::EPSILON);

    let command = marine.queue_command(PLAYER, barracks_id).unwrap();
    assert_eq!(command.building_type, BuildingCommandType::TrainSquad);
    assert_eq!(command.target_id, marine.target_id);
    assert_eq!(command.count, 1);
    assert_eq!(command.base.player_id, i32::from(PLAYER));
    assert_eq!(command.base.recipients, vec![barracks_id]);
    assert!(marine.cancel_command(PLAYER, barracks_id).is_none());
    assert_eq!(
        items[0]
            .queue_command(PLAYER, barracks_id)
            .unwrap()
            .building_type,
        BuildingCommandType::Research
    );
    assert_eq!(
        items[2]
            .queue_command(PLAYER, barracks_id)
            .unwrap()
            .building_type,
        BuildingCommandType::TrainUnit
    );
}

#[test]
fn cost_and_population_block_the_left_click() {
    let database = menu_database(false);
    let (world, barracks_id) = menu_world(&database, 10.0, 40.0);
    let items = menu_items(&world, &database, PLAYER, barracks_id);
    assert_eq!(items[1].state, MenuItemState::MissingResources);
    assert!(items[1].queue_command(PLAYER, barracks_id).is_none());
    assert_eq!(items[2].state, MenuItemState::Available);

    let (world, barracks_id) = menu_world(&database, 0.5, 1_000.0);
    let items = menu_items(&world, &database, PLAYER, barracks_id);
    assert_eq!(items[1].state, MenuItemState::MissingPopulation);
    assert_eq!(items[2].state, MenuItemState::MissingPopulation);
    assert_eq!(items[0].state, MenuItemState::Available);
}

#[test]
fn train_limit_blocks_the_next_item_and_a_queued_item_can_be_canceled() {
    let database = menu_database(true);
    let (mut world, barracks_id) = menu_world(&database, 10.0, 1_000.0);
    let marine_id = sim::squad_prototype_id(&database, MARINE).unwrap();
    world
        .queue_training(
            PLAYER,
            barracks_id,
            &database,
            TrainingKind::Squad,
            marine_id,
            1,
        )
        .unwrap();

    let items = menu_items(&world, &database, PLAYER, barracks_id);
    let marine = &items[1];
    assert_eq!(
        marine.state,
        MenuItemState::Unavailable(UnavailableReason::TrainLimitReached)
    );
    assert_eq!(marine.queued, 1);
    assert!(marine.can_cancel);
    let cancel = marine.cancel_command(PLAYER, barracks_id).unwrap();
    assert_eq!(cancel.building_type, BuildingCommandType::TrainSquad);
    assert_eq!(cancel.count, -1);
    assert!(!items[2].can_cancel);
}

#[test]
fn a_researching_technology_is_unavailable_and_can_be_canceled() {
    let database = menu_database(false);
    let (mut world, barracks_id) = menu_world(&database, 10.0, 1_000.0);
    let upgrade_id = sim::technology_prototype_id(&database, UPGRADE).unwrap();
    world
        .queue_research(PLAYER, barracks_id, &database, upgrade_id)
        .unwrap();

    let items = menu_items(&world, &database, PLAYER, barracks_id);
    let upgrade = &items[0];
    assert_eq!(
        upgrade.state,
        MenuItemState::Unavailable(UnavailableReason::TechnologyStatus(TechStatus::Researching))
    );
    assert_eq!(upgrade.queued, 1);
    let cancel = upgrade.cancel_command(PLAYER, barracks_id).unwrap();
    assert_eq!(cancel.building_type, BuildingCommandType::Research);
    assert_eq!(cancel.target_id, upgrade_id);
    assert_eq!(cancel.count, -1);
}

#[test]
fn disabled_commands_and_other_players_buildings_have_no_items() {
    let database = menu_database(false);
    let (mut world, barracks_id) = menu_world(&database, 10.0, 1_000.0);
    assert_eq!(menu_items(&world, &database, 2, barracks_id), []);
    assert_eq!(menu_items(&world, &database, PLAYER, EntityId::INVALID), []);

    world
        .activate_technology(PLAYER, &database, DISABLE_MARINES)
        .unwrap();
    let kinds = menu_items(&world, &database, PLAYER, barracks_id)
        .iter()
        .map(|item| item.kind)
        .collect::<Vec<_>>();
    assert_eq!(kinds, [MenuItemKind::Research, MenuItemKind::TrainUnit]);
}

#[test]
fn state_rules_use_the_retail_check_order() {
    let mut have = sim::Resources::new();
    have.set(0, 10.0);
    let mut cost = sim::Resources::new();
    cost.set(0, 50.0);
    let free = sim::Resources::new();
    let available = ItemRule::Research {
        status: TechStatus::Available,
        queued: false,
    };

    assert_eq!(
        item_state(false, available, &have, &free, || true),
        MenuItemState::Unavailable(UnavailableReason::NotSelectable)
    );
    assert_eq!(
        item_state(
            true,
            ItemRule::Research {
                status: TechStatus::Obtainable,
                queued: false,
            },
            &have,
            &cost,
            || true,
        ),
        MenuItemState::Unavailable(UnavailableReason::TechnologyStatus(TechStatus::Obtainable))
    );
    assert_eq!(
        item_state(
            true,
            ItemRule::Research {
                status: TechStatus::Available,
                queued: true,
            },
            &have,
            &free,
            || true,
        ),
        MenuItemState::Unavailable(UnavailableReason::AlreadyQueued)
    );
    let limit_reached = ItemRule::Train {
        train_limit_remaining: Some(0),
    };
    assert_eq!(
        item_state(true, limit_reached, &have, &cost, || false),
        MenuItemState::Unavailable(UnavailableReason::TrainLimitReached)
    );
    let unlimited = ItemRule::Train {
        train_limit_remaining: None,
    };
    assert_eq!(
        item_state(true, unlimited, &have, &cost, || false),
        MenuItemState::MissingResources
    );
    assert_eq!(
        item_state(true, unlimited, &have, &free, || false),
        MenuItemState::MissingPopulation
    );
    assert_eq!(
        item_state(
            true,
            ItemRule::Train {
                train_limit_remaining: Some(2),
            },
            &have,
            &free,
            || true,
        ),
        MenuItemState::Available
    );
}
