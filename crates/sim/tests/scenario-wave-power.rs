use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    EntityId, NativePowerInput, PowerUserId, WaveGravityBallState, WavePowerInvocation,
    load_scenario_from_game_dir, power_prototype_id, spawn_squad_at, squad_prototype_id,
};

const WAVE_POWER: &str = "CovLeaderWave";
const BRUTE_CHIEFTAIN: &str = "cov_inf_bruteChief_01";
const MARINES: &str = "unsc_inf_marine_01";
const WARTHOG: &str = "unsc_veh_warthog_01";
const BALL: &str = "pow_gp_wave_01";
const LIGHTNING: &str = "pow_proj_wave_lightning_01";
const LIGHTNING_BEAM: &str = "pow_proj_wave_lightning_beam_01";
const DEBRIS: &str = "pow_proj_wave_debris_01";
const EXPLOSION: &str = "pow_proj_wave_explode_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-wave-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_damage_part_catalog_uses_layered_visual_damage_and_model_assets() {
    let loaded = load_installed_scenario();
    let marine = loaded
        .simulation
        .gameplay
        .damage_parts(MARINES)
        .expect("shipped Marine damage template");
    assert_eq!(marine.impact_point_count(), 0);

    let warthog = loaded
        .simulation
        .gameplay
        .damage_parts(WARTHOG)
        .expect("shipped Warthog damage template");
    assert_eq!(warthog.impact_point_count(), 5);
    let parts = (0..warthog.impact_point_count())
        .filter_map(|index| warthog.impact_point(index))
        .collect::<Vec<_>>();
    assert!(!parts.is_empty());
    for part in parts {
        assert!(!part.mesh_names().is_empty());
        assert!(part.collider().half_extents.cmpgt(Vec3::ZERO).all());
        assert!(part.material().mass > 0.0);
        assert!(part.force_multiplier().is_finite());
    }
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_wave_profile_uses_scenario_layered_database_and_tactics() {
    let mut loaded = load_installed_scenario();
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_enemies(&mut loaded.simulation.world);
    let center = playable_center(&loaded.simulation.world);
    let owner_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        BRUTE_CHIEFTAIN,
        center - Vec3::X * 20.0,
    );
    let target_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        2,
        MARINES,
        center,
    );
    let target_position = loaded
        .simulation
        .world
        .get_squad(target_id)
        .unwrap()
        .base
        .position;
    let target_units = loaded
        .simulation
        .world
        .get_squad(target_id)
        .unwrap()
        .unit_ids
        .clone();
    let health_before = total_health(&loaded.simulation.world, &target_units);
    let power_id =
        power_prototype_id(&loaded.content.database, WAVE_POWER).expect("shipped Wave power");
    let execution_id = loaded
        .simulation
        .world
        .invoke_wave_power(
            &loaded.content.database,
            invocation(power_id, owner_id, target_position),
        )
        .expect("shipped Wave profile should fully resolve");
    assert_shipped_profile(&loaded, execution_id);

    let ball_id = loaded.simulation.world.active_wave_powers()[0].ball_object_id();
    assert_eq!(ball_id.class(), Some(sim::EntityClass::Unit));
    assert_proto(&loaded.simulation.world, ball_id, BALL);
    assert_eq!(
        loaded.simulation.world.get_squad(owner_id).unwrap().mode,
        sim::SquadMode::Power
    );
    assert!(loaded.simulation.world.submit_wave_power_input(
        &loaded.content.database,
        execution_id,
        NativePowerInput::Position(target_position + Vec3::X * 8.0),
    ));

    advance(&mut loaded, 1);
    let execution = &loaded.simulation.world.active_wave_powers()[0];
    assert_close(
        loaded
            .simulation
            .world
            .get_unit(ball_id)
            .unwrap()
            .base
            .position
            .x,
        target_position.x + 1.5,
    );
    assert_close(execution.current_explosion_damage_bank(), 150.0);
    assert!(world_has_prototype(
        &loaded.simulation.world,
        LIGHTNING_BEAM
    ));
    let health_after_lightning = total_health(&loaded.simulation.world, &target_units);
    assert!(health_after_lightning < health_before);
    assert!(loaded.simulation.world.submit_wave_power_input(
        &loaded.content.database,
        execution_id,
        NativePowerInput::Confirm(target_position),
    ));
    advance(&mut loaded, 1);
    let execution = &loaded.simulation.world.active_wave_powers()[0];
    assert_eq!(execution.state(), WaveGravityBallState::Exploding);
    assert!(total_health(&loaded.simulation.world, &target_units) < health_after_lightning);

    advance(&mut loaded, 16);
    assert!(loaded.simulation.world.active_wave_powers().is_empty());
    assert!(loaded.simulation.world.get_unit(ball_id).is_none());
    assert_eq!(
        loaded.simulation.world.get_squad(owner_id).unwrap().mode,
        sim::SquadMode::Normal
    );
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_wave_captures_a_marine_physics_replacement_as_owned_debris() {
    let mut loaded = load_installed_scenario();
    let setup = begin_marine_wave_capture(&mut loaded);
    let captured = capture_marine_replacement(&mut loaded, setup.target_unit_id);

    assert!(loaded.simulation.world.submit_wave_power_input(
        &loaded.content.database,
        setup.execution_id,
        NativePowerInput::Confirm(setup.target_position),
    ));
    advance(&mut loaded, 1);
    assert!(loaded.simulation.world.get_unit(captured.unit_id).is_none());
    assert!(
        loaded.simulation.world.active_wave_powers()[0]
            .captured_objects()
            .is_empty()
    );
    assert_launched_marine_debris(&loaded, captured);
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_wave_throws_live_marines_with_layered_action_and_clamshell_data() {
    let mut loaded = load_installed_scenario();
    let setup = begin_shipped_marine_throw(&mut loaded);
    advance(&mut loaded, 70);
    let execution = &loaded.simulation.world.active_wave_powers()[0];
    assert_eq!(execution.captured_objects().len(), 4);
    let captured_count = u16::try_from(execution.captured_objects().len()).unwrap();
    let captured_maximum = f32::from(captured_count) * 1_500.0;
    let throw_scalar = execution
        .current_explosion_damage_bank()
        .clamp(0.0, captured_maximum)
        / execution.maximum_possible_explosion_damage_bank();
    assert!(throw_scalar > 0.15);
    let ball_id = execution.ball_object_id();
    let ball = loaded
        .simulation
        .world
        .get_unit(ball_id)
        .unwrap()
        .base
        .position;
    let marine = loaded
        .simulation
        .world
        .get_unit(setup.target_unit_id)
        .unwrap();
    assert!(marine.physics.is_none());
    let planar = Vec3::new(
        marine.base.position.x - ball.x,
        0.0,
        marine.base.position.z - ball.z,
    )
    .normalize();
    let expected_speed = 25.0 * throw_scalar;

    assert!(loaded.simulation.world.submit_wave_power_input(
        &loaded.content.database,
        setup.execution_id,
        NativePowerInput::Confirm(ball),
    ));
    advance(&mut loaded, 1);
    let marine = loaded
        .simulation
        .world
        .get_unit(setup.target_unit_id)
        .unwrap();
    assert!(marine.is_thrown());
    assert_eq!(marine.thrown_by(), Some(ball_id));
    assert!(marine.physics.as_ref().unwrap().collider().half_extents.y > 1.5);
    let expected_horizontal = expected_speed * 30.0_f32.to_radians().cos();
    let horizontal = Vec3::new(marine.base.velocity.x, 0.0, marine.base.velocity.z);
    assert!(horizontal.normalize().abs_diff_eq(planar, 0.000_1));
    assert!(horizontal.length() <= expected_horizontal);
    assert!(horizontal.length() > expected_horizontal * 0.98);
    assert!(marine.base.velocity.y > 0.0);
    assert!(marine.base.velocity.y < expected_speed * 30.0_f32.to_radians().sin());
}

#[derive(Debug, Clone, Copy)]
struct ShippedMarineThrowSetup {
    execution_id: sim::PowerExecutionId,
    target_unit_id: EntityId,
}

fn begin_shipped_marine_throw(loaded: &mut sim::LoadedGameScenario) -> ShippedMarineThrowSetup {
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_enemies(&mut loaded.simulation.world);
    let center = playable_center(&loaded.simulation.world);
    let owner_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        BRUTE_CHIEFTAIN,
        center - Vec3::X * 20.0,
    );
    let capture_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        2,
        MARINES,
        center + Vec3::Z * 3.0,
    );
    let bank_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        2,
        MARINES,
        center - Vec3::X * 10.0,
    );
    let target_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        MARINES,
        center + Vec3::X * 10.0,
    );
    let captured_units = loaded
        .simulation
        .world
        .get_squad(capture_id)
        .unwrap()
        .unit_ids
        .clone();
    assert_eq!(captured_units.len(), 4);
    for unit_id in captured_units {
        loaded
            .simulation
            .world
            .get_unit_mut(unit_id)
            .unwrap()
            .kill();
    }
    for squad_id in [bank_id, target_id] {
        for unit_id in loaded
            .simulation
            .world
            .get_squad(squad_id)
            .unwrap()
            .unit_ids
            .clone()
        {
            let unit = loaded.simulation.world.get_unit_mut(unit_id).unwrap();
            unit.set_max_hitpoints(100_000.0);
            unit.hitpoints = 100_000.0;
        }
    }
    let target_unit_id = loaded
        .simulation
        .world
        .get_squad(target_id)
        .unwrap()
        .unit_ids[0];
    let power_id =
        power_prototype_id(&loaded.content.database, WAVE_POWER).expect("shipped Wave power");
    let execution_id = loaded
        .simulation
        .world
        .invoke_wave_power(
            &loaded.content.database,
            invocation(power_id, owner_id, center),
        )
        .expect("shipped Wave execution");
    ShippedMarineThrowSetup {
        execution_id,
        target_unit_id,
    }
}

#[derive(Debug, Clone, Copy)]
struct MarineWaveSetup {
    execution_id: sim::PowerExecutionId,
    target_position: Vec3,
    target_unit_id: EntityId,
}

#[derive(Debug, Clone, Copy)]
struct CapturedMarineSnapshot {
    unit_id: EntityId,
    pickup_id: EntityId,
    variation: Option<usize>,
    center: Vec3,
    visual_center_offset: Vec3,
    velocity: Vec3,
    forward: Vec3,
    radial: Vec3,
}

fn begin_marine_wave_capture(loaded: &mut sim::LoadedGameScenario) -> MarineWaveSetup {
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_enemies(&mut loaded.simulation.world);
    let center = playable_center(&loaded.simulation.world);
    let owner_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        BRUTE_CHIEFTAIN,
        center - Vec3::X * 20.0,
    );
    let target_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        2,
        MARINES,
        center,
    );
    let target_position = loaded
        .simulation
        .world
        .get_squad(target_id)
        .unwrap()
        .base
        .position;
    let target_unit_id = loaded
        .simulation
        .world
        .get_squad(target_id)
        .unwrap()
        .unit_ids[0];
    assert_marine_replacement_catalog(loaded, target_unit_id);
    loaded
        .simulation
        .world
        .get_unit_mut(target_unit_id)
        .unwrap()
        .kill();
    let power_id =
        power_prototype_id(&loaded.content.database, WAVE_POWER).expect("shipped Wave power");
    let execution_id = loaded
        .simulation
        .world
        .invoke_wave_power(
            &loaded.content.database,
            invocation(power_id, owner_id, target_position),
        )
        .expect("shipped Wave execution");
    MarineWaveSetup {
        execution_id,
        target_position,
        target_unit_id,
    }
}

fn capture_marine_replacement(
    loaded: &mut sim::LoadedGameScenario,
    target_unit_id: EntityId,
) -> CapturedMarineSnapshot {
    advance(loaded, 1);
    let replacement_count = loaded
        .simulation
        .world
        .units
        .iter()
        .filter(|(_, unit)| unit.is_physics_replacement())
        .count();
    assert_eq!(
        replacement_count, 1,
        "dead Marine should become one physics replacement before pickup"
    );
    advance(loaded, 3);
    let execution = &loaded.simulation.world.active_wave_powers()[0];
    let [captured] = execution.captured_objects() else {
        panic!("dead Marine should be one captured physics replacement");
    };
    let captured_unit_id = captured.unit_id();
    let pickup_id = captured.pickup_attachment_id();
    assert_ne!(captured_unit_id, target_unit_id);
    assert!(loaded.simulation.world.get_unit(target_unit_id).is_none());
    let captured_unit = loaded.simulation.world.get_unit(captured_unit_id).unwrap();
    assert!(captured_unit.is_physics_replacement());
    assert_eq!(captured_unit.base.player_id, 1);
    assert_eq!(captured_unit.obstruction_half_extents, Vec3::ZERO);
    assert_close(
        captured_unit
            .physics
            .as_ref()
            .unwrap()
            .material()
            .angular_damping,
        0.8,
    );
    assert_close(
        captured_unit
            .physics
            .as_ref()
            .unwrap()
            .material()
            .linear_damping,
        0.1,
    );
    assert_proto_object(&loaded.simulation.world, pickup_id, "fx_pow_wave_pickup_01");
    let captured_variation = captured_unit.object_state.visual_variation_index();
    let center_offset = captured_unit
        .physics
        .as_ref()
        .unwrap()
        .collider()
        .center_offset;
    let captured_center = captured_unit.base.position + center_offset;
    let pickup = loaded.simulation.world.get_object(pickup_id).unwrap();
    assert!(pickup.base.position.abs_diff_eq(captured_center, 0.000_1));
    let local_offset = pickup.object_state.attachment_local_offset();
    let forward = Vec3::new(
        captured_unit.base.forward.x,
        0.0,
        captured_unit.base.forward.z,
    )
    .normalize_or(Vec3::Z);
    let right = Vec3::Y.cross(forward);
    let reconstructed_offset =
        right * local_offset.x + Vec3::Y * local_offset.y + forward * local_offset.z;
    assert!(reconstructed_offset.abs_diff_eq(center_offset, 0.000_1));
    let captured_velocity = captured_unit.base.velocity;
    let captured_forward = captured_unit.base.forward;
    let ball_id = loaded.simulation.world.active_wave_powers()[0].ball_object_id();
    let ball = loaded
        .simulation
        .world
        .get_unit(ball_id)
        .unwrap()
        .base
        .position;
    let radial = (captured_center - ball).normalize_or(Vec3::X);
    CapturedMarineSnapshot {
        unit_id: captured_unit_id,
        pickup_id,
        variation: captured_variation,
        center: captured_center,
        visual_center_offset: -center_offset,
        velocity: captured_velocity,
        forward: captured_forward,
        radial,
    }
}

fn assert_launched_marine_debris(
    loaded: &sim::LoadedGameScenario,
    captured: CapturedMarineSnapshot,
) {
    let (debris_id, debris) = loaded
        .simulation
        .world
        .projectiles
        .iter()
        .find(|(_, projectile)| projectile.proto_object_name.eq_ignore_ascii_case(DEBRIS))
        .expect("captured Marine should launch as Wave debris");
    assert_eq!(debris.visual_proto_object_name(), MARINES);
    assert_eq!(debris.base.player_id, 2);
    assert_eq!(debris.created_by_player_id(), 1);
    assert_eq!(debris.visual_center_offset(), captured.visual_center_offset);
    assert_eq!(
        debris.object_state.visual_variation_index(),
        captured.variation
    );
    assert!(
        debris
            .initial_position()
            .abs_diff_eq(captured.center, 0.000_1)
    );
    let inherited_velocity = captured.velocity + captured.radial * 15.0;
    assert_close(debris.base.velocity.x, inherited_velocity.x);
    assert_close(debris.base.velocity.z, inherited_velocity.z);
    assert!(debris.base.velocity.y < inherited_velocity.y);
    assert!(debris.base.forward.abs_diff_eq(captured.forward, 0.000_1));
    assert!(debris.affected_by_gravity);
    assert_close(
        debris.gravity,
        loaded.simulation.gameplay.projectile_gravity(),
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(captured.pickup_id)
            .unwrap()
            .object_state
            .attached_to(),
        Some(debris_id)
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(captured.pickup_id)
            .unwrap()
            .object_state
            .attachment_local_offset(),
        Vec3::ZERO
    );
}

fn assert_marine_replacement_catalog(loaded: &sim::LoadedGameScenario, unit_id: EntityId) {
    let Some(replacement) = loaded.simulation.gameplay.physics_replacement(MARINES) else {
        panic!(
            "scenario-layered marine PhysicsReplacementInfo; issues: {:?}",
            loaded.simulation.gameplay.physics_replacement_issues()
        );
    };
    assert!(
        replacement.collider().half_extents.y > 1.5,
        "Marine replacement should merge upper, lower, and pelvis bounds"
    );
    assert!(replacement.is_clamshell());
    assert!(
        loaded
            .simulation
            .gameplay
            .physics_replacement_issues()
            .is_empty(),
        "shipped replacement chains should resolve: {:#?}",
        loaded.simulation.gameplay.physics_replacement_issues()
    );
    assert!(
        !loaded
            .simulation
            .world
            .get_unit(unit_id)
            .unwrap()
            .is_invulnerable(),
        "spawned Marine must be pullable"
    );
}

fn assert_shipped_profile(loaded: &sim::LoadedGameScenario, execution_id: sim::PowerExecutionId) {
    let execution = loaded
        .simulation
        .world
        .active_wave_powers()
        .iter()
        .find(|execution| execution.id() == execution_id)
        .unwrap();
    assert_eq!(execution.ball_prototype(), BALL);
    assert_eq!(execution.lightning_projectile_prototype(), LIGHTNING);
    assert_eq!(execution.lightning_beam_prototype(), Some(LIGHTNING_BEAM));
    assert_eq!(execution.debris_projectile_prototype(), DEBRIS);
    assert_eq!(execution.explode_projectile_prototype(), EXPLOSION);
    assert_close(execution.tick_length(), 0.2);
    assert_close(execution.supplies_per_tick(), 10.0);
    assert_close(execution.pulling_range(), 17.0);
    assert_close(execution.maximum_ball_speed_pulling(), 15.0);
    assert_eq!(execution.command_interval_ms(), 200);
    assert_close(execution.minimum_ball_distance(), 0.0);
    assert_close(execution.maximum_ball_distance(), 55.0);
    assert_eq!(execution.maximum_captured_objects(), 20);
    assert_tactic(loaded, LIGHTNING, "Lightning", 210.0, 5.0);
    assert_tactic(loaded, DEBRIS, "ThrownDebris", 325.0, 3.0);
    assert_tactic(loaded, EXPLOSION, "Wave", 750.0, 20.0);
}

fn assert_tactic(
    loaded: &sim::LoadedGameScenario,
    prototype: &str,
    weapon_name: &str,
    damage: f32,
    radius: f32,
) {
    let tactics = loaded
        .simulation
        .gameplay
        .object(prototype)
        .unwrap_or_else(|| panic!("scenario-layered tactics for {prototype}"))
        .tactics();
    let weapon = tactics
        .weapons
        .iter()
        .find(|weapon| weapon.name.eq_ignore_ascii_case(weapon_name))
        .unwrap_or_else(|| panic!("{weapon_name} tactic weapon"));
    assert_eq!(weapon.damage_per_second, Some(damage));
    assert_eq!(weapon.aoe_radius, Some(radius));
    if prototype.eq_ignore_ascii_case(EXPLOSION) {
        assert_eq!(weapon.throw_offset_angle, None);
        assert_eq!(weapon.throw_velocity, Some(25.0));
    }
}

fn advance(loaded: &mut sim::LoadedGameScenario, ticks: usize) {
    for _ in 0..ticks {
        loaded.simulation.world.game_time_ms =
            loaded.simulation.world.game_time_ms.wrapping_add(100);
        loaded
            .simulation
            .world
            .update_entities_with_database_and_gameplay(
                0.1,
                &loaded.content.database,
                &loaded.simulation.gameplay,
            );
    }
}

fn spawn_named_squad(
    world: &mut sim::World,
    database: &Database,
    player_id: u8,
    name: &str,
    mut position: Vec3,
) -> EntityId {
    let prototype_id = squad_prototype_id(database, name).expect("shipped squad prototype");
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    spawn_squad_at(world, database, player_id, prototype_id, position, Vec3::Z)
        .expect("spawn shipped squad")
}

fn total_health(world: &sim::World, unit_ids: &[EntityId]) -> f32 {
    unit_ids
        .iter()
        .filter_map(|unit_id| world.get_unit(*unit_id))
        .map(|unit| unit.hitpoints + unit.shields.current)
        .sum()
}

fn assert_proto(world: &sim::World, id: EntityId, name: &str) {
    assert_eq!(
        world
            .get_unit(id)
            .map(|unit| unit.proto_object_name.as_str()),
        Some(name)
    );
}

fn assert_proto_object(world: &sim::World, id: EntityId, name: &str) {
    assert_eq!(
        world
            .get_object(id)
            .map(|object| object.proto_object_name.as_str()),
        Some(name)
    );
}

fn world_has_prototype(world: &sim::World, name: &str) -> bool {
    world
        .objects
        .iter()
        .any(|(_, object)| object.proto_object_name.eq_ignore_ascii_case(name))
}

fn invocation(
    proto_power_id: i32,
    squad_id: EntityId,
    target_location: Vec3,
) -> WavePowerInvocation {
    WavePowerInvocation {
        player_id: 1,
        proto_power_id,
        power_level: 0,
        squad_id,
        target_location,
        ignore_requirements: true,
        power_user_id: PowerUserId::INVALID,
    }
}

fn configure_enemies(world: &mut sim::World) {
    world.get_player_mut(1).expect("player one").team_id = 1;
    world.get_player_mut(2).expect("player two").team_id = 2;
    world.configure_standard_team_relations();
}

fn playable_center(world: &sim::World) -> Vec3 {
    let bounds = world
        .effective_playable_bounds()
        .expect("scenario terrain bounds");
    let mut center = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    center.y = world
        .terrain_height(center, true)
        .expect("terrain at playable center");
    center
}

fn remove_scenario_squads(world: &mut sim::World) {
    let ids = world.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids {
        world.remove_squad(id).unwrap();
    }
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "expected {expected}, got {actual}"
    );
}
