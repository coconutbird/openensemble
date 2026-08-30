use glam::Vec3;
use sim::{Entity, JoinKind, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id};

const PROPHET: &str = "cov_inf_prophet_01";
const MONITOR: &str = "cov_air_monitor_01";
const FLOOD_BOMBER: &str = "fld_air_bomber_01";

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_spawn_actions_survive_scenario_database_layering() {
    let loaded = load_installed_scenario();
    let gameplay = &loaded.simulation.gameplay;

    let prophet = &gameplay.persistent_squad_spawns(PROPHET)[0];
    assert_eq!(prophet.action_name(), "SpawnSentinel");
    assert_eq!(prophet.squad_type(), MONITOR);
    assert_eq!(prophet.work_rate().to_bits(), 0.0_f32.to_bits());
    assert_eq!(prophet.count(), 2);
    assert!(prophet.starts_disabled());
    assert!(prophet.auto_join());
    assert!(!prophet.stationary());
    assert_eq!(prophet.animation(), None);

    let bomber = &gameplay.persistent_squad_spawns(FLOOD_BOMBER)[0];
    assert_eq!(bomber.action_name(), "SpawnSentinel");
    assert_eq!(bomber.squad_type(), "fld_egg_infectionForm_01");
    assert_eq!(bomber.work_rate().to_bits(), 10.0_f32.to_bits());
    assert_eq!(bomber.work_rate_variance().to_bits(), 4.0_f32.to_bits());
    assert_eq!(bomber.count(), 0);
    assert_eq!(bomber.animation(), Some("SpawnEgg"));
    assert!(bomber.stationary());
    assert!(bomber.hide_until_release());
    assert!(!bomber.auto_join());
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn prophet_spawn_action_maintains_two_live_auto_joined_monitors() {
    let mut loaded = load_installed_scenario();
    let database = &loaded.content.database;
    let prototype = squad_prototype_id(database, PROPHET).expect("Prophet squad prototype");
    let owner_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        1,
        prototype,
        Vec3::new(128.0, 0.0, 128.0),
        Vec3::Z,
    )
    .expect("spawn Prophet");
    let owner_unit = loaded
        .simulation
        .world
        .get_squad(owner_squad)
        .expect("Prophet squad")
        .unit_ids[0];
    loaded
        .simulation
        .world
        .get_unit_mut(owner_unit)
        .expect("Prophet unit")
        .actions
        .set_enabled("SpawnSentinel", true);

    advance(&mut loaded, 100);
    let initial = spawned_followers(&loaded, owner_unit);
    assert_eq!(initial.len(), 2);
    let join_states = initial
        .iter()
        .map(|id| {
            let squad = loaded.simulation.world.get_squad(*id).unwrap();
            (
                *id,
                squad.join_target(),
                squad.join_kind(),
                squad.base.position,
                squad.move_target,
                squad.state,
            )
        })
        .collect::<Vec<_>>();
    assert!(
        initial.iter().all(|id| {
            loaded.simulation.world.get_squad(*id).is_some_and(|squad| {
                squad.join_target() == Some(owner_squad)
                    && squad.join_kind() == Some(JoinKind::FollowAttack)
            })
        }),
        "spawned monitor join states: {join_states:?}"
    );

    assert!(loaded.simulation.world.kill_squad(initial[0], false));
    advance(&mut loaded, 100);
    let replenished = spawned_followers(&loaded, owner_unit);
    assert_eq!(replenished.len(), 2);
    assert!(replenished.iter().all(|id| {
        loaded.simulation.world.get_squad(*id).is_some_and(|squad| {
            squad.join_target() == Some(owner_squad)
                && squad.join_kind() == Some(JoinKind::FollowAttack)
        })
    }));
}

fn advance(loaded: &mut sim::LoadedGameScenario, ticks: usize) {
    for _ in 0..ticks {
        loaded.simulation.world.game_time_ms =
            loaded.simulation.world.game_time_ms.wrapping_add(50);
        loaded
            .simulation
            .world
            .update_entities_with_database_and_gameplay(
                0.05,
                &loaded.content.database,
                &loaded.simulation.gameplay,
            );
    }
}

fn spawned_followers(loaded: &sim::LoadedGameScenario, owner: sim::EntityId) -> Vec<sim::EntityId> {
    loaded
        .simulation
        .world
        .squads
        .iter()
        .filter_map(|(id, squad)| {
            (squad.is_alive() && squad.trained_by == Some(owner)).then_some(id)
        })
        .collect()
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
