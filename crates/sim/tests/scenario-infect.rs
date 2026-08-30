use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    GameplayCatalog, InfectionPhase, World, load_scenario_from_game_dir, spawn_squad_at,
    squad_prototype_id,
};

const SPORE: &str = "fld_air_sporecloud_01";
const MARINE: &str = "unsc_inf_marine_01";
const INFECTED_MARINE: &str = "fld_inf_InfectedMarine_01";
const INFECTED_MARINE_SQUAD: &str = "fld_Inf_InfectedMarineSingle_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-infect -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_database_and_tactics_drive_marine_infection() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    assert_shipped_profile(&loaded.simulation.gameplay);
    assert_shipped_mapping(&loaded.content.database);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let (source_squad, target_squad) = spawn_test_squads(world, database);
    let source_id = world.get_squad(source_squad).unwrap().unit_ids[0];
    let original_marines = world.get_squad(target_squad).unwrap().unit_ids.clone();

    tick(world, database, gameplay);
    let exposure = &world.get_unit(source_id).unwrap().infection_exposures()[0];
    assert_eq!(exposure.squad_id(), target_squad);
    assert_eq!(exposure.visual_count(), original_marines.len());

    let converted_id = wait_for_mark(world, database, gameplay, &original_marines);
    assert_eq!(world.get_unit(source_id).unwrap().infected_count(), 1);
    tick(world, database, gameplay);
    let transformed = world.get_unit(converted_id).unwrap();
    let infected_squad_id = transformed.squad_id.unwrap();
    assert_eq!(transformed.infection_phase(), InfectionPhase::Transforming);
    assert_eq!(transformed.proto_object_name, INFECTED_MARINE);
    assert_eq!(transformed.base.player_id, sim::GAIA_PLAYER);
    assert_eq!(
        world.get_squad(infected_squad_id).unwrap().proto_squad_name,
        INFECTED_MARINE_SQUAD
    );

    tick(world, database, gameplay);
    let converted = world.get_unit(converted_id).unwrap();
    assert_eq!(converted.infection_phase(), InfectionPhase::None);
    assert_eq!(converted.base.player_id, 1);
    assert_eq!(
        world.get_squad(infected_squad_id).unwrap().base.player_id,
        1
    );
}

fn assert_shipped_profile(gameplay: &GameplayCatalog) {
    let profile = gameplay
        .infect(SPORE)
        .expect("scenario-layered spore-cloud Infect action");
    assert_eq!(profile.action_name(), "InfectAction");
    assert_close(profile.work_rate(), 10.0);
    assert_close(profile.work_range(), 15.0);
    assert_eq!(profile.min_idle_duration_ms(), 7_000);
    assert_eq!(
        profile.attachment_proto_object(),
        Some("fx_sporecloud_infect")
    );
    assert!(profile.invalid_targets().is_empty());
    assert!(!profile.starts_disabled());
}

fn assert_shipped_mapping(database: &Database) {
    let entry = database
        .game_data
        .as_ref()
        .and_then(|data| data.infection_map.as_ref())
        .and_then(|map| {
            map.entries
                .iter()
                .find(|entry| entry.base.eq_ignore_ascii_case(MARINE))
        })
        .expect("scenario-layered Marine infection mapping");
    assert_eq!(entry.infected, INFECTED_MARINE);
    assert_eq!(entry.infected_squad, INFECTED_MARINE_SQUAD);
}

fn spawn_test_squads(world: &mut World, database: &Database) -> (sim::EntityId, sim::EntityId) {
    let bounds = world
        .terrain_bounds()
        .expect("authoritative terrain bounds");
    let mut source_position = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    source_position.y = world
        .terrain_height(source_position, true)
        .unwrap_or_default();
    let mut target_position = source_position + Vec3::X * 5.0;
    target_position.y = world
        .terrain_height(target_position, true)
        .unwrap_or(source_position.y);
    let source = spawn(world, database, 1, SPORE, source_position);
    let target = spawn(world, database, 2, MARINE, target_position);
    (source, target)
}

fn spawn(
    world: &mut World,
    database: &Database,
    player_id: u8,
    prototype: &str,
    position: Vec3,
) -> sim::EntityId {
    spawn_squad_at(
        world,
        database,
        player_id,
        squad_prototype_id(database, prototype).expect("installed squad prototype"),
        position,
        Vec3::Z,
    )
    .expect("scenario-layered squad should spawn")
}

fn wait_for_mark(
    world: &mut World,
    database: &Database,
    gameplay: &GameplayCatalog,
    candidates: &[sim::EntityId],
) -> sim::EntityId {
    for _ in 0..180 {
        tick(world, database, gameplay);
        if let Some(unit_id) = candidates.iter().copied().find(|unit_id| {
            world
                .get_unit(*unit_id)
                .is_some_and(|unit| unit.infection_phase() == InfectionPhase::Marked)
        }) {
            return unit_id;
        }
    }
    panic!("shipped spore cloud did not mark a Marine within nine seconds");
}

fn tick(world: &mut World, database: &Database, gameplay: &GameplayCatalog) {
    world.advance_time(50);
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= 0.000_1,
        "expected {left} to equal {right}"
    );
}
