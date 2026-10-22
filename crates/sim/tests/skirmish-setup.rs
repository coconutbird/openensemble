//! Checks the tick-0 state of a 1v1 Blood Gulch skirmish against `BScenario_load`.

use sim::PlayerType;

fn count_units(world: &sim::World, proto: &str, player: Option<u8>) -> usize {
    world
        .units
        .iter()
        .filter(|(_, unit)| unit.proto_object_name.eq_ignore_ascii_case(proto))
        .filter(|(_, unit)| player.is_none_or(|player| unit.base.player_id == player))
        .count()
}

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test skirmish-setup -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_1v1_matches_the_retail_tick_zero_setup() {
    let loaded = test_support::installed_scenario("blood_gulch");
    let world = &loaded.simulation.world;
    let database = &loaded.content.database;
    let leader = |name: &str| {
        database
            .leaders
            .iter()
            .position(|leader| leader.name == name)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap()
    };

    // `BScenario_setupPlayers`: Gaia, two lobby players and the Creeps NPC.
    let players = world.players().collect::<Vec<_>>();
    assert_eq!(players.len(), 4);
    assert_eq!(players[1].player_type, PlayerType::Human);
    assert_eq!(players[1].leader_id, leader("Cutter"));
    assert_eq!(players[2].player_type, PlayerType::ComputerAi);
    assert_eq!(players[2].leader_id, leader("Arbiter"));
    assert_eq!(players[3].name, "Creeps");
    assert_eq!(players[3].player_type, PlayerType::Npc);
    assert!(
        players[3].leader_id >= 0,
        "Creeps get a random Covenant leader"
    );
    for player in &players[1..=2] {
        assert!((player.get_resource(0) - 800.0).abs() < f32::EPSILON);
    }

    // Colour loop at 0x140382FC0: the lobby players take the first entry of their civ row
    // (UNSC 1, Covenant 7). Creeps have `UsePlayerSettings="false"` and keep `Color="13"`.
    // Gaia keeps the `BPlayer_ctor` colour -1 (255).
    let colors = players
        .iter()
        .map(|player| player.color)
        .collect::<Vec<_>>();
    assert_eq!(colors, [u8::MAX, 1, 7, 13]);

    // `BScenario_createStartingUnits`: Gaia sockets with unbuilt HQs and the leader squads.
    for (player, hq) in [(1, "unsc_bldg_command_03"), (2, "cov_bldg_builder_02")] {
        let hq_id = world
            .units
            .iter()
            .find(|(_, unit)| unit.proto_object_name == hq && unit.base.player_id == player)
            .map(|(id, _)| id)
            .expect("starting HQ");
        assert!(!world.get_unit(hq_id).unwrap().built);
        assert!(loaded.simulation.get_initial_base_id(player).is_some());
    }
    assert_eq!(count_units(world, "unsc_veh_warthog_01", Some(1)), 1);
    assert_eq!(count_units(world, "cov_veh_ghost_01", Some(2)), 1);

    // `.sc2` trees and `.sc3` sound objects.
    let objects = |prefix: &str| {
        world
            .objects
            .iter()
            .filter(|(_, object)| object.proto_object_name.starts_with(prefix))
            .count()
    };
    assert_eq!(objects("env_arcadia_pinetree"), 35);
    assert_eq!(objects("snd_play"), 18);

    // `BScenario_load`: the `.scn` trigger systems, then the game mode (`GG`): `SkirmishWorld`
    // once and `SkirmishPlayer` for the human and computer AI players only, then (`HH`)
    // `SkirmishAI_raw_` for the computer AI player.
    assert_eq!(
        script_list(world),
        [
            ("Audio".to_owned(), None),
            ("Blood_Gulch".to_owned(), None),
            ("SkirmishWorld.triggerscript".to_owned(), None),
            ("SkirmishPlayer.triggerscript".to_owned(), Some(1)),
            ("SkirmishPlayer.triggerscript".to_owned(), Some(2)),
            ("SkirmishAI_raw_.triggerscript".to_owned(), Some(2)),
        ]
    );
    let ai_cost = world
        .trigger_engine()
        .scripts()
        .find_map(|(_, script)| script.name.starts_with("SkirmishAI").then_some(script))
        .and_then(|script| script.externals.cost);
    assert_eq!(ai_cost, Some(sim::trigger::TriggerCost::default()));
}

/// The name and Player external of each trigger script, in script ID order.
fn script_list(world: &sim::World) -> Vec<(String, Option<i32>)> {
    world
        .trigger_engine()
        .scripts()
        .map(|(_, script)| (script.name.clone(), script.externals.player))
        .collect()
}

/// Simulated time of the 1v1 run: three minutes.
const RUN_MS: u32 = 180_000;

/// Script files whose effects and conditions name the source of an unsupported type.
const SCRIPT_FILES: [&str; 6] = [
    "SkirmishWorld.triggerscript",
    "SkirmishPlayer.triggerscript",
    "SkirmishAI_raw_.triggerscript",
    "Creeps.triggerscript",
    "CrateMaker3.triggerscript",
    "CrateMakerMed.triggerscript",
];

/// Runs the 1v1 for a few simulated minutes: the starting HQs complete after their build points
/// and the game-mode scripts defeat nobody early.
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_1v1_runs_minutes_without_an_early_defeat() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let database = &loaded.content.database;
    let hqs = [(1, "unsc_bldg_command_03"), (2, "cov_bldg_builder_02")].map(|(player, proto)| {
        let id = loaded
            .simulation
            .world
            .units
            .iter()
            .find(|(_, unit)| unit.proto_object_name == proto && unit.base.player_id == player)
            .map(|(id, _)| id)
            .expect("starting HQ");
        let build_points = database
            .objects
            .iter()
            .find(|object| object.name == proto)
            .and_then(|object| object.build_points)
            .expect("HQ build points");
        (id, build_points)
    });

    let mut clock = sim::Simulation::new();
    let mut built_at = [None; 2];
    let mut unsupported_effects = std::collections::BTreeMap::<u16, u32>::new();
    let mut unsupported_conditions = std::collections::BTreeMap::<u16, u32>::new();
    let mut loop_guard_ticks = 0_u32;
    while loaded.simulation.world.game_time() < RUN_MS {
        let tick = clock.tick_with_scenario(&mut loaded.simulation, database);
        loop_guard_ticks += u32::from(tick.triggers.infinite_loop_guard_reached);
        for raw_type in tick.triggers.unsupported_effect_types {
            *unsupported_effects.entry(raw_type).or_default() += 1;
        }
        for raw_type in tick.triggers.unsupported_condition_types {
            *unsupported_conditions.entry(raw_type).or_default() += 1;
        }
        let world = &loaded.simulation.world;
        for ((hq, _), built) in hqs.iter().zip(&mut built_at) {
            if built.is_none() && world.get_unit(*hq).is_some_and(|unit| unit.built) {
                *built = Some(world.game_time());
            }
        }
    }

    let world = &loaded.simulation.world;
    let mut resident = std::collections::BTreeMap::<String, u32>::new();
    for (name, _) in script_list(world) {
        *resident.entry(name).or_default() += 1;
    }
    println!("resident scripts after {RUN_MS} ms: {resident:?}");
    println!("HQs built at {built_at:?} ms");
    let library = world.trigger_engine().script_library();
    let sources = |raw_type: u16, effect: bool| {
        SCRIPT_FILES
            .iter()
            .filter(|file| {
                let sim::trigger::LibraryScript::Parsed(script) = library.get(file) else {
                    return false;
                };
                script.triggers.iter().any(|trigger| {
                    if effect {
                        trigger.effects().any(|item| item.raw_type == raw_type)
                    } else {
                        trigger
                            .conditions
                            .iter()
                            .any(|item| item.raw_type == raw_type)
                    }
                })
            })
            .map(|file| file.trim_end_matches(".triggerscript"))
            .collect::<Vec<_>>()
    };
    for (kind, counts, effect) in [
        ("effect", &unsupported_effects, true),
        ("condition", &unsupported_conditions, false),
    ] {
        let mut ranked = counts.iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));
        println!("unsupported {kind} types: {}", ranked.len());
        for (raw_type, ticks) in ranked {
            println!(
                "  {kind} {raw_type}: {ticks} ticks, in {:?}",
                sources(*raw_type, effect)
            );
        }
    }
    // A script that loops until the guard stops it points to a missing effect or condition. The
    // skirmish AI script looped in each update until the sim had `AsTime` (716).
    assert_eq!(
        loop_guard_ticks, 0,
        "ticks whose trigger update reached the loop guard"
    );
    for ((_, build_points), built) in hqs.iter().zip(built_at) {
        // `BUnitActionBuilding_update` @ `0x1404F4B70` advances the unbuilt HQ by the elapsed
        // seconds against its authored `BuildPoints`; no work-rate or quick-build scalar applies.
        let built = f64::from(built.expect("the starting HQ completes"));
        let build_ms = f64::from(*build_points) * 1000.0;
        assert!(
            (build_ms..=build_ms + 1000.0).contains(&built),
            "HQ built at {built} ms, build points {build_points}"
        );
    }
    for player in [1, 2] {
        assert_eq!(
            world.get_player(player).unwrap().state,
            sim::PlayerState::Playing,
            "player {player} must not lose early"
        );
    }
    assert_eq!(world.game_result(), None);
    // `SkirmishWorld` collects the creep units at 1 s and checks them every 10 s. It defeats the
    // creep player and ends only when none of them is left. No player attacks the creeps here,
    // so the world script stays resident with the player and AI scripts.
    assert!(
        world.units.iter().any(|(_, unit)| unit.base.player_id == 3),
        "the Creeps scripts spawn creep units"
    );
    let launched = script_list(world)
        .into_iter()
        .filter(|(name, _)| {
            ["SkirmishWorld", "SkirmishPlayer", "SkirmishAI_raw_"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        launched,
        [
            ("SkirmishWorld.triggerscript".to_owned(), None),
            ("SkirmishPlayer.triggerscript".to_owned(), Some(1)),
            ("SkirmishPlayer.triggerscript".to_owned(), Some(2)),
            ("SkirmishAI_raw_.triggerscript".to_owned(), Some(2)),
        ]
    );
}
