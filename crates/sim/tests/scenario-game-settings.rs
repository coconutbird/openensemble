use pipeline::database::hw1::{Database, GameData};
use pipeline::hw1::scenario::{PlayersWrapper, ScenarioData, ScenarioPlayer};
use sim::load_scenario_into_world;

#[test]
fn scenario_database_default_difficulty_initializes_every_player() {
    let scenario = ScenarioData {
        players: Some(PlayersWrapper {
            entries: vec![ScenarioPlayer {
                name: "Campaign Player".to_owned(),
                controllable: true,
                ..ScenarioPlayer::default()
            }],
        }),
        ..ScenarioData::default()
    };
    let mut database = Database::new();
    database.game_data = Some(GameData {
        difficulty_default: Some(0.82),
        ..GameData::default()
    });

    let mut loaded = load_scenario_into_world(&scenario, &database);
    assert!((loaded.world.get_player(0).unwrap().difficulty - 0.82).abs() < f32::EPSILON);
    assert!((loaded.world.get_player(1).unwrap().difficulty - 0.82).abs() < f32::EPSILON);

    let checksum = loaded.world.checksum();
    loaded.world.get_player_mut(1).unwrap().difficulty = 0.2;
    assert_ne!(loaded.world.checksum(), checksum);
}
