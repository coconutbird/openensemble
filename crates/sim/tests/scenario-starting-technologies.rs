use std::env;

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn campaign_players_activate_layered_civilization_then_leader_technologies() {
    let game_dir = env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let loaded = sim::load_scenario_from_game_dir(&game_dir, "PHXscn01")
        .expect("installed campaign scenario should load");
    let database = &loaded.content.database;
    let mut players_with_starting_technology = 0;
    let mut active_shadow_technologies = 0;

    for player in loaded.simulation.world.active_players() {
        let mut expected = Vec::new();
        if let Some(name) = usize::try_from(player.civ_id)
            .ok()
            .and_then(|index| database.civs.get(index))
            .and_then(|civilization| civilization.civ_tech.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            expected.push(name);
        }
        if let Some(name) = usize::try_from(player.leader_id)
            .ok()
            .and_then(|index| database.leaders.get(index))
            .and_then(|leader| leader.tech.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            && !expected
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(name))
        {
            expected.push(name);
        }
        if expected.is_empty() {
            continue;
        }
        players_with_starting_technology += 1;
        let active = player
            .technologies
            .active_technologies()
            .collect::<Vec<_>>();
        active_shadow_technologies += active
            .iter()
            .filter(|active_name| {
                database.techs.iter().any(|technology| {
                    technology.name.eq_ignore_ascii_case(active_name)
                        && technology
                            .flags
                            .iter()
                            .any(|flag| flag.eq_ignore_ascii_case("Shadow"))
                })
            })
            .count();
        assert!(active.len() >= expected.len());
        let mut previous_index = None;
        for expected in expected {
            let index = active
                .iter()
                .position(|actual| actual.eq_ignore_ascii_case(expected))
                .expect("starting technology should be active");
            assert!(previous_index.is_none_or(|previous| index > previous));
            previous_index = Some(index);
        }
    }

    assert!(
        players_with_starting_technology > 0,
        "PHXscn01 should author at least one concrete civilization/leader bootstrap"
    );
    assert!(
        active_shadow_technologies > 0,
        "PHXscn01 bootstrap should activate at least one shipped Shadow technology"
    );
}
