use sim::{
    LoadedGameScenario, TriggerEngine, TriggerUpdate, TriggerValue, VarType,
    load_scenario_from_game_dir,
};

#[path = "scenario_campaign_trigger_loading/assertions.rs"]
mod assertions;

const CAMPAIGN_SCENARIOS: [&str; 17] = [
    "PHXscn01",
    "PHXscn02",
    "PHXscn03",
    "PHXscn04",
    "PHXscn05",
    "PHXscn06",
    "PHXscn07",
    "PHXscn08",
    "PHXscn09",
    "PHXscn10",
    "PHXscn11",
    "PHXscn12",
    "PHXscn13",
    "PHXscn14",
    "PHXscn15",
    "campaignTutorial",
    "campaignTutorialAdvanced",
];

/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-campaign-trigger-loading -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn every_campaign_trigger_catalog_loads_with_retail_variable_aliases() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut coverage = CampaignCoverage::default();

    for scenario in CAMPAIGN_SCENARIOS {
        let mut loaded = load_scenario_from_game_dir(&game_dir, scenario)
            .unwrap_or_else(|error| panic!("{scenario} should load: {error}"));
        let engine = loaded.simulation.world.trigger_engine();
        assert_catalog(scenario, engine);
        coverage.include_variables(engine);

        let update = loaded
            .simulation
            .world
            .update_triggers_with_gameplay(&loaded.content.database, &loaded.simulation.gameplay);
        assert_update_frontier(scenario, &update);
        assert_forbid_state(scenario, &loaded);
        assert_population_state(scenario, &loaded);
        assert_black_map_state(scenario, &loaded);
        assert_score_state(scenario, &loaded);
        assert_obstruction_state(scenario, &loaded);
        assert_design_line_state(scenario, &loaded);
        assert_work_state(scenario, &loaded);
        assert_presentation_state(scenario, &loaded);
        assert_hint_callout_state(scenario, &loaded);
        assert_objective_state(scenario, &loaded);
        assert_entity_visual_state(scenario, &loaded);
        assert_icon_state(scenario, &loaded);
        assertions::assert_tower_wall_state(scenario, &loaded);
        assert_timer_state(scenario, &loaded);
        assert_trigger_created_squads(scenario, &loaded);
        coverage.include_state(&loaded);
    }

    coverage.assert_all();
}

fn assert_catalog(scenario: &str, engine: &TriggerEngine) {
    assert!(
        engine.script_count() > 0,
        "{scenario} should contain scripts"
    );
    assert!(
        engine
            .scripts()
            .any(|(_, script)| !script.triggers.is_empty()),
        "{scenario} should contain triggers",
    );
}

fn assert_update_frontier(scenario: &str, update: &TriggerUpdate) {
    for effect_type in [
        36, 55, 66, 74, 117, 237, 283, 285, 385, 413, 439, 456, 457, 460, 520, 526, 532, 588, 632,
        658, 659, 687, 712, 717, 741, 773, 809, 810, 833, 841, 863, 867, 868, 869, 870, 872, 884,
        922, 935, 936, 937, 938, 984, 1000, 1001, 1007, 1034, 1037, 1044, 1045, 1048, 1054, 1061,
    ] {
        assert!(
            !update.unsupported_effect_types.contains(&effect_type),
            "{scenario} gameplay effect {effect_type} should execute",
        );
    }
    for condition_type in [661, 865, 866, 913, 944] {
        assert!(
            !update.unsupported_condition_types.contains(&condition_type),
            "{scenario} gameplay condition {condition_type} should execute",
        );
    }
    assert!(
        !update.infinite_loop_guard_reached,
        "{scenario}'s initial trigger update should terminate",
    );
    if scenario == "PHXscn01" {
        assert!(
            update.unsupported_condition_types.is_empty(),
            "PHXscn01's reached condition frontier should execute: {:?}",
            update.unsupported_condition_types,
        );
        for effect_type in [124, 265, 480, 630, 729, 811, 818, 836, 838, 839, 1018] {
            assert!(!update.unsupported_effect_types.contains(&effect_type));
        }
        assert_eq!(update.effects_skipped, 0);
    }
    if scenario == "PHXscn04" {
        assert!(!update.unsupported_effect_types.contains(&875));
        assert_eq!(update.effects_skipped, 0);
        assert!(update.skipped_effect_types.is_empty());
    }
}

fn assert_trigger_created_squads(scenario: &str, loaded: &LoadedGameScenario) {
    match scenario {
        "PHXscn04" => assert_phx04_group_transports(loaded),
        "PHXscn07" => assert_phx07_trigger_transports(loaded),
        "PHXscn08" => assert_phx08_attack_move_spawns(loaded),
        _ => {}
    }
}

fn assert_phx04_group_transports(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    let script = world
        .trigger_engine()
        .get_script(1)
        .expect("PHXscn04 scenario script");
    let created = match &script
        .get_variable(21_977)
        .expect("PHXscn04 CreateSquads output")
        .value
    {
        TriggerValue::SquadList(squads) => squads,
        value => panic!("expected PHXscn04 squad-list output, got {value:?}"),
    };
    assert_eq!(created.len(), 2);
    let group_carriers = world
        .squads
        .iter()
        .filter_map(|(_, squad)| {
            let action = squad.transport_fly_in()?;
            action
                .passenger_squad_ids()
                .iter()
                .any(|passenger_id| created.contains(passenger_id))
                .then_some((squad, action))
        })
        .collect::<Vec<_>>();
    assert!(
        !group_carriers.is_empty(),
        "PHXscn04's initial Covenant waves should use group Spirit fly-ins"
    );
    for passenger_id in created {
        assert_eq!(
            group_carriers
                .iter()
                .filter(|(_, action)| action.passenger_squad_ids().contains(passenger_id))
                .count(),
            1,
            "each PHXscn04 CreateSquads passenger needs exactly one carrier"
        );
    }
    for (carrier, action) in group_carriers {
        assert_eq!(carrier.proto_squad_name, "cov_air_spirit_trigger_01");
        assert_eq!(action.phase(), sim::TransportFlyInPhase::Incoming);
        for passenger_id in action.passenger_squad_ids() {
            let passenger = world
                .get_squad(*passenger_id)
                .expect("PHXscn04 group transport passenger");
            assert!(passenger.garrison.is_garrisoned());
            assert!(passenger.unit_ids.iter().all(|unit_id| {
                world
                    .get_unit(*unit_id)
                    .is_some_and(sim::Unit::is_garrisoned)
            }));
        }
    }
}

fn assert_phx07_trigger_transports(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    let carriers = world
        .squads
        .iter()
        .filter_map(|(_, squad)| squad.transport_fly_in().map(|action| (squad, action)))
        .collect::<Vec<_>>();
    assert_eq!(
        carriers.len(),
        1,
        "PHXscn07's initially firing trigger should create one Spirit fly-in"
    );
    for (carrier, action) in carriers {
        assert_eq!(carrier.proto_squad_name, "cov_air_spirit_trigger_01");
        assert_eq!(action.phase(), sim::TransportFlyInPhase::Incoming);
        let passenger = world
            .get_squad(action.passenger_squad_id())
            .expect("PHXscn07 transport passenger");
        assert!(passenger.garrison.is_garrisoned());
        assert!(passenger.unit_ids.iter().all(|unit_id| {
            world
                .get_unit(*unit_id)
                .is_some_and(sim::Unit::is_garrisoned)
        }));
    }
}

fn assert_phx08_attack_move_spawns(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    let script = world
        .trigger_engine()
        .get_script(1)
        .expect("PHXscn08 scenario trigger script");
    for output_id in [13489, 14601] {
        let squads = script
            .get_variable(output_id)
            .and_then(|variable| match &variable.value {
                TriggerValue::SquadList(values) => Some(values),
                _ => None,
            })
            .unwrap_or_else(|| panic!("PHXscn08 trigger squad list {output_id}"));
        let [squad_id] = squads.as_slice() else {
            panic!("PHXscn08 trigger squad list {output_id} should contain one squad");
        };
        assert!(
            world
                .get_squad(*squad_id)
                .is_some_and(sim::Squad::is_executing_attack_move),
            "PHXscn08 spawned squad {squad_id:?} should retain its attack-move rally order"
        );
    }
}

fn assert_timer_state(scenario: &str, loaded: &LoadedGameScenario) {
    let output_id = match scenario {
        "PHXscn03" => 4619,
        "PHXscn15" => 1152,
        _ => return,
    };
    assert!(
        loaded.simulation.world.game_timers().next().is_none(),
        "{scenario}'s initially reached destroy path should leave no active game timer"
    );
    let output = loaded
        .simulation
        .world
        .trigger_engine()
        .get_script(1)
        .and_then(|script| script.get_variable(output_id))
        .map(|variable| &variable.value);
    assert_eq!(
        output,
        Some(&TriggerValue::Int(-1)),
        "{scenario}'s later create path should not have run during initial activation"
    );
}

fn assert_forbid_state(scenario: &str, loaded: &LoadedGameScenario) {
    if scenario != "PHXscn13" {
        return;
    }
    let database = &loaded.content.database;
    let world = &loaded.simulation.world;
    let player_three = world.get_player(3).expect("PHXscn13 player 3");
    assert!(player_three.is_object_forbidden(database, 176));
    for prototype_id in [902, 2454, 915, 2488] {
        assert!(player_three.is_squad_forbidden(database, prototype_id));
    }
    let player_four = world.get_player(4).expect("PHXscn13 player 4");
    for prototype_id in [902, 2488] {
        assert!(player_four.is_squad_forbidden(database, prototype_id));
    }
    let player_five = world.get_player(5).expect("PHXscn13 player 5");
    for prototype_id in [928, 1155] {
        assert!(player_five.is_squad_forbidden(database, prototype_id));
    }
}

fn assert_population_state(scenario: &str, loaded: &LoadedGameScenario) {
    let expected = match scenario {
        "campaignTutorialAdvanced" => Some((1, 6.0)),
        _ => None,
    };
    let Some((player_id, expected_limit)) = expected else {
        return;
    };
    let unit_population_id = loaded
        .content
        .database
        .game_data
        .as_ref()
        .and_then(|data| data.pops.as_ref())
        .and_then(|pops| {
            pops.entries
                .iter()
                .position(|name| name.eq_ignore_ascii_case("Unit"))
        })
        .expect("campaign database Unit population type");
    let population = loaded
        .simulation
        .world
        .get_player(player_id)
        .and_then(|player| player.get_population(unit_population_id))
        .expect("campaign player Unit population state");
    assert!((population.cap - expected_limit).abs() < f32::EPSILON);
    assert!((population.max - expected_limit).abs() < f32::EPSILON);
}

fn assert_black_map_state(scenario: &str, loaded: &LoadedGameScenario) {
    if scenario == "PHXscn12" {
        assert!(
            loaded.simulation.world.black_map_is_cleared(),
            "PHXscn12 startup should clear retail black-map exploration"
        );
    }
}

fn assert_score_state(scenario: &str, loaded: &LoadedGameScenario) {
    if scenario != "PHXscn01" {
        return;
    }
    let info = loaded
        .simulation
        .world
        .scenario_score_info()
        .expect("PHXscn01 startup should configure campaign scoring");
    assert_eq!(info.scenario_id(), -1);
    assert_eq!(
        info.combat_bonus_min_multiplier().to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(
        info.combat_bonus_max_multiplier().to_bits(),
        10.0_f32.to_bits()
    );
    assert_eq!(info.mission_min_par_time_ms(), 300_000);
    assert_eq!(info.mission_max_par_time_ms(), 720_000);
    assert_eq!(info.grade_score_thresholds(), [27_000, 18_000, 12_000]);
}

fn assert_obstruction_state(scenario: &str, loaded: &LoadedGameScenario) {
    if scenario != "PHXscn03" {
        return;
    }
    let prototype_name = obstruction_prototype_name(loaded).expect("Obstruction code prototype");
    let obstruction = loaded
        .simulation
        .world
        .units
        .iter()
        .map(|(_, unit)| unit)
        .find(|unit| unit.proto_object_name.eq_ignore_ascii_case(prototype_name))
        .expect("PHXscn03 startup should create its bridge obstruction");
    assert_eq!(obstruction.base.player_id, 0);
    assert_eq!(
        obstruction.obstruction_half_extents,
        sim::physics::BoxCollider::new(glam::Vec3::new(29.0, 3.0, 30.0), glam::Vec3::ZERO)
            .half_extents
    );
    assert_eq!(
        obstruction
            .physics
            .as_ref()
            .map(sim::PhysicsBody::motion_type),
        Some(sim::physics::MotionType::Static)
    );
}

fn obstruction_prototype_name(loaded: &LoadedGameScenario) -> Option<&str> {
    loaded
        .content
        .database
        .game_data
        .as_ref()?
        .code_proto_objects
        .as_ref()?
        .entries
        .iter()
        .find(|mapping| mapping.object_type.eq_ignore_ascii_case("Obstruction"))
        .map(|mapping| mapping.proto_name.trim())
}

fn assert_design_line_state(scenario: &str, loaded: &LoadedGameScenario) {
    if scenario != "PHXscn10" {
        return;
    }
    let world = &loaded.simulation.world;
    let points = world
        .design_line_points(1875)
        .expect("PHXscn10 AlphaToLZ design line");
    assert_eq!(points.len(), 4);
    assert_eq!(points[0], glam::Vec3::new(159.6148, 6.0555, 147.6894));
    assert_eq!(points[1], glam::Vec3::new(213.9123, 9.1885, 348.0874));

    let output = world
        .trigger_engine()
        .get_script(1)
        .and_then(|script| script.get_variable(18907))
        .map(|variable| &variable.value)
        .expect("PHXscn10 AlphaToLZ trigger output");
    let TriggerValue::VectorList(output_points) = output else {
        panic!("PHXscn10 design-line output should remain a typed vector list");
    };
    assert_eq!(output_points.len(), points.len());
    for (output_point, world_point) in output_points.iter().zip(points) {
        assert_eq!(output_point.x.to_bits(), world_point.x.to_bits());
        assert_eq!(output_point.y.to_bits(), world_point.y.to_bits());
        assert_eq!(output_point.z.to_bits(), world_point.z.to_bits());
    }
}

fn assert_work_state(scenario: &str, loaded: &LoadedGameScenario) {
    let expected = match scenario {
        "PHXscn03" | "PHXscn15" => 3,
        "PHXscn04" => 12,
        "PHXscn14" => 6,
        _ => return,
    };
    let garrisoning = loaded
        .simulation
        .world
        .squads
        .iter()
        .filter_map(|(_, squad)| {
            let sim::SquadContainmentState::Garrisoning { target, range, .. } =
                squad.garrison.state()
            else {
                return None;
            };
            Some((target, range))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        garrisoning.len(),
        expected,
        "{scenario} startup Work effects should create contextual garrison orders"
    );
    for (target, range) in garrisoning {
        assert_eq!(range.to_bits(), 5.0_f32.to_bits());
        assert!(
            loaded
                .simulation
                .world
                .get_unit(target)
                .is_some_and(|unit| unit.garrison.can_contain()),
            "{scenario} Work target should resolve through the scenario database"
        );
    }
}

fn assert_presentation_state(scenario: &str, loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    match scenario {
        "PHXscn01" => assert!(!world.hud_item_enabled(sim::HudItem::Resources)),
        "PHXscn03" => {
            assert_eq!(
                world.minimap_rotation_degrees().to_bits(),
                180.0_f32.to_bits()
            );
            assert!(world.chats_enabled());
            let shake = world.camera_shake(1).expect("PHXscn03 camera shake");
            assert_eq!(shake.strength().to_bits(), 2.0_f32.to_bits());
            assert_eq!(shake.conservation_factor().to_bits(), 0.5_f32.to_bits());
            let rumbles = world.rumble_requests(2).collect::<Vec<_>>();
            let [rumble] = rumbles.as_slice() else {
                panic!("PHXscn03 should start one player-two rumble, got {rumbles:?}");
            };
            assert_eq!(rumble.id(), 0);
            assert_eq!(rumble.left().rumble_type(), Some("Fixed"));
            assert_eq!(rumble.right().rumble_type(), Some("Fixed"));
            assert_eq!(rumble.left().strength().to_bits(), 0.75_f32.to_bits());
            assert_eq!(rumble.duration_seconds().to_bits(), 1.0_f32.to_bits());
            assert!(!rumble.looped());
            assert!(rumble.pattern().is_none());
            for player_id in 1..world.player_count() {
                let player_id = u8::try_from(player_id).unwrap();
                assert!(world.objective_pointers(player_id).next().is_none());
            }
        }
        "PHXscn04" => assert!(!world.render_terrain_skirt_enabled()),
        "PHXscn05" => assert_eq!(
            world.minimap_rotation_degrees().to_bits(),
            (-160.0_f32).to_bits()
        ),
        "PHXscn06" => assert_phx06_presentation(loaded),
        "PHXscn07" => assert_phx07_fade(loaded),
        "PHXscn11" | "PHXscn12" => assert!(!world.minimap_skirt_mirroring()),
        "campaignTutorial" => assert_tutorial_presentation(loaded, false),
        "campaignTutorialAdvanced" => assert_tutorial_presentation(loaded, true),
        _ => {}
    }
}

fn assert_phx07_fade(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    let overlay = world
        .screen_fade_overlay()
        .expect("PHXscn07 should begin its authored black transition");
    assert_eq!(overlay.revision(), 1);
    assert_eq!(overlay.color(), [0, 0, 0]);
    assert_eq!(overlay.opacity().to_bits(), 0.0_f32.to_bits());
    assert!(!world.screen_fade_completed());
}

fn assert_phx06_presentation(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    assert!(!world.render_terrain_skirt_enabled());
    assert_eq!(
        world.minimap_rotation_degrees().to_bits(),
        (-85.0_f32).to_bits()
    );
    for player_id in [1, 4] {
        let state = world.player_presentation_state(player_id);
        assert!(state.camera_controls.scroll);
        assert!(state.camera_controls.yaw);
        assert!(state.camera_controls.zoom);
        let directive = state
            .camera_directive
            .unwrap_or_else(|| panic!("PHXscn06 player {player_id} camera directive"));
        assertions::assert_vec3_close(
            directive.location.expect("PHXscn06 camera location"),
            glam::Vec3::new(266.0444, 69.3125, 235.5218),
            0.001,
        );
        assertions::assert_vec3_close(
            directive.direction.expect("PHXscn06 camera direction"),
            glam::Vec3::new(0.6442, 0.0, 0.7648),
            0.0001,
        );
        assert!(directive.hover_height_offset.is_none());
    }
}

fn assert_hint_callout_state(scenario: &str, loaded: &LoadedGameScenario) {
    let expected = match scenario {
        "PHXscn03" => Some((22934, glam::Vec3::new(368.8735, -33.8938, 635.7485))),
        "PHXscn11" => Some((25992, glam::Vec3::new(469.1019, 35.2007, 572.122))),
        "PHXscn15" => Some((24788, glam::Vec3::new(249.8237, -2.2667, 492.1157))),
        _ => None,
    };
    let Some((string_id, location)) = expected else {
        return;
    };
    let callouts = loaded.simulation.world.hint_callouts().collect::<Vec<_>>();
    let [callout] = callouts.as_slice() else {
        panic!("{scenario} should leave one active hint callout, got {callouts:?}");
    };
    assert_eq!(callout.id(), 0);
    assert_eq!(callout.widget_slot(), 0);
    assert_eq!(callout.string_id(), string_id);
    let sim::HintCalloutAnchor::Location(actual) = callout.anchor() else {
        panic!("{scenario} callout should use its authored location");
    };
    assertions::assert_vec3_close(actual, location, 0.0001);

    if scenario == "PHXscn03" {
        let output = loaded
            .simulation
            .world
            .trigger_engine()
            .get_script(1)
            .and_then(|script| script.get_variable(10301))
            .map(|variable| &variable.value);
        assert_eq!(output, Some(&TriggerValue::Int(0)));
    }
}

fn assert_tutorial_presentation(loaded: &LoadedGameScenario, advanced: bool) {
    let world = &loaded.simulation.world;
    assert!(world.screen_blur_enabled());
    assert_eq!(world.circle_menu_reset_revision(), u32::from(advanced));
    let state = world.player_presentation_state(1);
    assert!(!state.camera_controls.scroll);
    assert!(!state.camera_controls.yaw);
    assert!(!state.camera_controls.zoom);
    assert_eq!(state.ignore_dpad, !advanced);
    assert_eq!(state.user_lock_owner_script, Some(1));
}

fn assert_objective_state(scenario: &str, loaded: &LoadedGameScenario) {
    let expected = match scenario {
        "PHXscn03" => Some((11, 45, -1, 10958, 45)),
        "PHXscn05" => Some((9, 5, -1, 8675, 5)),
        "PHXscn08" => Some((15, 4, 4, 14273, 4)),
        "PHXscn11" => Some((8, 100, -1, 5293, 100)),
        "PHXscn15" => Some((8, 3, -1, 4527, 3)),
        _ => None,
    };
    let Some((objective_id, final_count, current_count, output_id, output_count)) = expected else {
        return;
    };
    let world = &loaded.simulation.world;
    let objective = world
        .objective(objective_id)
        .unwrap_or_else(|| panic!("{scenario} objective {objective_id}"));
    assert_eq!(objective.final_count(), final_count);
    assert_eq!(objective.current_count(), current_count);
    let Some(output) = world
        .trigger_engine()
        .get_script(1)
        .and_then(|script| script.get_variable(output_id))
        .map(|variable| &variable.value)
    else {
        panic!("{scenario} objective counter output {output_id}");
    };
    assert_eq!(output, &TriggerValue::Int(output_count));

    if scenario == "PHXscn08" {
        assert_eq!(objective.score(), 500);
        assert!(objective.assigned_to_player(1));
        assert!(objective.assigned_to_player(2));
        assert!(!objective.required());
        assert_eq!(world.objective(14).unwrap().final_count(), -1);
    }
}

fn assert_entity_visual_state(scenario: &str, loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    if scenario == "PHXscn03" {
        for (scenario_id, animation_type, output_id) in
            [(1958, "Death", 4548), (616, "Research", 7371)]
        {
            let entity_id = loaded
                .simulation
                .get_entity_id(scenario_id)
                .unwrap_or_else(|| panic!("PHXscn03 scenario object {scenario_id}"));
            let animation = world
                .entity_scripted_animation(entity_id)
                .unwrap_or_else(|| panic!("PHXscn03 {animation_type} animation"));
            assert_eq!(animation.animation_type(), animation_type);
            assert!(animation.asset_path().is_some());
            assert_eq!(animation.duration_ms(), 2_000);
            assert_eq!(
                animation.normalized_position(world.game_time()).to_bits(),
                0.0_f32.to_bits()
            );
            let output = world
                .trigger_engine()
                .get_script(1)
                .and_then(|script| script.get_variable(output_id))
                .map(|variable| &variable.value);
            assert_eq!(output, Some(&TriggerValue::Time(2_000)));
        }
    }
    if scenario == "PHXscn01" {
        let flashes = world
            .units
            .iter()
            .filter_map(|(unit_id, _)| world.entity_targeting_selection(unit_id))
            .collect::<Vec<_>>();
        assert!(
            !flashes.is_empty(),
            "PHXscn01 startup callouts should flash units"
        );
        for flash in flashes {
            assert_eq!(flash.color(), [255, 255, 0, 255]);
            assert_eq!(flash.expires_at_ms(), Some(3_000));
            assert_eq!(flash.scroll_speed().to_bits(), (-4.0_f32).to_bits());
            assert_eq!(flash.intensity().to_bits(), 20.0_f32.to_bits());
        }
    }
    if scenario == "PHXscn15" {
        let policies = world
            .units
            .iter()
            .filter_map(|(unit_id, _)| world.entity_dopple_policy(unit_id))
            .filter(|policy| policy.reset_revision() > 0)
            .collect::<Vec<_>>();
        assert!(
            !policies.is_empty(),
            "PHXscn15 startup should reset dopples"
        );
        assert!(policies.iter().all(|policy| policy.dopples()));
        assert!(policies.iter().any(|policy| policy.gray_map_dopples()));
        assert!(policies.iter().any(|policy| !policy.gray_map_dopples()));
        assert!(
            policies
                .iter()
                .all(|policy| policy.visibility_update_pending())
        );
    }
}

fn assert_icon_state(scenario: &str, loaded: &LoadedGameScenario) {
    match scenario {
        "PHXscn04" => assert_phx04_location_icons(loaded),
        "PHXscn14" => assert_phx14_attached_icons(loaded),
        _ => {}
    }
}

fn assert_phx04_location_icons(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    let script = world
        .trigger_engine()
        .get_script(1)
        .expect("PHXscn04 scenario trigger script");
    let mut outputs = Vec::new();
    for output_id in [19593, 19594] {
        let Some(TriggerValue::Object(icon_id)) = script
            .get_variable(output_id)
            .map(|variable| &variable.value)
        else {
            panic!("PHXscn04 icon output {output_id}")
        };
        assert_ne!(*icon_id, sim::EntityId::INVALID);
        assert_eq!(icon_id.class(), Some(sim::EntityClass::Object));
        assert!(
            world.get_object(*icon_id).is_none(),
            "PHXscn04 Init destroys each temporary RunTo icon after creating it"
        );
        outputs.push(*icon_id);
    }
    assert_ne!(outputs[0], outputs[1]);
}

fn assert_phx14_attached_icons(loaded: &LoadedGameScenario) {
    let world = &loaded.simulation.world;
    let Some(TriggerValue::Squad(squad_id)) = world
        .trigger_engine()
        .get_script(1)
        .and_then(|script| script.get_variable(2140))
        .map(|variable| &variable.value)
    else {
        panic!("PHXscn14 FTLCoreSquad output")
    };
    let squad = world
        .get_squad(*squad_id)
        .expect("PHXscn14 FTLCoreSquad should remain live");
    assert!(!squad.unit_ids.is_empty());
    for unit_id in &squad.unit_ids {
        let [attachment_id] = world
            .entity_object_state(*unit_id)
            .expect("PHXscn14 FTL child object state")
            .attachments()
        else {
            panic!("PHXscn14 FTL child should own one icon attachment")
        };
        let attachment = world
            .get_object(*attachment_id)
            .expect("PHXscn14 live icon attachment");
        assert_eq!(attachment.proto_object_name, "sys_icon_27_01");
        assert!(attachment.icon().is_some());
        assert_eq!(attachment.object_state.attached_to(), Some(*unit_id));
        assert_eq!(
            attachment.base.position,
            world.get_unit(*unit_id).unwrap().base.position
        );
    }
}

#[derive(Default)]
struct CampaignCoverage {
    command_types: usize,
    diplomacy: usize,
    player_lists: usize,
    team_lists: usize,
    event_types: usize,
    proto_squad_lists: usize,
    design_line_values: usize,
    design_line_lists: usize,
    modified_unit_scalars: usize,
    modified_prototypes: usize,
    granted_powers: usize,
    revealers: usize,
    fog_disabled_scenarios: usize,
    playable_bounds_scenarios: usize,
    rally_points: usize,
    score_configured_scenarios: usize,
    obstruction_units: usize,
    design_lines: usize,
    contextual_work_orders: usize,
    objectives: usize,
    flashed_units: usize,
    dopple_policy_units: usize,
    tower_wall_actions: usize,
    hint_callouts: usize,
    icon_objects: usize,
    attached_objects: usize,
}

impl CampaignCoverage {
    fn include_variables(&mut self, engine: &TriggerEngine) {
        for variable in engine
            .scripts()
            .flat_map(|(_, script)| script.variables.values())
            .filter(|variable| !variable.is_null)
        {
            match variable.var_type {
                VarType::TechDataCommandType => {
                    assert!(matches!(variable.value, TriggerValue::Int(_)));
                    self.command_types += 1;
                }
                VarType::RelationType => {
                    assert!(matches!(variable.value, TriggerValue::Int(_)));
                    self.diplomacy += 1;
                }
                VarType::PlayerList => {
                    assert!(matches!(variable.value, TriggerValue::PlayerList(_)));
                    self.player_lists += 1;
                }
                VarType::TeamList => {
                    assert!(matches!(variable.value, TriggerValue::TeamList(_)));
                    self.team_lists += 1;
                }
                VarType::EventType => {
                    assert!(matches!(variable.value, TriggerValue::Int(_)));
                    self.event_types += 1;
                }
                VarType::ProtoSquadList => {
                    assert!(matches!(variable.value, TriggerValue::ProtoSquadList(_)));
                    self.proto_squad_lists += 1;
                }
                VarType::DesignLine => {
                    assert!(matches!(variable.value, TriggerValue::DesignLine(_)));
                    self.design_line_values += 1;
                }
                VarType::DesignLineList => {
                    assert!(matches!(variable.value, TriggerValue::DesignLineList(_)));
                    self.design_line_lists += 1;
                }
                _ => {}
            }
        }
    }

    fn include_state(&mut self, loaded: &LoadedGameScenario) {
        self.rally_points += loaded
            .simulation
            .world
            .players()
            .filter(|player| player.rally_point().is_some())
            .count();
        self.fog_disabled_scenarios += usize::from(!loaded.simulation.world.fog_of_war_enabled());
        self.playable_bounds_scenarios +=
            usize::from(loaded.simulation.world.playable_bounds().is_some());
        self.score_configured_scenarios +=
            usize::from(loaded.simulation.world.scenario_score_info().is_some());
        self.design_lines += loaded.simulation.world.design_line_count();
        self.objectives += loaded.simulation.world.objectives().count();
        self.include_unit_state(loaded);
        self.include_object_state(loaded);
        self.granted_powers += loaded
            .simulation
            .world
            .players()
            .map(|player| player.power_entries().len())
            .sum::<usize>();
        self.modified_prototypes += loaded
            .simulation
            .world
            .players()
            .map(|player| player.technologies.runtime_proto_modification_count())
            .sum::<usize>();
    }

    fn include_unit_state(&mut self, loaded: &LoadedGameScenario) {
        self.flashed_units += loaded
            .simulation
            .world
            .units
            .iter()
            .filter(|(unit_id, _)| {
                loaded
                    .simulation
                    .world
                    .entity_targeting_selection(*unit_id)
                    .is_some()
            })
            .count();
        self.dopple_policy_units += loaded
            .simulation
            .world
            .units
            .iter()
            .filter(|(unit_id, _)| {
                loaded
                    .simulation
                    .world
                    .entity_dopple_policy(*unit_id)
                    .is_some_and(|policy| policy.reset_revision() > 0)
            })
            .count();
        self.tower_wall_actions += loaded
            .simulation
            .world
            .units
            .iter()
            .filter(|(_, unit)| unit.tower_wall.is_some())
            .count();
        self.hint_callouts += loaded.simulation.world.hint_callouts().count();
        self.contextual_work_orders += loaded
            .simulation
            .world
            .squads
            .iter()
            .filter(|(_, squad)| {
                matches!(
                    squad.garrison.state(),
                    sim::SquadContainmentState::Garrisoning { .. }
                )
            })
            .count();
        if let Some(prototype_name) = obstruction_prototype_name(loaded) {
            self.obstruction_units += loaded
                .simulation
                .world
                .units
                .iter()
                .filter(|(_, unit)| unit.proto_object_name.eq_ignore_ascii_case(prototype_name))
                .count();
        }
        self.modified_unit_scalars += loaded
            .simulation
            .world
            .units
            .iter()
            .filter(|(_, unit)| unit_has_modified_scalar(unit))
            .count();
    }

    fn include_object_state(&mut self, loaded: &LoadedGameScenario) {
        for (object_id, object) in loaded.simulation.world.objects.iter() {
            assert!(
                object.revealer().is_some() || object.icon().is_some() || object.is_visual(),
                "campaign class-0 object {object_id:?} should expose a supported sim kind"
            );
        }
        self.icon_objects += loaded.simulation.world.icon_objects().count();
        self.attached_objects += loaded
            .simulation
            .world
            .objects
            .iter()
            .filter(|(_, object)| object.object_state.attached_to().is_some())
            .count();
        self.revealers += loaded
            .simulation
            .world
            .objects
            .iter()
            .filter(|(_, object)| object.revealer().is_some())
            .count();
    }

    fn assert_all(&self) {
        for (count, name) in [
            (self.command_types, "CommandType"),
            (self.diplomacy, "Diplomacy"),
            (self.player_lists, "PlayerList"),
            (self.team_lists, "TeamList"),
            (self.event_types, "EventType"),
            (self.proto_squad_lists, "ProtoSquadList"),
            (self.design_line_values, "DesignLine"),
            (self.design_line_lists, "DesignLineList"),
        ] {
            assert!(count > 0, "campaigns should exercise {name}");
        }
        assert!(self.modified_unit_scalars > 0);
        assert!(self.modified_prototypes > 0);
        assert!(self.granted_powers > 0);
        assert!(self.revealers > 0);
        assert!(self.fog_disabled_scenarios > 0);
        assert!(self.playable_bounds_scenarios > 0);
        assert!(self.rally_points > 0);
        assert!(self.score_configured_scenarios > 0);
        assert!(self.obstruction_units > 0);
        assert!(self.design_lines > 0);
        assert!(self.contextual_work_orders > 0);
        assert!(self.objectives > 0);
        assert!(self.flashed_units > 0);
        assert!(self.dopple_policy_units > 0);
        assert!(self.tower_wall_actions > 0);
        assert!(self.hint_callouts > 0);
        assert!(self.icon_objects > 0);
        assert!(self.attached_objects > 0);
    }
}

fn unit_has_modified_scalar(unit: &sim::Unit) -> bool {
    [
        unit.accuracy_scalar,
        unit.work_rate_scalar,
        unit.damage_multiplier,
        unit.line_of_sight_scalar,
        unit.velocity_scalar,
        unit.weapon_range_scalar,
        unit.damage_taken_multiplier,
    ]
    .into_iter()
    .any(|scalar| (scalar - 1.0).abs() > f32::EPSILON)
}
