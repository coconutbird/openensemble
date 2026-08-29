#[path = "scenario_asset_loading/catalog_assertions.rs"]
mod catalog_assertions;
#[path = "scenario_asset_loading/combat_assertions.rs"]
mod combat_assertions;

use catalog_assertions::assert_loaded_gameplay_catalog;
use combat_assertions::{
    damaged_squad_member_count, squad_member_hitpoint_snapshot, squad_member_hitpoints,
};
use sim::entities::squads::marine::MARINE_SQUAD_NAME;
use sim::{
    AttackQuery, BuildingCommand, CommandEntry, CommandExecutor, LoadedGameScenario, MS_PER_TICK,
    PlayerId, QueuedCommand, RecoveryType, ScenarioAssetLoadError, ShieldCoverage, Simulation,
    SquadMode, TechStatus, TrainingKind, WorkCommand, configure_player_leader,
    load_scenario_from_game_dir, object_prototype_id, spawn_object_at, spawn_squad_at,
    spawn_squad_from_base_by_name, squad_prototype_id, squad_runtime_id, technology_prototype_id,
};

const DATABASE_PATHS: [&str; 10] = [
    "data\\objects.xml.xmb",
    "data\\squads.xml.xmb",
    "data\\techs.xml.xmb",
    "data\\abilities.xml.xmb",
    "data\\powers.xml.xmb",
    "data\\civs.xml.xmb",
    "data\\leaders.xml.xmb",
    "data\\weapontypes.xml.xmb",
    "data\\damagetypes.xml.xmb",
    "data\\gamedata.xml.xmb",
];

fn nearly_equal(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0)
}

#[test]
fn missing_scenario_archive_is_reported_before_database_loading() {
    let missing_dir = std::env::temp_dir().join(format!(
        "openensemble-missing-game-assets-{}",
        std::process::id()
    ));
    let result = load_scenario_from_game_dir(
        &missing_dir.to_string_lossy(),
        "definitely_not_a_real_scenario",
    );

    assert!(matches!(
        result,
        Err(ScenarioAssetLoadError::ScenarioArchiveNotFound { .. })
    ));
}

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-asset-loading -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn loads_real_scenario_database_and_simulation_together() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let scenario =
        std::env::var("OPENENSEMBLE_TEST_SCENARIO").unwrap_or_else(|_| "blood_gulch".to_owned());

    let mut loaded = load_scenario_from_game_dir(&game_dir, &scenario)
        .expect("scenario, layered database, and simulation should load");

    assert_database_and_archive_layers(&mut loaded);
    assert_initial_base_state(&loaded);
    assert_real_barracks_training(&mut loaded);
    assert_real_barracks_research(&mut loaded);
    assert_real_shield_loading_and_recharge(&mut loaded);
    assert_real_base_spawn_and_move(&mut loaded);
}

fn assert_real_barracks_training(loaded: &mut LoadedGameScenario) {
    let fixture = prepare_real_barracks_training(loaded);
    let mut clock = start_real_barracks_training(loaded, &fixture);
    assert_real_training_reserved(loaded, &fixture);
    let trained_id = wait_for_trained_squad(loaded, &mut clock, fixture.barracks_id);
    assert_real_trained_squad(loaded, &fixture, trained_id);
    loaded.simulation.world.remove_squad(trained_id).unwrap();
    loaded
        .simulation
        .world
        .remove_unit(fixture.barracks_id)
        .unwrap();
}

struct BarracksTrainingFixture {
    player_id: PlayerId,
    unit_population_id: usize,
    population_before: sim::Population,
    live_squad_count_before: u32,
    future_squad_count_before: u32,
    barracks_id: sim::EntityId,
    marine_database_id: i32,
    marine_id: i32,
}

fn prepare_real_barracks_training(loaded: &mut LoadedGameScenario) -> BarracksTrainingFixture {
    let marine_id = squad_runtime_id(&loaded.content.database, MARINE_SQUAD_NAME)
        .expect("real database should expose the Marine runtime squad ID");
    let marine_database_id = squad_prototype_id(&loaded.content.database, MARINE_SQUAD_NAME)
        .expect("real database should expose the Marine database squad ID");
    let unit_population_id = loaded
        .content
        .database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.pops.as_ref())
        .and_then(|pops| {
            pops.entries
                .iter()
                .position(|name| name.eq_ignore_ascii_case("Unit"))
        })
        .expect("real GameData should define Unit population");
    let player_id = loaded
        .simulation
        .world
        .active_players()
        .next()
        .map(|player| player.id)
        .expect("the scenario should have an active lobby slot");
    ensure_unit_population_room(loaded, player_id, unit_population_id);
    loaded
        .simulation
        .world
        .get_player_mut(player_id)
        .unwrap()
        .resources
        .amounts = [1_000.0, 10.0, 0.0, 0.0];
    let population_before = *loaded
        .simulation
        .world
        .get_player(player_id)
        .unwrap()
        .get_population(unit_population_id)
        .unwrap();
    let database = &loaded.content.database;
    let barracks_proto_id = object_prototype_id(database, "unsc_bldg_barracks_01")
        .expect("real database should contain the UNSC Barracks");
    let barracks_id = spawn_object_at(
        &mut loaded.simulation.world,
        database,
        player_id,
        barracks_proto_id,
        glam::Vec3::new(4_900.0, 0.0, 5_000.0),
        glam::Vec3::Z,
    )
    .expect("real Barracks prototype should spawn as a building");
    let live_squad_count_before = loaded
        .simulation
        .world
        .player_squad_count(player_id, Some(marine_database_id));
    let future_squad_count_before = loaded
        .simulation
        .world
        .player_future_squad_count(player_id, Some(marine_database_id));
    BarracksTrainingFixture {
        player_id,
        unit_population_id,
        population_before,
        live_squad_count_before,
        future_squad_count_before,
        barracks_id,
        marine_database_id,
        marine_id,
    }
}

fn ensure_unit_population_room(
    loaded: &mut LoadedGameScenario,
    player_id: PlayerId,
    unit_population_id: usize,
) {
    let database = &loaded.content.database;
    let has_room = loaded
        .simulation
        .world
        .get_player(player_id)
        .and_then(|player| player.get_population(unit_population_id))
        .is_some_and(|population| {
            population.count + population.future + 1.0 <= population.cap
                && population.count + population.future + 1.0 <= population.max
        });
    if !has_room {
        let cutter_id = database
            .leaders
            .iter()
            .position(|leader| leader.name.eq_ignore_ascii_case("Cutter"))
            .and_then(|index| i32::try_from(index).ok())
            .expect("real database should contain Cutter for the lobby fixture");
        assert!(configure_player_leader(
            &mut loaded.simulation.world,
            database,
            player_id,
            cutter_id,
        ));
    }
    assert!(
        loaded
            .simulation
            .world
            .get_player(player_id)
            .and_then(|player| player.get_population(unit_population_id))
            .is_some_and(|population| {
                population.count + population.future + 1.0 <= population.cap
                    && population.count + population.future + 1.0 <= population.max
            }),
        "the lobby-selected Cutter should have one free Unit population"
    );
}

fn start_real_barracks_training(
    loaded: &mut LoadedGameScenario,
    fixture: &BarracksTrainingFixture,
) -> Simulation {
    let mut clock = Simulation::new();
    clock.game_time_ms = loaded.simulation.world.game_time_ms;
    clock.command_queue.enqueue_building(
        BuildingCommand::train_squads(
            i32::from(fixture.player_id),
            vec![fixture.barracks_id],
            fixture.marine_id,
            1,
        ),
        clock.game_time_ms.saturating_add(MS_PER_TICK),
        u64::from(fixture.player_id),
    );
    clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
    clock
}

fn assert_real_training_reserved(loaded: &LoadedGameScenario, fixture: &BarracksTrainingFixture) {
    let player = loaded
        .simulation
        .world
        .get_player(fixture.player_id)
        .unwrap();
    assert!(nearly_equal(player.resources.get(0), 900.0));
    assert!(nearly_equal(
        player.population[fixture.unit_population_id].future,
        fixture.population_before.future + 1.0
    ));
    assert_eq!(
        loaded
            .simulation
            .world
            .player_future_squad_count(fixture.player_id, Some(fixture.marine_database_id)),
        fixture.future_squad_count_before + 1
    );
    let progress = loaded
        .simulation
        .world
        .training_progress(
            fixture.player_id,
            fixture.barracks_id,
            &loaded.content.database,
            TrainingKind::Squad,
            fixture.marine_id,
        )
        .unwrap()
        .expect("the paid Marine should be on the Barracks worker");
    assert!(nearly_equal(progress.current_points, 0.0));
    assert!(nearly_equal(progress.total_points, 8.0));
}

fn wait_for_trained_squad(
    loaded: &mut LoadedGameScenario,
    clock: &mut Simulation,
    barracks_id: sim::EntityId,
) -> sim::EntityId {
    for _ in 0..200 {
        clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
        let trained_id = loaded
            .simulation
            .world
            .squads
            .iter()
            .find_map(|(id, squad)| (squad.trained_by == Some(barracks_id)).then_some(id));
        if let Some(trained_id) = trained_id {
            return trained_id;
        }
    }
    panic!("eight work points should complete a real Marine squad");
}

fn assert_real_trained_squad(
    loaded: &LoadedGameScenario,
    fixture: &BarracksTrainingFixture,
    trained_id: sim::EntityId,
) {
    let database = &loaded.content.database;
    let trained = loaded
        .simulation
        .world
        .get_squad(trained_id)
        .expect("completed Marine squad");
    assert_eq!(trained.proto_squad_name, MARINE_SQUAD_NAME);
    let effective_marine = loaded
        .simulation
        .world
        .get_player(fixture.player_id)
        .unwrap()
        .technologies
        .resolved_squad_prototype(MARINE_SQUAD_NAME);
    let expected_members = database
        .squads
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(effective_marine))
        .and_then(|prototype| prototype.units.as_ref())
        .map(|units| {
            units
                .entries
                .iter()
                .map(|entry| usize::try_from(entry.count.max(0)).unwrap_or_default())
                .sum::<usize>()
        })
        .expect("effective Marine prototype should contain authored members");
    assert_eq!(trained.unit_ids.len(), expected_members);
    assert!(loaded.simulation.world.squad_is_at_max_size(trained_id));
    let population = loaded
        .simulation
        .world
        .get_player(fixture.player_id)
        .unwrap()
        .population[fixture.unit_population_id];
    assert!(nearly_equal(
        population.future,
        fixture.population_before.future
    ));
    assert!(nearly_equal(
        population.count,
        fixture.population_before.count + 1.0
    ));
    assert_eq!(
        loaded
            .simulation
            .world
            .player_squad_count(fixture.player_id, Some(fixture.marine_database_id)),
        fixture.live_squad_count_before + 1
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .player_future_squad_count(fixture.player_id, Some(fixture.marine_database_id)),
        fixture.future_squad_count_before
    );
}

fn assert_real_barracks_research(loaded: &mut LoadedGameScenario) {
    let database = &loaded.content.database;
    let technology_id = technology_prototype_id(database, "unsc_marine_upgrade1")
        .expect("real database should contain the first Marine upgrade");
    let player_id = loaded
        .simulation
        .world
        .active_players()
        .find(|player| !player.technologies.is_active("unsc_marine_upgrade1"))
        .map(|player| player.id)
        .expect("the scenario should have a player without the test technology");
    loaded
        .simulation
        .world
        .get_player_mut(player_id)
        .unwrap()
        .resources
        .amounts = [1_000.0, 10.0, 0.0, 0.0];
    let barracks_proto_id = object_prototype_id(database, "unsc_bldg_barracks_01")
        .expect("real database should contain the UNSC Barracks");
    let barracks_id = spawn_object_at(
        &mut loaded.simulation.world,
        database,
        player_id,
        barracks_proto_id,
        glam::Vec3::new(5_000.0, 0.0, 5_000.0),
        glam::Vec3::Z,
    )
    .expect("real Barracks prototype should spawn as a building");
    assert_eq!(
        loaded
            .simulation
            .world
            .technology_status(player_id, database, technology_id)
            .unwrap(),
        TechStatus::Available
    );

    let mut clock = Simulation::new();
    clock.game_time_ms = loaded.simulation.world.game_time_ms;
    clock.command_queue.enqueue_building(
        BuildingCommand::research(i32::from(player_id), vec![barracks_id], technology_id, 1),
        clock.game_time_ms.saturating_add(MS_PER_TICK),
        u64::from(player_id),
    );
    clock.tick_with_scenario(&mut loaded.simulation, database);
    let resources = loaded
        .simulation
        .world
        .get_player(player_id)
        .unwrap()
        .resources
        .amounts;
    for (actual, expected) in resources.into_iter().zip([800.0, 9.0, 0.0, 0.0]) {
        assert!((actual - expected).abs() <= 1.0e-6);
    }
    assert_eq!(
        loaded
            .simulation
            .world
            .technology_status(player_id, database, technology_id)
            .unwrap(),
        TechStatus::Researching
    );

    for _ in 0..900 {
        clock.tick_with_scenario(&mut loaded.simulation, database);
        if loaded
            .simulation
            .world
            .technology_status(player_id, database, technology_id)
            .unwrap()
            == TechStatus::Active
        {
            break;
        }
    }
    assert_eq!(
        loaded
            .simulation
            .world
            .technology_status(player_id, database, technology_id)
            .unwrap(),
        TechStatus::Active
    );
    assert!(
        loaded
            .simulation
            .world
            .research_progress(player_id, database, technology_id)
            .unwrap()
            .is_none()
    );
}

fn assert_real_shield_loading_and_recharge(loaded: &mut LoadedGameScenario) {
    assert!(nearly_equal(
        loaded.simulation.gameplay.shield_regen_delay(),
        20.0
    ));
    assert!(nearly_equal(
        loaded.simulation.gameplay.shield_regen_time(),
        5.0
    ));
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("the scenario should assign a player base")
        .0;
    let spartan_proto_id = squad_prototype_id(&loaded.content.database, "unsc_inf_spartan_01")
        .expect("real database should contain the Spartan squad");
    let squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        spartan_proto_id,
        glam::Vec3::ZERO,
        glam::Vec3::Z,
    )
    .expect("real scenario database should spawn a Spartan squad");
    let unit_id = loaded
        .simulation
        .world
        .get_squad(squad_id)
        .expect("spawned Spartan squad")
        .unit_ids[0];
    let unit = loaded
        .simulation
        .world
        .get_unit(unit_id)
        .expect("spawned Spartan unit");
    assert_eq!(unit.shields.coverage, ShieldCoverage::Full);
    assert!(nearly_equal(unit.shields.maximum, 5_000.0));
    assert!(nearly_equal(unit.shields.current, 0.0));

    loaded
        .simulation
        .world
        .update_entities_with_gameplay(5.0, &loaded.simulation.gameplay);
    assert!(nearly_equal(
        loaded
            .simulation
            .world
            .get_unit(unit_id)
            .expect("charged Spartan")
            .shields
            .current,
        5_000.0
    ));
    assert!(loaded.simulation.world.damage_unit(unit_id, 750.0));
    loaded
        .simulation
        .world
        .update_entities_with_gameplay(20.0, &loaded.simulation.gameplay);
    assert!(nearly_equal(
        loaded
            .simulation
            .world
            .get_unit(unit_id)
            .expect("delayed Spartan")
            .shields
            .current,
        4_250.0
    ));
    loaded
        .simulation
        .world
        .update_entities_with_gameplay(0.1, &loaded.simulation.gameplay);
    assert!(
        loaded
            .simulation
            .world
            .get_unit(unit_id)
            .expect("recharging Spartan")
            .shields
            .current
            > 4_250.0
    );
}

fn assert_database_and_archive_layers(loaded: &mut LoadedGameScenario) {
    assert_archive_and_database_layers(loaded);
    assert_loaded_terrain_bounds(loaded);
    assert_loaded_gameplay_catalog(loaded);
    assert_real_marine_action_selection(loaded);
}

fn assert_archive_and_database_layers(loaded: &LoadedGameScenario) {
    let archives = loaded.source.files_per_archive();
    let (scenario_archive, scenario_files) = archives
        .last()
        .expect("the loaded scenario should be the final archive layer");
    assert!(
        scenario_files
            .iter()
            .any(|file| file.to_ascii_lowercase().ends_with(".scn.xmb")),
        "{scenario_archive} should contain the active SCN"
    );
    for path in DATABASE_PATHS {
        assert!(
            loaded.source.provenance(path).is_some(),
            "database table {path} should resolve alongside {scenario_archive}"
        );
    }

    assert!(!loaded.content.database.objects.is_empty());
    assert!(!loaded.content.database.squads.is_empty());
    assert!(
        loaded
            .content
            .database
            .objects
            .iter()
            .any(|object| { object.name.eq_ignore_ascii_case("unsc_veh_warthog_01") })
    );
    assert!(
        loaded
            .content
            .database
            .objects
            .iter()
            .any(|object| { object.name.eq_ignore_ascii_case("unsc_inf_marine_01") })
    );
    assert!(loaded.content.scenario_data.is_some());
    assert!(loaded.simulation.world.player_count() > 1);
}

fn assert_loaded_terrain_bounds(loaded: &LoadedGameScenario) {
    let terrain = loaded
        .content
        .terrain_data
        .as_ref()
        .expect("the active scenario should expose XTD terrain bounds");
    let bounds = loaded
        .simulation
        .world
        .terrain_bounds()
        .expect("sim should consume the active scenario XTD bounds");
    assert!(nearly_equal(bounds.min_x(), terrain.header.world_min[0]));
    assert!(nearly_equal(bounds.min_z(), terrain.header.world_min[2]));
    assert!(nearly_equal(bounds.max_x(), terrain.header.world_max[0]));
    assert!(nearly_equal(bounds.max_z(), terrain.header.world_max[2]));
    assert!(
        loaded.simulation.world.has_terrain_simulation(),
        "sim should load the scenario XSD height grid alongside XTD presentation terrain"
    );
    let center = glam::Vec3::new(
        f32::midpoint(terrain.header.world_min[0], terrain.header.world_max[0]),
        0.0,
        f32::midpoint(terrain.header.world_min[2], terrain.header.world_max[2]),
    );
    let center_height = loaded
        .simulation
        .world
        .terrain_height(center, false)
        .expect("the XSD should cover the center of the XTD world bounds");
    assert!(center_height.is_finite());
    assert!(center_height >= terrain.header.world_min[1]);
    assert!(center_height <= terrain.header.world_max[1]);
}

fn assert_real_marine_action_selection(loaded: &mut LoadedGameScenario) {
    let command_id = loaded
        .content
        .database
        .abilities
        .iter()
        .position(|ability| ability.name.eq_ignore_ascii_case("Command"))
        .and_then(|index| u8::try_from(index).ok())
        .expect("real database should expose the Command ability on the wire");
    let mut query = AttackQuery {
        target_proto_object_name: Some("unsc_inf_marine_01"),
        ..AttackQuery::default()
    };
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("the scenario should assign a player base")
        .0;

    assert_eq!(
        selected_marine_action(loaded, player_id, &query),
        Some("AssaultRifleAttackAction")
    );
    query.ability_id = Some(command_id);
    assert_eq!(
        selected_marine_action(loaded, player_id, &query),
        Some("GrenadeAttackAction")
    );
    query.squad_mode = SquadMode::Cover;
    assert_eq!(
        selected_marine_action(loaded, player_id, &query),
        Some("InCoverGrenadeAttackAction")
    );

    for technology in ["unsc_marine_upgrade1", "unsc_marine_upgrade2"] {
        assert!(
            loaded
                .simulation
                .world
                .activate_technology(player_id, &loaded.content.database, technology)
                .expect("real Marine technology should activate")
        );
    }
    query.squad_mode = SquadMode::Normal;
    assert_eq!(
        selected_marine_action(loaded, player_id, &query),
        Some("RocketAttackAction")
    );
}

fn selected_marine_action<'catalog>(
    loaded: &'catalog LoadedGameScenario,
    player_id: PlayerId,
    query: &AttackQuery<'_>,
) -> Option<&'catalog str> {
    let technologies = &loaded.simulation.world.get_player(player_id)?.technologies;
    loaded
        .simulation
        .gameplay
        .select_ranged_action("unsc_inf_marine_01", query, |action| {
            technologies.action_enabled(
                "unsc_inf_marine_01",
                &action.name,
                action.start_disabled != Some(true),
            )
        })
        .map(|action| action.action.name.as_str())
}

fn assert_initial_base_state(loaded: &LoadedGameScenario) {
    let scenario_data = loaded
        .content
        .scenario_data
        .as_ref()
        .expect("scenario data");
    let max_players = loaded
        .content
        .scenario
        .as_ref()
        .map(|descriptor| descriptor.max_players);
    let expected_start_count = scenario_data
        .positions()
        .iter()
        .filter(|position| {
            u32::try_from(position.number).is_ok_and(|number| {
                number > 0 && max_players.is_none_or(|maximum| number <= maximum)
            })
        })
        .count()
        .min(loaded.simulation.world.active_players().count());
    assert_eq!(
        loaded.simulation.initial_base_ids.len(),
        expected_start_count
    );
    for (&player_id, &base_id) in &loaded.simulation.initial_base_ids {
        assert_eq!(
            loaded.simulation.get_initial_base_id(player_id),
            Some(base_id)
        );
        let base = loaded
            .simulation
            .world
            .get_base(base_id)
            .expect("initial base should be registered in the sim world");
        let anchor = loaded
            .simulation
            .world
            .get_building(base.anchor_building_id)
            .expect("initial base should have a building anchor");
        assert_eq!(base.player_id, player_id);
        assert_eq!(anchor.base.player_id, player_id);
        assert!(!anchor.proto_object_name.is_empty());
    }
    assert!(
        !loaded.simulation.world.units.is_empty() || !loaded.simulation.world.squads.is_empty()
    );
}

fn assert_real_base_spawn_and_move(loaded: &mut LoadedGameScenario) {
    let (&player_id, &base_id) = loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("the skirmish scenario should assign at least one player base");
    let units_before = loaded.simulation.world.units.len();
    let squad_id = spawn_squad_from_base_by_name(
        &mut loaded.simulation.world,
        &loaded.content.database,
        base_id,
        MARINE_SQUAD_NAME,
    )
    .expect("the real layered database should spawn Marines from a sim base");
    let squad = loaded
        .simulation
        .world
        .get_squad(squad_id)
        .expect("spawned Marine squad");
    assert_eq!(squad.base.player_id, player_id);
    assert!(loaded.simulation.world.units.len() > units_before);

    let start = squad.base.position;
    let target = start + squad.base.forward * 20.0;
    let mut clock = Simulation::new();
    clock.start();
    clock.command_queue.enqueue_work(
        WorkCommand::move_squads(i32::from(player_id), vec![squad_id], target),
        MS_PER_TICK,
        u64::from(player_id),
    );
    clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
    let squad = loaded
        .simulation
        .world
        .get_squad(squad_id)
        .expect("moving Marine squad");
    assert_eq!(squad.move_target, Some(target));
    assert_ne!(squad.base.position, start);
    assert_real_marine_combat(loaded, player_id, squad_id, &mut clock);
}

fn assert_real_marine_combat(
    loaded: &mut LoadedGameScenario,
    attacker_player_id: sim::PlayerId,
    attacker_squad_id: sim::EntityId,
    clock: &mut Simulation,
) {
    let enemy_player_id = loaded
        .simulation
        .world
        .active_players()
        .map(|player| player.id)
        .find(|&player_id| {
            loaded
                .simulation
                .world
                .players_are_enemies(attacker_player_id, player_id)
        })
        .expect("the skirmish scenario should contain an enemy player");
    let (attacker_position, attacker_forward) = loaded
        .simulation
        .world
        .get_squad(attacker_squad_id)
        .map(|attacker| (attacker.base.position, attacker.base.forward))
        .expect("attacking Marine squad");
    let target_position = attacker_position + attacker_forward * 10.0;
    let marine_proto_id = squad_prototype_id(&loaded.content.database, MARINE_SQUAD_NAME)
        .expect("real database should contain the Marine squad");
    let target_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        enemy_player_id,
        marine_proto_id,
        target_position,
        -attacker_forward,
    )
    .expect("real database should spawn an enemy Marine squad");
    let target_member_ids = loaded
        .simulation
        .world
        .get_squad(target_squad_id)
        .expect("target Marine squad")
        .unit_ids
        .clone();
    let initial_hitpoints = squad_member_hitpoints(&loaded.simulation.world, &target_member_ids);
    let command = WorkCommand::attack_squads(
        i32::from(attacker_player_id),
        vec![attacker_squad_id],
        target_squad_id,
    );
    clock.command_queue.enqueue_work(
        command,
        clock.game_time_ms.saturating_add(MS_PER_TICK),
        u64::from(attacker_player_id),
    );

    let mut saw_projectile = false;
    let mut damaged_hitpoints = initial_hitpoints;
    for _ in 0..600 {
        clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
        saw_projectile |= !loaded.simulation.world.projectiles.is_empty();
        damaged_hitpoints = squad_member_hitpoints(&loaded.simulation.world, &target_member_ids);
        if damaged_hitpoints < initial_hitpoints {
            break;
        }
    }
    assert!(
        saw_projectile,
        "Marine Attack tags should launch rifle projectiles"
    );
    assert!(
        damaged_hitpoints < initial_hitpoints,
        "Marine projectile impact should reduce authoritative sim hit points"
    );
    assert_real_marine_rocket_recovery(
        loaded,
        attacker_player_id,
        attacker_squad_id,
        target_squad_id,
        clock,
    );
}

fn assert_real_marine_rocket_recovery(
    loaded: &mut LoadedGameScenario,
    attacker_player_id: PlayerId,
    attacker_squad_id: sim::EntityId,
    target_squad_id: sim::EntityId,
    clock: &mut Simulation,
) {
    let command_id = database_ability_id(&loaded.content.database, "Command");
    let rocket_id = database_ability_id(&loaded.content.database, "UnscMarineRockets");
    let (target_member_ids, initial_target_hitpoints) =
        squad_member_hitpoint_snapshot(&loaded.simulation.world, target_squad_id);
    let mut command = WorkCommand::attack_squads(
        i32::from(attacker_player_id),
        vec![attacker_squad_id],
        target_squad_id,
    );
    command.ability_id = i32::from(command_id);
    clock.command_queue.enqueue_work(
        command,
        clock.game_time_ms.saturating_add(MS_PER_TICK),
        u64::from(attacker_player_id),
    );

    let mut saw_rocket_action = false;
    let mut damaged_member_count = 0;
    for _ in 0..600 {
        clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
        let squad = loaded
            .simulation
            .world
            .get_squad(attacker_squad_id)
            .expect("attacking Marine squad should remain alive");
        saw_rocket_action |= squad.unit_ids.iter().any(|unit_id| {
            loaded
                .simulation
                .world
                .get_unit(*unit_id)
                .and_then(|unit| unit.combat.action_name())
                .is_some_and(|action| action.eq_ignore_ascii_case("RocketAttackAction"))
        });
        damaged_member_count = damaged_squad_member_count(
            &loaded.simulation.world,
            &target_member_ids,
            &initial_target_hitpoints,
        );
        if squad.recovery.is_recovering() && damaged_member_count >= 2 {
            break;
        }
    }

    let squad = loaded
        .simulation
        .world
        .get_squad(attacker_squad_id)
        .expect("Marine squad should enter ability recovery");
    assert!(saw_rocket_action, "Marine upgrade 2 should select rockets");
    assert_eq!(squad.recovery.recovery_type(), Some(RecoveryType::Ability));
    assert_eq!(squad.recovery.ability_id(), Some(rocket_id));
    assert!(squad.recovery.remaining() > 0.0);
    assert!(squad.recovery.remaining() <= 20.0);
    assert!(
        damaged_member_count >= 2,
        "Marine rocket AOE should damage multiple authoritative squad members"
    );

    let previous_target = squad.attack_target;
    let (target_player_id, target_position) = loaded
        .simulation
        .world
        .get_squad(target_squad_id)
        .map(|target| (target.base.player_id, target.base.position))
        .expect("first target squad should remain alive");
    let marine_proto_id = squad_prototype_id(&loaded.content.database, MARINE_SQUAD_NAME)
        .expect("real database should contain the Marine squad");
    let second_target = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        target_player_id,
        marine_proto_id,
        target_position,
        glam::Vec3::NEG_Z,
    )
    .expect("real database should spawn a second target squad");
    let mut blocked = WorkCommand::attack_squads(
        i32::from(attacker_player_id),
        vec![attacker_squad_id],
        second_target,
    );
    blocked.ability_id = i32::from(command_id);
    CommandExecutor::with_database(&loaded.content.database).execute(
        &mut loaded.simulation.world,
        &CommandEntry {
            command: QueuedCommand::Work(blocked),
            exec_time: clock.game_time_ms,
            sequence: 0,
            source_client: u64::from(attacker_player_id),
        },
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_squad(attacker_squad_id)
            .expect("recovering Marine squad")
            .attack_target,
        previous_target,
        "the active Ability recovery channel should reject Command reuse"
    );
}

fn database_ability_id(database: &pipeline::database::hw1::Database, name: &str) -> u8 {
    database
        .abilities
        .iter()
        .position(|ability| ability.name.eq_ignore_ascii_case(name))
        .and_then(|index| u8::try_from(index).ok())
        .unwrap_or_else(|| panic!("real database should contain ability {name}"))
}
