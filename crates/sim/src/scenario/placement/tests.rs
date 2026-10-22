use super::*;

fn position(number: i32, player: i32) -> ScenarioPosition {
    ScenarioPosition {
        number,
        player,
        position: "1,0,1".to_owned(),
        ..ScenarioPosition::default()
    }
}

fn player(team: i32) -> PlacementPlayer {
    PlacementPlayer { team, pinned: None }
}

#[test]
fn fixed_placement_takes_pinned_then_next_free_positions() {
    let positions = [position(1, -1), position(2, 2), position(3, -1)];
    let positions = positions.iter().collect::<Vec<_>>();
    let players = [
        player(1),
        PlacementPlayer {
            team: 2,
            pinned: Some(1),
        },
        player(3),
    ];
    let mut rng = SimRandom::new();
    let starts = assign_player_positions(
        &players,
        &positions,
        PlayerPlacement::Fixed,
        false,
        &mut rng,
    );
    assert_eq!(starts, [Some(0), Some(1), Some(2)]);
    assert_eq!(rng.seed(), 1, "fixed placement draws no random value");
}

#[test]
fn grouped_placement_keeps_each_team_on_one_number() {
    let positions = [
        position(1, -1),
        position(2, -1),
        position(1, -1),
        position(2, -1),
    ];
    let positions = positions.iter().collect::<Vec<_>>();
    let players = [player(1), player(2), player(1), player(2)];
    for seed in 0..32 {
        let mut rng = SimRandom::new();
        rng.set_seed(seed);
        let starts = assign_player_positions(
            &players,
            &positions,
            PlayerPlacement::Grouped,
            false,
            &mut rng,
        );
        let numbers = starts
            .iter()
            .map(|start| positions[start.expect("assigned")].number)
            .collect::<Vec<_>>();
        assert_eq!(numbers[0], numbers[2], "seed {seed}");
        assert_eq!(numbers[1], numbers[3], "seed {seed}");
        assert_ne!(numbers[0], numbers[1], "seed {seed}");

        let mut again = SimRandom::new();
        again.set_seed(seed);
        let repeat = assign_player_positions(
            &players,
            &positions,
            PlayerPlacement::Grouped,
            false,
            &mut again,
        );
        assert_eq!(starts, repeat, "a fixed seed gives a fixed assignment");
    }
}

#[test]
fn no_random_placement_config_keeps_the_fixed_assignment() {
    let positions = [position(1, -1), position(2, -1)];
    let positions = positions.iter().collect::<Vec<_>>();
    let players = [player(1), player(2)];
    let mut rng = SimRandom::new();
    let starts = assign_player_positions(
        &players,
        &positions,
        PlayerPlacement::Random,
        true,
        &mut rng,
    );
    assert_eq!(starts, [Some(0), Some(1)]);
}
