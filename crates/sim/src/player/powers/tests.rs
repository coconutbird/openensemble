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
