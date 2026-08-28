use sim::{
    LoadedGameScenario, TriggerEngine, TriggerUpdate, TriggerValue, VarType,
    load_scenario_from_game_dir,
};

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

/// Run with:
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
    for effect_type in [237, 413, 456, 457] {
        assert!(
            !update.unsupported_effect_types.contains(&effect_type),
            "{scenario} gameplay effect {effect_type} should execute",
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
    modified_unit_scalars: usize,
    modified_prototypes: usize,
    granted_powers: usize,
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
                _ => {}
            }
        }
    }

    fn include_state(&mut self, loaded: &LoadedGameScenario) {
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
        self.modified_unit_scalars += loaded
            .simulation
            .world
            .units
            .iter()
            .filter(|(_, unit)| unit_has_modified_scalar(unit))
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
        ] {
            assert!(count > 0, "campaigns should exercise {name}");
        }
        assert!(self.modified_unit_scalars > 0);
        assert!(self.modified_prototypes > 0);
        assert!(self.granted_powers > 0);
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
