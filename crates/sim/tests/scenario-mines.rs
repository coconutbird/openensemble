use sim::load_scenario_from_game_dir;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-mines -- --ignored --nocapture`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenarios_do_not_author_the_latent_mines_action() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let scenarios = std::env::var("OPENENSEMBLE_TEST_SCENARIO")
        .ok()
        .map_or_else(shipped_scenarios, |scenario| vec![scenario]);
    for scenario in scenarios {
        let loaded =
            load_scenario_from_game_dir(&game_dir, &scenario).expect("real scenario should load");
        let candidates = loaded
            .content
            .database
            .abilities
            .iter()
            .filter(|ability| {
                ability.ammo_cost.is_some()
                    || ability.name.to_ascii_lowercase().contains("mine")
                    || ability
                        .objects
                        .iter()
                        .any(|object| object.to_ascii_lowercase().contains("mine"))
            })
            .map(|ability| {
                (
                    ability.name.as_str(),
                    ability.ability_type.as_deref(),
                    ability.ammo_cost,
                    ability.objects.as_slice(),
                )
            })
            .collect::<Vec<_>>();
        assert!(
            candidates.is_empty(),
            "{scenario} unexpectedly authors Mines/AmmoCost abilities: {candidates:#?}"
        );
        let action_candidates = loaded
            .simulation
            .gameplay
            .objects()
            .flat_map(|object| {
                object.tactics().actions.iter().filter_map(|action| {
                    action
                        .action_type
                        .as_deref()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("Mines"))
                        .then_some((object.proto_object_name(), action.name.as_str()))
                })
            })
            .collect::<Vec<_>>();
        assert!(
            action_candidates.is_empty(),
            "{scenario} unexpectedly authors Mines tactic actions: {action_candidates:#?}"
        );
    }
}

fn shipped_scenarios() -> Vec<String> {
    std::iter::once("blood_gulch".to_owned())
        .chain((1..=15).map(|number| format!("PHXscn{number:02}")))
        .collect()
}
