use super::*;

fn grant(proto_power_id: i32, uses: i32, icon_location: i32) -> PowerGrant {
    PowerGrant {
        proto_power_id,
        squad_id: EntityId::INVALID,
        uses,
        icon_location,
        ignore_cost: false,
        ignore_tech_prerequisites: false,
        ignore_population: false,
    }
}

#[test]
fn grant_replaces_duplicate_and_evicts_implicit_icon_slot() {
    let mut player = Player::new(1);
    player.grant_power(grant(4, 2, -1), PowerRules::default(), |_, _| false);
    assert_eq!(player.power_entry(4).unwrap().finite_uses_remaining(), 2);

    player.grant_power(grant(4, 1, -1), PowerRules::default(), |_, _| false);
    assert_eq!(player.power_entry(4).unwrap().finite_uses_remaining(), 1);

    player.grant_power(grant(7, 3, 5), PowerRules::default(), |power, icon| {
        power == 4 && icon == 5
    });
    assert!(player.power_entry(4).is_none());
    assert_eq!(player.power_entry(7).unwrap().icon_location(), 5);
}

#[test]
fn squad_specific_entries_retain_other_sources() {
    let mut player = Player::new(1);
    let first_squad = EntityId::new(crate::EntityClass::Squad, 2);
    let second_squad = EntityId::new(crate::EntityClass::Squad, 3);
    let mut first = grant(6, 1, -1);
    first.squad_id = first_squad;
    player.grant_power(first, PowerRules::default(), |_, _| false);
    let mut second = first;
    second.squad_id = second_squad;
    player.grant_power(second, PowerRules::default(), |_, _| false);

    player.revoke_power(6, first_squad, PowerRules::default());

    let entry = player.power_entry(6).unwrap();
    assert_eq!(entry.items().len(), 1);
    assert_eq!(entry.items()[0].squad_id(), second_squad);
}

#[test]
fn power_use_prefers_exact_squad_then_falls_back_to_global_source() {
    let mut player = Player::new(1);
    let owner = EntityId::new(crate::EntityClass::Squad, 2);
    let other = EntityId::new(crate::EntityClass::Squad, 3);
    player.grant_power(grant(6, 1, -1), PowerRules::default(), |_, _| false);
    let mut bound = grant(6, 1, -1);
    bound.squad_id = owner;
    player.grant_power(bound, PowerRules::default(), |_, _| false);

    assert!(player.consume_power_use(6, owner, PowerRules::default(), 0, 0));
    let entry = player.power_entry(6).unwrap();
    assert_eq!(
        entry
            .items()
            .iter()
            .find(|item| item.squad_id() == owner)
            .unwrap()
            .uses_remaining(),
        0
    );
    assert_eq!(
        entry
            .items()
            .iter()
            .find(|item| item.squad_id().is_invalid())
            .unwrap()
            .uses_remaining(),
        1
    );

    assert!(player.consume_power_use(6, other, PowerRules::default(), 0, 0));
    assert_eq!(player.power_entry(6).unwrap().finite_uses_remaining(), 0);
}

#[test]
fn multi_recharge_replacement_removes_one_item_then_adds_requested_uses() {
    let mut player = Player::new(1);
    let rules = PowerRules {
        multi_recharge: true,
        ..PowerRules::default()
    };
    player.grant_power(grant(3, 3, -1), rules, |_, _| false);
    assert_eq!(player.power_entry(3).unwrap().items().len(), 3);

    player.grant_power(grant(3, 2, -1), rules, |_, _| false);

    let entry = player.power_entry(3).unwrap();
    assert_eq!(entry.items().len(), 4);
    assert_eq!(entry.finite_uses_remaining(), 4);
}

#[test]
fn infinite_and_sequential_flags_match_retail_item_rules() {
    let mut player = Player::new(1);
    let rules = PowerRules {
        infinite_uses: true,
        sequential_recharge: true,
        ..PowerRules::default()
    };
    player.grant_power(grant(9, 0, -1), rules, |_, _| false);

    let item = &player.power_entry(9).unwrap().items()[0];
    assert!(item.has_infinite_uses());
    assert_eq!(item.uses_remaining(), 0);
    assert_eq!(item.charge_cap(), 0);
    assert!(player.has_available_power_uses(9));
}

#[test]
fn finite_casts_schedule_and_complete_authoritative_recharges() {
    let mut player = Player::new(1);
    player.grant_power(grant(5, 1, -1), PowerRules::default(), |_, _| false);

    assert!(player.consume_power_use(5, EntityId::INVALID, PowerRules::default(), 100, 50,));
    let item = &player.power_entry(5).unwrap().items()[0];
    assert_eq!(item.uses_remaining(), 0);
    assert_eq!(item.times_used(), 1);
    assert_eq!(item.next_grant_time(), 150);
    assert!(item.is_recharging());

    player.update_power_recharges(149, |_| (100, 0));
    assert_eq!(player.power_entry(5).unwrap().finite_uses_remaining(), 0);
    player.update_power_recharges(150, |_| (100, 0));
    let item = &player.power_entry(5).unwrap().items()[0];
    assert_eq!(item.uses_remaining(), 1);
    assert!(!item.is_recharging());
}

#[test]
fn sequential_charges_are_granted_one_recharge_interval_at_a_time() {
    let mut player = Player::new(1);
    let rules = PowerRules {
        sequential_recharge: true,
        ..PowerRules::default()
    };
    player.grant_power(grant(8, 2, -1), rules, |_, _| false);
    assert_eq!(player.power_entry(8).unwrap().finite_uses_remaining(), 0);

    player.update_power_recharges(0, |_| (100, 0));
    assert_eq!(
        player.power_entry(8).unwrap().items()[0].next_grant_time(),
        100
    );
    player.update_power_recharges(100, |_| (100, 0));
    assert_eq!(player.power_entry(8).unwrap().finite_uses_remaining(), 1);
    player.update_power_recharges(100, |_| (100, 0));
    assert_eq!(
        player.power_entry(8).unwrap().items()[0].next_grant_time(),
        200
    );
    player.update_power_recharges(200, |_| (100, 0));
    assert_eq!(player.power_entry(8).unwrap().finite_uses_remaining(), 2);
}
