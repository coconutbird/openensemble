use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::objects::ChildObjectType;
use sim::{
    ConstructionKind, EntityId, PlayerId, World, load_scenario_from_game_dir, object_prototype_id,
    object_runtime_id, spawn_object_at,
};

const COMMAND_CENTER: &str = "unsc_bldg_command_01";
const REACTOR: &str = "unsc_bldg_reactor_01";
const ADVANCED_REACTOR: &str = "unsc_bldg_reactor_02";
const SUPPLY_PAD: &str = "unsc_bldg_supplypad_01";
const COVENANT_TEMPLE: &str = "cov_bldg_temple_01";
const COVENANT_BUILDER: &str = "cov_bldg_builder_01";
const REBEL_BASE: &str = "creep_rebel_base_01";
const HOT_PICKUP: &str = "cov_obj_hotpickup_01";
const ARBITER_SQUAD: &str = "cov_inf_arbiter_01";
const ARBITER_LEADER_TECH: &str = "covenant_LeaderArbiter";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-construction-loading -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn builds_real_command_center_through_blood_gulch_power_socket() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    assert_command_center_child_definitions(&loaded.content.database);
    let (&player_id, &base_id) = loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base");
    let builder_id = loaded
        .simulation
        .world
        .get_base(base_id)
        .expect("initial base")
        .anchor_building_id;
    assert_eq!(
        loaded
            .simulation
            .world
            .get_building(builder_id)
            .unwrap()
            .proto_object_name,
        "game_base_Socket_01",
    );
    loaded
        .simulation
        .world
        .get_player_mut(player_id)
        .unwrap()
        .resources
        .amounts = [100_000.0; 4];
    let target_id = object_runtime_id(&loaded.content.database, COMMAND_CENTER)
        .expect("real database should expose the command-center runtime ID");
    let bases_before = loaded.simulation.world.bases().count();

    loaded
        .simulation
        .world
        .queue_build_other(player_id, builder_id, &loaded.content.database, target_id)
        .expect("the authored PowerSocketBase should accept its command center");
    let promoted = loaded
        .simulation
        .world
        .update_production(0.05, &loaded.content.database);
    assert_eq!(promoted.completed_construction, 0);
    let building_id = loaded
        .simulation
        .world
        .units
        .iter()
        .find_map(|(id, unit)| {
            (unit.built_by == Some(builder_id)
                && unit.proto_object_name.eq_ignore_ascii_case(COMMAND_CENTER))
            .then_some(id)
        })
        .expect("promotion should create the unfinished command center");
    let building = loaded.simulation.world.get_building(building_id).unwrap();
    assert!(!building.built);
    assert!(building.associated_sockets().is_empty());
    assert_eq!(
        loaded
            .simulation
            .world
            .unit_rally_point(building_id, player_id),
        None,
    );
    assert_eq!(building.base_id, Some(base_id));
    assert_eq!(loaded.simulation.world.bases().count(), bases_before);
    let progress = loaded
        .simulation
        .world
        .construction_progress(
            player_id,
            builder_id,
            ConstructionKind::BuildOther,
            target_id,
        )
        .unwrap()
        .expect("socket worker should expose child progress");
    assert_eq!(progress.building_id, Some(building_id));
    assert!((progress.total_points - 30.0).abs() < f32::EPSILON);

    let completed = loaded
        .simulation
        .world
        .update_production(30.0, &loaded.content.database);
    assert_eq!(completed.completed_construction, 1);
    assert_completed_command_center_children(&loaded.simulation.world, building_id, player_id);
    assert_command_center_post_completion(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        base_id,
        builder_id,
        building_id,
    );
}

fn assert_command_center_post_completion(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    base_id: sim::BaseId,
    builder_id: EntityId,
    building_id: EntityId,
) {
    let _released = world.update_production(0.05, database);
    assert!(world.get_building(builder_id).unwrap().production.is_idle());
    assert_real_base_child_damage_protection(world, database, player_id, base_id, building_id);
}

fn assert_real_base_child_damage_protection(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    base_id: sim::BaseId,
    command_center_id: EntityId,
) {
    let command_center = database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(COMMAND_CENTER))
        .unwrap();
    let supply_pad = database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(SUPPLY_PAD))
        .unwrap();
    assert_eq!(command_center.child_object_damage_taken_scalar, Some(1.0));
    assert!(
        supply_pad
            .flags
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("ChildForDamageTakenScalar"))
    );
    assert_close(
        world
            .get_building(command_center_id)
            .unwrap()
            .child_object_damage_taken_multiplier(),
        1.0,
    );
    let socket_id = world
        .get_building(command_center_id)
        .unwrap()
        .associated_sockets()[0];
    let supply_pad_runtime_id = object_runtime_id(database, SUPPLY_PAD).unwrap();
    world
        .queue_build_other(player_id, socket_id, database, supply_pad_runtime_id)
        .expect("retail base socket should accept its supply-pad command");
    let _promoted = world.update_production(0.05, database);
    let supply_pad_id = world
        .units
        .iter()
        .find_map(|(id, unit)| {
            (unit.built_by == Some(socket_id)
                && unit.proto_object_name.eq_ignore_ascii_case(SUPPLY_PAD))
            .then_some(id)
        })
        .expect("socket should promote the unfinished retail supply pad");
    assert_eq!(
        world.get_building(supply_pad_id).unwrap().base_id,
        Some(base_id)
    );

    let completed = world.update_production(1_000.0, database);
    assert_eq!(completed.completed_construction, 1);
    assert_close(
        world
            .get_building(command_center_id)
            .unwrap()
            .child_object_damage_taken_multiplier(),
        0.5,
    );
    assert!(world.kill_unit(supply_pad_id, false));
    assert_close(
        world
            .get_building(command_center_id)
            .unwrap()
            .child_object_damage_taken_multiplier(),
        1.0,
    );
}

fn assert_command_center_child_definitions(database: &Database) {
    let command_center = database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(COMMAND_CENTER))
        .expect("layered database should contain the command center");
    let children = command_center
        .child_objects
        .as_ref()
        .expect("installed command center should retain its child-object list");
    assert_eq!(
        children
            .objects
            .iter()
            .filter(|child| child.child_type == Some(ChildObjectType::Socket))
            .count(),
        3,
    );
    assert!(
        children
            .objects
            .iter()
            .any(|child| child.child_type == Some(ChildObjectType::Rally))
    );
    assert!(children.objects.iter().any(|child| {
        child.child_type == Some(ChildObjectType::Foundation)
            && child
                .proto_object
                .eq_ignore_ascii_case("unsc_bldg_command_foundation_01")
    }));
}

fn assert_completed_command_center_children(
    world: &World,
    building_id: EntityId,
    player_id: PlayerId,
) {
    let building = world.get_building(building_id).unwrap();
    assert!(building.built);
    assert_eq!(building.associated_sockets().len(), 3);
    assert!(
        building
            .associated_sockets()
            .iter()
            .all(|&socket_id| world.get_unit(socket_id).is_some_and(|socket| socket.built))
    );
    assert_eq!(building.associated_foundations().len(), 1);
    let foundation = world
        .get_building(building.associated_foundations()[0])
        .expect("completed command center should create its foundation");
    assert!(foundation.built);
    assert!(
        foundation
            .proto_object_name
            .eq_ignore_ascii_case("unsc_bldg_command_foundation_01")
    );
    assert!(foundation.base.position.distance(building.base.position) < 1.0);
    let rally_holder_id = building.associated_parking_lot().unwrap_or(building_id);
    assert!(world.unit_rally_point(rally_holder_id, player_id).is_some());
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn real_rebel_base_materializes_and_kills_its_authored_units() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base")
        .0;
    let prototype_id = object_prototype_id(&loaded.content.database, REBEL_BASE)
        .expect("layered database should expose the rebel base");
    let position = Vec3::new(100.0, 0.0, 200.0);
    let base_id = spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        prototype_id,
        position,
        Vec3::X,
    )
    .expect("retail rebel base should be spawnable");
    let child_ids = loaded
        .simulation
        .world
        .get_building(base_id)
        .unwrap()
        .associated_child_units()
        .to_vec();

    assert_eq!(child_ids.len(), 4);
    assert_eq!(
        child_ids
            .iter()
            .filter(|&&child_id| loaded
                .simulation
                .world
                .get_unit(child_id)
                .is_some_and(|child| child
                    .proto_object_name
                    .eq_ignore_ascii_case("creep_rebel_turret_01")))
            .count(),
        2,
    );
    assert!(child_ids.iter().all(|&child_id| {
        loaded
            .simulation
            .world
            .get_unit(child_id)
            .is_some_and(|child| child.built && child.built_by == Some(base_id))
    }));
    let child_position = loaded
        .simulation
        .world
        .get_unit(child_ids[0])
        .unwrap()
        .base
        .position;
    assert!((child_position.x - 124.0).abs() < 1.0e-3);
    assert!((child_position.z - 157.0).abs() < 1.0e-3);

    assert!(loaded.simulation.world.kill_unit(base_id, false));
    assert!(child_ids.iter().all(|&child_id| {
        loaded
            .simulation
            .world
            .get_unit(child_id)
            .is_some_and(|child| !child.base.is_alive())
    }));
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn real_covenant_base_materializes_its_default_building_child() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base")
        .0;
    let prototype_id = object_prototype_id(&loaded.content.database, COVENANT_BUILDER)
        .expect("layered database should expose the Covenant builder");
    let position = Vec3::new(100.0, 0.0, 200.0);
    let base_id = spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        prototype_id,
        position,
        Vec3::X,
    )
    .expect("retail Covenant builder should be spawnable");
    let child_ids = loaded
        .simulation
        .world
        .get_building(base_id)
        .unwrap()
        .associated_child_buildings()
        .to_vec();

    assert_eq!(child_ids.len(), 1);
    let child = loaded
        .simulation
        .world
        .get_building(child_ids[0])
        .expect("default child type should preserve the prototype's building class");
    assert!(child.proto_object_name.eq_ignore_ascii_case(HOT_PICKUP));
    assert_eq!(child.built_by, Some(base_id));
    assert!((child.base.position.x - 85.0).abs() < 1.0e-3);
    assert!((child.base.position.z - 115.0).abs() < 1.0e-3);

    assert!(loaded.simulation.world.kill_unit(base_id, false));
    assert!(
        loaded
            .simulation
            .world
            .get_unit(child_ids[0])
            .is_some_and(|child| !child.base.is_alive())
    );
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn real_reactor_quote_uses_the_scenario_layered_object_database() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base")
        .0;
    let runtime_id = object_runtime_id(&loaded.content.database, REACTOR)
        .expect("layered database should expose the retail reactor runtime ID");
    let advanced_prototype_id = object_prototype_id(&loaded.content.database, ADVANCED_REACTOR)
        .expect("layered database should expose the retail advanced-reactor prototype ID");
    let supplies_id = loaded
        .content
        .database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .and_then(|resources| {
            resources
                .entries
                .iter()
                .position(|resource| resource.name.eq_ignore_ascii_case("Supplies"))
        })
        .expect("retail database should define Supplies");
    let power_id = loaded
        .content
        .database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .and_then(|resources| {
            resources
                .entries
                .iter()
                .position(|resource| resource.name.eq_ignore_ascii_case("Power"))
        })
        .expect("retail database should define Power");
    let power_before = loaded
        .simulation
        .world
        .get_player(player_id)
        .unwrap()
        .get_resource(power_id);
    let before = loaded
        .simulation
        .world
        .object_cost(&loaded.content.database, player_id, runtime_id)
        .expect("retail reactor should have a valid quote");

    let advanced_reactor_id = spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        advanced_prototype_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .expect("retail advanced reactor should be spawnable");
    assert_close(
        loaded
            .simulation
            .world
            .get_player(player_id)
            .unwrap()
            .get_resource(power_id),
        power_before + 2.0,
    );
    let after = loaded
        .simulation
        .world
        .object_cost(&loaded.content.database, player_id, runtime_id)
        .expect("retail reactor should still have a valid quote");

    let escalation = after.get(supplies_id) - before.get(supplies_id);
    assert!((escalation - 250.0).abs() <= f32::EPSILON);
    loaded
        .simulation
        .world
        .remove_unit(advanced_reactor_id)
        .expect("spawned advanced reactor should remain live");
    assert_close(
        loaded
            .simulation
            .world
            .get_player(player_id)
            .unwrap()
            .get_resource(power_id),
        power_before,
    );
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn real_supply_pad_built_state_activates_and_removes_its_rate() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base")
        .0;
    let supply_pad_id = object_prototype_id(&loaded.content.database, SUPPLY_PAD)
        .expect("layered database should expose the retail supply pad");
    let supplies_rate_id = loaded
        .content
        .database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.rates.as_ref())
        .and_then(|rates| {
            rates
                .entries
                .iter()
                .position(|rate| rate.eq_ignore_ascii_case("Supplies"))
        })
        .expect("retail database should define the Supplies rate");
    let rate_before = loaded
        .simulation
        .world
        .get_player(player_id)
        .unwrap()
        .get_rate(supplies_rate_id);

    let building_id = spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        supply_pad_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .expect("retail supply pad should be spawnable");
    assert_close(
        loaded
            .simulation
            .world
            .get_player(player_id)
            .unwrap()
            .get_rate(supplies_rate_id),
        rate_before + 5.0,
    );

    loaded
        .simulation
        .world
        .remove_unit(building_id)
        .expect("spawned supply pad should remain live");
    assert_close(
        loaded
            .simulation
            .world
            .get_player(player_id)
            .unwrap()
            .get_rate(supplies_rate_id),
        rate_before,
    );
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn real_temple_one_time_child_trains_only_one_enabled_leader() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base")
        .0;
    assert!(
        loaded
            .simulation
            .world
            .activate_technology(player_id, &loaded.content.database, ARBITER_LEADER_TECH)
            .expect("retail Arbiter leader technology should activate")
    );
    let temple_id = object_prototype_id(&loaded.content.database, COVENANT_TEMPLE)
        .expect("layered database should expose the Covenant temple");
    let leaders_before = count_squads(&loaded.simulation.world, ARBITER_SQUAD);

    for position in [Vec3::ZERO, Vec3::X * 50.0] {
        spawn_object_at(
            &mut loaded.simulation.world,
            &loaded.content.database,
            player_id,
            temple_id,
            position,
            Vec3::Z,
        )
        .expect("retail Covenant temple should be spawnable");
    }

    assert_eq!(
        count_squads(&loaded.simulation.world, ARBITER_SQUAD),
        leaders_before + 1,
    );
}

fn count_squads(world: &World, prototype: &str) -> usize {
    world
        .squads
        .iter()
        .filter(|(_, squad)| squad.proto_squad_name.eq_ignore_ascii_case(prototype))
        .count()
}

fn assert_close(left: f32, right: f32) {
    assert!((left - right).abs() <= f32::EPSILON, "{left} != {right}");
}
