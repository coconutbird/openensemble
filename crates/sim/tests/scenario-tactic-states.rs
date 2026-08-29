use std::collections::BTreeSet;

use glam::Vec3;
use sim::{
    CommandEntry, CommandExecutor, LoadedGameScenario, QueuedCommand, SquadMode, WorkCommand,
    load_scenario_from_game_dir, object_prototype_id, spawn_object_at, spawn_squad_at,
    squad_prototype_id,
};

const GHOST: &str = "cov_veh_ghost_01";
const MARINE: &str = "unsc_inf_marine_01";
const SUICIDE_GRUNT: &str = "cov_inf_suicideGrunt_01";
const METHANE_TANK: &str = "env_harvest_methanetank_01";
const SCENARIOS: [&str; 16] = [
    "blood_gulch",
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
];

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-tactic-states -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn layered_scenarios_preserve_shipped_tactic_state_contracts() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut state_definitions = BTreeSet::new();
    let mut transitions = BTreeSet::new();

    for scenario in SCENARIOS {
        let loaded = load_scenario_from_game_dir(&game_dir, scenario)
            .unwrap_or_else(|error| panic!("installed scenario {scenario} should load: {error}"));
        collect_tactic_state_contracts(&loaded, &mut state_definitions, &mut transitions);
    }

    assert_eq!(state_definitions, expected_state_definitions());
    assert_eq!(
        transitions,
        BTreeSet::from(["cov_veh_ghost_01|PersistentCollisionAttack|RamState|false".to_owned()])
    );
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_ghost_ram_drives_authoritative_tactic_state() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario should load");
    assert_ghost_ram_state_transition(loaded);
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_suicide_grunt_detonates_through_authoritative_state() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario should load");
    assert_suicide_grunt_detonation(&mut loaded);
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_methane_tank_exposes_physics_death_detonation() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut diagnostics = Vec::new();
    for scenario in SCENARIOS {
        let mut loaded = load_scenario_from_game_dir(&game_dir, scenario)
            .unwrap_or_else(|error| panic!("installed scenario {scenario} should load: {error}"));
        let gameplay = &loaded.simulation.gameplay;
        let Some(replacement) = gameplay.physics_replacement(METHANE_TANK) else {
            diagnostics.extend(
                gameplay
                    .physics_replacement_issues()
                    .iter()
                    .filter(|issue| issue.proto_object_name().eq_ignore_ascii_case(METHANE_TANK))
                    .map(|issue| format!("{scenario}: {}: {}", issue.asset_path(), issue.reason())),
            );
            continue;
        };
        assert_eq!(replacement.physics_info(), "env_harvest_methanetank");
        assert!(replacement.material().mass > 0.0);
        assert!(replacement.collider().half_extents.min_element() > 0.0);
        let detonate = gameplay
            .first_detonate_action(METHANE_TANK)
            .expect("shipped methane-tank Detonate action");
        assert_eq!(detonate.action_name(), "DetonateDeath");
        assert_eq!(
            detonate
                .duration()
                .map(sim::DetonateDurationProfile::seconds),
            Some(0.5)
        );
        assert_close(detonate.damage_per_second(), 6_000.0);
        assert_eq!(
            detonate.area_damage().map(|profile| profile.radius),
            Some(15.0)
        );
        assert_marine_triggers_methane_replacement(&mut loaded);
        return;
    }
    panic!(
        "no installed scenario resolved methane-tank replacement physics:\n{}",
        diagnostics.join("\n")
    );
}

fn assert_marine_triggers_methane_replacement(loaded: &mut LoadedGameScenario) {
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();

    let origin = Vec3::new(10_000.0, 0.0, 10_000.0);
    let marine_proto = squad_prototype_id(database, MARINE).expect("shipped Marine squad");
    let marine_squad = spawn_squad_at(world, database, 1, marine_proto, origin, Vec3::X)
        .expect("attacking Marine squad");
    let methane_proto =
        object_prototype_id(database, METHANE_TANK).expect("shipped methane-tank object");
    let methane_id = spawn_object_at(
        world,
        database,
        2,
        methane_proto,
        origin + Vec3::X * 8.0,
        Vec3::NEG_X,
    )
    .expect("enemy methane tank");
    world.get_unit_mut(methane_id).unwrap().hitpoints = 1.0;
    let marine_ids = world.get_squad(marine_squad).unwrap().unit_ids.clone();
    let initial_marine_hitpoints = unit_hitpoints(world, &marine_ids);
    assert!(world.issue_attack_order(1, marine_squad, methane_id, 30.0));

    let mut replacement_id = None;
    for _ in 0..240 {
        world.update_entities_with_gameplay(0.05, gameplay);
        replacement_id = world.units.iter().find_map(|(unit_id, unit)| {
            (unit.is_physics_replacement()
                && unit.proto_object_name.eq_ignore_ascii_case(METHANE_TANK)
                && unit.base.position.distance(origin) < 30.0)
                .then_some(unit_id)
        });
        if replacement_id.is_some() {
            break;
        }
    }
    let replacement_id = replacement_id.expect("Marine fire should create a physics replacement");
    let replacement = world.get_unit(replacement_id).unwrap();
    assert_eq!(replacement.detonate_action_name(), Some("DetonateDeath"));
    assert!(replacement.detonate_countdown_remaining_ms().is_some());

    for _ in 0..60 {
        world.update_entities_with_gameplay(0.05, gameplay);
        if world.get_unit(replacement_id).is_none() {
            break;
        }
    }
    assert!(
        world.get_unit(replacement_id).is_none(),
        "authored methane replacement should detonate and clean itself up"
    );
    assert_close(unit_hitpoints(world, &marine_ids), initial_marine_hitpoints);
}

fn collect_tactic_state_contracts(
    loaded: &LoadedGameScenario,
    states: &mut BTreeSet<String>,
    transitions: &mut BTreeSet<String>,
) {
    for object in loaded.simulation.gameplay.objects() {
        for state in &object.tactics().states {
            states.insert(format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                object.proto_object_name().to_ascii_lowercase(),
                state.name,
                optional(state.idle_anim.as_deref()),
                optional(state.walk_anim.as_deref()),
                optional(state.jog_anim.as_deref()),
                optional(state.run_anim.as_deref()),
                optional(state.death_anim.as_deref()),
                state.actions.join(","),
            ));
        }
        for action in &object.tactics().actions {
            if action.new_tactic_state.is_some() || action.clear_tactic_state == Some(true) {
                transitions.insert(format!(
                    "{}|{}|{}|{}",
                    object.proto_object_name().to_ascii_lowercase(),
                    action.name,
                    optional(action.new_tactic_state.as_deref()),
                    action.clear_tactic_state == Some(true),
                ));
            }
        }
    }
}

fn expected_state_definitions() -> BTreeSet<String> {
    BTreeSet::from([
        "cov_inf_suicidegrunt_01||-|SuicideRun|SuicideRun|SuicideRun|-|".to_owned(),
        "cov_veh_ghost_01|RamState|-|Run|Run|Run|-|".to_owned(),
    ])
}

struct GhostRamPair {
    attacker_squad: sim::EntityId,
    target_squad: sim::EntityId,
    attacker_id: sim::EntityId,
    target_id: sim::EntityId,
    initial_target_hitpoints: f32,
}

fn assert_ghost_ram_state_transition(mut loaded: LoadedGameScenario) {
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let ram_state = assert_ghost_catalog(gameplay);
    let physics_profile = gameplay
        .ground_vehicle_physics(GHOST)
        .expect("shipped Ghost physics chain");
    let pair = spawn_ghost_ram_pair(&mut loaded.simulation.world, database, physics_profile);
    issue_ghost_ram(
        database,
        gameplay,
        &mut loaded.simulation.world,
        pair.attacker_squad,
        pair.target_squad,
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_squad(pair.attacker_squad)
            .unwrap()
            .mode,
        SquadMode::HitAndRun
    );
    loaded
        .simulation
        .world
        .update_entities_with_gameplay(0.05, gameplay);
    assert_ghost_ram_impact(&loaded.simulation.world, &pair, ram_state);

    loaded
        .simulation
        .world
        .update_entities_with_gameplay(0.05, gameplay);
    let attacker = loaded.simulation.world.get_unit(pair.attacker_id).unwrap();
    assert_eq!(attacker.tactic_state(), None);
    assert_eq!(attacker.tactic_state_revision(), 2);
}

fn assert_ghost_catalog(gameplay: &sim::GameplayCatalog) -> sim::TacticStateId {
    let profile = gameplay
        .collision_attack(GHOST)
        .expect("shipped Ghost collision attack");
    let ram_state = profile
        .new_tactic_state
        .expect("Ghost collision action should enter RamState");
    let state = gameplay
        .tactic_state(GHOST, ram_state)
        .expect("Ghost RamState profile");
    assert_eq!(state.name(), "RamState");
    assert_eq!(state.run_animation(), Some("Run"));
    let physics_profile = gameplay
        .ground_vehicle_physics(GHOST)
        .expect("shipped Ghost physics chain");
    assert_eq!(physics_profile.kind(), sim::GroundVehicleKind::Ghost);
    assert_eq!(
        physics_profile.collider().half_extents,
        Vec3::new(1.5, 1.0, 3.0)
    );
    assert_close(physics_profile.collider().center_offset.y, 2.28);
    assert_close(physics_profile.material().mass, 150.0);
    assert_close(physics_profile.material().friction, 2.0);
    assert_close(physics_profile.material().restitution, 0.5);
    assert_close(physics_profile.material().angular_damping, 0.01);
    ram_state
}

#[derive(Clone, Copy)]
struct SuicideContract {
    state_zero: sim::TacticStateId,
    command_id: u8,
    ability_id: u8,
}

struct SuicidePair {
    source: sim::EntityId,
    target: sim::EntityId,
    source_members: Vec<sim::EntityId>,
    survivor_id: sim::EntityId,
    initial_target_hitpoints: f32,
}

fn assert_suicide_grunt_detonation(loaded: &mut LoadedGameScenario) {
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let contract = assert_suicide_catalog(database, gameplay);
    let pair = spawn_suicide_pair(&mut loaded.simulation.world, database);
    issue_suicide_detonate(
        database,
        &mut loaded.simulation.world,
        pair.source,
        pair.target,
        contract.command_id,
    );
    assert_suicide_lifecycle(&mut loaded.simulation.world, gameplay, &pair, contract);
    assert_suicide_upgrade_selection(
        &mut loaded.simulation.world,
        database,
        gameplay,
        contract.command_id,
    );
}

fn assert_suicide_catalog(
    database: &pipeline::database::hw1::Database,
    gameplay: &sim::GameplayCatalog,
) -> SuicideContract {
    let profile = gameplay
        .select_detonate_action(SUICIDE_GRUNT, &sim::AttackQuery::default(), |action| {
            action.start_disabled != Some(true)
        })
        .expect("shipped Suicide Grunt Detonate action");
    assert_eq!(profile.action_name(), "SuicideBomb");
    assert_eq!(profile.weapon_name(), "Bomb");
    assert_eq!(profile.weapon_type(), Some("Basic"));
    assert_eq!(
        profile.projectile(),
        Some("fx_proj_suicide_plasmaGrenade_01")
    );
    assert_close(profile.work_range(), 0.1);
    assert_close(profile.glowy_range(), 50.0);
    assert_close(profile.velocity_scalar(), 1.0);
    assert_close(profile.damage_per_second(), 1_400.0);
    assert_close(profile.area_damage().unwrap().radius, 12.0);
    let state_zero = sim::TacticStateId::from_index(0).expect("state zero");
    assert_eq!(
        gameplay
            .tactic_state(SUICIDE_GRUNT, state_zero)
            .and_then(sim::TacticStateProfile::run_animation),
        Some("SuicideRun")
    );
    let command_id = database_ability_id(database, "Command");
    let ability = gameplay
        .resolve_order_ability(SUICIDE_GRUNT, command_id)
        .expect("Command should resolve through Suicide Grunt AbilityCommand");
    assert_eq!(ability.name(), "CovGruntSuicideExplode");
    assert_eq!(
        ability.recovery_start(),
        Some(sim::AbilityRecoveryStart::Attack)
    );
    assert_eq!(ability.recovery_type(), Some(sim::RecoveryType::Ability));
    assert_close(ability.recovery_time(), 20.0);
    SuicideContract {
        state_zero,
        command_id,
        ability_id: ability.database_id(),
    }
}

fn spawn_suicide_pair(
    world: &mut sim::World,
    database: &pipeline::database::hw1::Database,
) -> SuicidePair {
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let origin = Vec3::new(10_000.0, 0.0, 10_000.0);
    let prototype = squad_prototype_id(database, SUICIDE_GRUNT).expect("Suicide Grunt squad");
    let source = spawn_squad_at(world, database, 1, prototype, origin, Vec3::X)
        .expect("source Suicide Grunts");
    let target = spawn_squad_at(world, database, 2, prototype, origin, Vec3::NEG_X)
        .expect("target Suicide Grunts");
    let source_members = world.get_squad(source).unwrap().unit_ids.clone();
    let survivor_id = *source_members.last().expect("four-member shipped squad");
    let survivor = world.get_unit_mut(survivor_id).unwrap();
    survivor.formation_offset = Vec3::X * 20.0;
    survivor.base.position = origin + Vec3::X * 20.0;
    for target_id in world.get_squad(target).unwrap().unit_ids.clone() {
        world.get_unit_mut(target_id).unwrap().hitpoints = 100.0;
    }
    SuicidePair {
        source,
        target,
        source_members,
        survivor_id,
        initial_target_hitpoints: squad_hitpoints(world, target),
    }
}

fn issue_suicide_detonate(
    database: &pipeline::database::hw1::Database,
    world: &mut sim::World,
    source: sim::EntityId,
    target: sim::EntityId,
    command_id: u8,
) {
    CommandExecutor::with_database(database).execute(
        world,
        &CommandEntry {
            command: QueuedCommand::Work(WorkCommand::detonate_squads(
                1,
                vec![source],
                target,
                command_id,
            )),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn assert_suicide_lifecycle(
    world: &mut sim::World,
    gameplay: &sim::GameplayCatalog,
    pair: &SuicidePair,
    contract: SuicideContract,
) {
    world.update_entities_with_gameplay(0.05, gameplay);
    assert_eq!(
        world.get_squad(pair.source).unwrap().detonate_action_name(),
        Some("SuicideBomb")
    );
    world.update_entities_with_gameplay(0.05, gameplay);
    for unit_id in &pair.source_members {
        assert_eq!(
            world.get_unit(*unit_id).unwrap().tactic_state(),
            Some(contract.state_zero)
        );
    }
    world.update_entities_with_gameplay(0.05, gameplay);
    assert_eq!(
        world.get_squad(pair.source).unwrap().detonate_phase(),
        sim::SquadDetonatePhase::Attacking
    );
    assert_eq!(
        world.get_squad(pair.source).unwrap().mode,
        SquadMode::HitAndRun
    );
    world.update_entities_with_gameplay(0.05, gameplay);
    for unit_id in &pair.source_members[..pair.source_members.len() - 1] {
        assert_eq!(
            world.get_unit(*unit_id).unwrap().detonate_phase(),
            sim::UnitDetonatePhase::Working
        );
    }
    world.update_entities_with_gameplay(0.05, gameplay);
    assert!(world.get_unit(pair.survivor_id).is_some());
    assert!(
        pair.source_members[..pair.source_members.len() - 1]
            .iter()
            .all(|unit_id| world.get_unit(*unit_id).is_none())
    );
    assert!(squad_hitpoints(world, pair.target) < pair.initial_target_hitpoints);
    let source_squad = world.get_squad(pair.source).unwrap();
    assert_eq!(
        source_squad.recovery.recovery_type(),
        Some(sim::RecoveryType::Ability)
    );
    assert_eq!(
        source_squad.recovery.ability_id(),
        Some(contract.ability_id)
    );

    world.update_entities_with_gameplay(0.05, gameplay);
    let survivor = world.get_unit(pair.survivor_id).unwrap();
    assert_eq!(survivor.tactic_state(), None);
    assert!(!survivor.is_detonate_armed());
    assert_eq!(
        world.get_squad(pair.source).unwrap().detonate_phase(),
        sim::SquadDetonatePhase::Inactive
    );
    assert_eq!(
        world.get_squad(pair.source).unwrap().mode,
        SquadMode::Normal
    );
}

fn assert_suicide_upgrade_selection(
    world: &mut sim::World,
    database: &pipeline::database::hw1::Database,
    gameplay: &sim::GameplayCatalog,
    command_id: u8,
) {
    for technology in ["cov_suicideGrunt_upgrade1", "cov_suicideGrunt_upgrade2"] {
        world
            .activate_technology(1, database, technology)
            .unwrap_or_else(|error| panic!("{technology} should activate: {error:?}"));
    }
    assert_eq!(
        select_live_suicide_action(
            world,
            database,
            gameplay,
            command_id,
            Vec3::new(11_000.0, 0.0, 11_000.0),
        ),
        "SprintSuicideBomb"
    );
    world
        .activate_technology(1, database, "cov_suicideGrunt_upgrade3")
        .expect("third Suicide Grunt upgrade should activate");
    assert_eq!(
        select_live_suicide_action(
            world,
            database,
            gameplay,
            command_id,
            Vec3::new(12_000.0, 0.0, 12_000.0),
        ),
        "SuicideCorrosiveBomb"
    );
}

fn select_live_suicide_action(
    world: &mut sim::World,
    database: &pipeline::database::hw1::Database,
    gameplay: &sim::GameplayCatalog,
    command_id: u8,
    position: Vec3,
) -> String {
    let prototype = squad_prototype_id(database, SUICIDE_GRUNT).expect("Suicide Grunt squad");
    let source = spawn_squad_at(world, database, 1, prototype, position, Vec3::X)
        .expect("upgraded Suicide Grunts");
    let target = spawn_squad_at(world, database, 2, prototype, position, Vec3::NEG_X)
        .expect("upgrade-selection target");
    issue_suicide_detonate(database, world, source, target, command_id);
    world.update_entities_with_gameplay(0.05, gameplay);
    let selected = world
        .get_squad(source)
        .and_then(sim::Squad::detonate_action_name)
        .expect("upgraded Detonate action should select")
        .to_owned();
    assert!(world.kill_squad(source, true));
    assert!(world.kill_squad(target, true));
    selected
}

fn squad_hitpoints(world: &sim::World, squad_id: sim::EntityId) -> f32 {
    world
        .get_squad(squad_id)
        .map_or(0.0, |squad| unit_hitpoints(world, &squad.unit_ids))
}

fn unit_hitpoints(world: &sim::World, unit_ids: &[sim::EntityId]) -> f32 {
    unit_ids
        .iter()
        .filter_map(|unit_id| world.get_unit(*unit_id))
        .map(|unit| unit.hitpoints)
        .sum()
}

fn spawn_ghost_ram_pair(
    world: &mut sim::World,
    database: &pipeline::database::hw1::Database,
    physics_profile: &sim::GroundVehiclePhysicsProfile,
) -> GhostRamPair {
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let ghost_proto = squad_prototype_id(database, GHOST).expect("shipped Ghost squad");
    let attacker_squad = spawn_squad_at(world, database, 1, ghost_proto, Vec3::ZERO, Vec3::X)
        .expect("attacking Ghost");
    let target_squad = spawn_squad_at(world, database, 2, ghost_proto, Vec3::X, Vec3::NEG_X)
        .expect("target Ghost");
    let attacker_id = world.get_squad(attacker_squad).unwrap().unit_ids[0];
    let target_id = world.get_squad(target_squad).unwrap().unit_ids[0];
    let attacker = world.get_unit(attacker_id).unwrap();
    let body = attacker.physics.as_ref().expect("spawned Ghost body");
    assert_eq!(body.collider(), physics_profile.collider());
    assert_eq!(body.material(), physics_profile.material());
    assert_close(body.max_speed(), 40.0);
    assert_close(body.acceleration(), 100.0);
    assert_close(body.turn_rate_degrees(), 1_100.0);
    GhostRamPair {
        attacker_squad,
        target_squad,
        attacker_id,
        target_id,
        initial_target_hitpoints: world.get_unit(target_id).unwrap().hitpoints,
    }
}

fn assert_ghost_ram_impact(world: &sim::World, pair: &GhostRamPair, ram_state: sim::TacticStateId) {
    let attacker = world.get_unit(pair.attacker_id).unwrap();
    assert!(attacker.is_operational());
    assert_eq!(attacker.tactic_state(), Some(ram_state));
    assert_eq!(attacker.tactic_state_revision(), 1);
    assert!(attacker.ammunition.current() < attacker.ammunition.maximum());
    assert!(world.get_unit(pair.target_id).unwrap().hitpoints < pair.initial_target_hitpoints);
    assert_eq!(
        world.get_squad(pair.attacker_squad).unwrap().mode,
        SquadMode::Normal
    );
}

fn issue_ghost_ram(
    database: &pipeline::database::hw1::Database,
    gameplay: &sim::GameplayCatalog,
    world: &mut sim::World,
    attacker: sim::EntityId,
    target: sim::EntityId,
) {
    let command_id = database_ability_id(database, "Command");
    let ability = gameplay
        .resolve_order_ability(GHOST, command_id)
        .expect("Command should resolve through Ghost AbilityCommand");
    assert_eq!(ability.name(), "CovGhostRam");
    assert_eq!(ability.squad_mode(), Some(SquadMode::HitAndRun));
    let mut command = WorkCommand::attack_squads(1, vec![attacker], target);
    command.ability_id = i32::from(command_id);
    CommandExecutor::with_database(database).execute(
        world,
        &CommandEntry {
            command: QueuedCommand::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
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

fn optional(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0),
        "expected {expected}, got {actual}"
    );
}
