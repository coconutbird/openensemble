use super::*;
use crate::command::Command;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{
    PowerCommand, PowerCommandType, PowerInputCommand, PowerInputCommandType, power_command_flags,
};
use crate::executor::CommandExecutor;
use crate::gameplay::{DamagePartProfile, PhysicsReplacementProfile, ThrownDamagePart};
use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
use crate::player::PowerGrant;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::{Database, GameData, Power, ProtoObject, Squad as ProtoSquad};

const POWER_ID: i32 = 0;

#[derive(Clone, Copy)]
struct DebrisSourceSnapshot {
    position: Vec3,
    visual_center_offset: Vec3,
    expected_velocity: Vec3,
    forward: Vec3,
}

#[test]
fn paid_wave_moves_ticks_lightning_banks_damage_and_explodes_on_confirmation() {
    let database = database(0.0, 2);
    let gameplay = gameplay(&database);
    let mut world = test_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "owner_squad", -Vec3::X * 20.0);
    let target_id = spawn(&mut world, &database, 2, "target_squad", Vec3::X * 5.0);
    make_physical(&mut world, target_id);
    let target_unit_id = squad_leader(&world, target_id);
    world.get_player_mut(1).unwrap().set_resource(0, 100.0);
    assert!(world.grant_player_power(1, &database, power_grant(owner_id)));
    let execution_id = world
        .invoke_wave_power(
            &database,
            invocation(owner_id, false, PowerUserId::new(1, WAVE_POWER_TYPE, 7)),
        )
        .unwrap();
    let execution = &world.active_wave_powers()[0];
    let ball_id = execution.ball_object_id();
    assert_eq!(ball_id.class(), Some(crate::EntityClass::Unit));
    assert_eq!(execution.id(), execution_id);
    assert_eq!(execution.ball_prototype(), "wave_ball");
    assert_eq!(execution.command_interval_ms(), 200);
    assert_close(execution.pulling_range(), 17.0);
    assert_eq!(world.get_squad(owner_id).unwrap().mode, SquadMode::Power);
    assert_close(world.get_player(1).unwrap().get_resource(0), 90.0);

    super::update(&mut world, 0.1, &database, Some(&gameplay));
    let execution = &world.active_wave_powers()[0];
    assert_close(execution.current_explosion_damage_bank(), 150.0);
    assert_eq!(execution.active_projectile_ids().len(), 1);
    assert_close(world.get_player(1).unwrap().get_resource(0), 80.0);
    assert_close(world.get_unit(ball_id).unwrap().base.position.y, 12.5);
    let health_before = world.get_unit(target_unit_id).unwrap().hitpoints;

    assert!(world.submit_wave_power_input(
        &database,
        execution_id,
        NativePowerInput::Position(Vec3::X * 8.0),
    ));
    world.update_entities_with_database_and_gameplay(0.1, &database, &gameplay);
    assert_close(world.get_unit(ball_id).unwrap().base.position.x, 1.5);
    assert!(world.get_unit(target_unit_id).unwrap().hitpoints < health_before);
    assert_close(world.get_player(1).unwrap().get_resource(0), 80.0);

    assert!(world.submit_wave_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
    ));
    super::update(&mut world, 0.01, &database, Some(&gameplay));
    let execution = &world.active_wave_powers()[0];
    assert_eq!(execution.state(), WaveGravityBallState::Exploding);
    assert_close(execution.explode_cooldown_left(), 1.5);
    assert!(world.projectiles.iter().any(|(_, projectile)| {
        projectile
            .proto_object_name
            .eq_ignore_ascii_case("wave_explode")
            && (projectile.damage - 750.0).abs() < f32::EPSILON
    }));
    assert!(world.get_unit(ball_id).is_some());

    super::update(&mut world, 1.5, &database, Some(&gameplay));
    assert!(world.active_wave_powers().is_empty());
    assert!(world.get_unit(ball_id).is_none());
    assert_eq!(world.get_squad(owner_id).unwrap().mode, SquadMode::Normal);
}

#[test]
fn low_health_physics_object_is_queued_captured_and_launched_as_debris() {
    let database = database(20.0, 1);
    let gameplay = gameplay(&database);
    let mut world = test_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "owner_squad", -Vec3::X * 20.0);
    let target_id = spawn(&mut world, &database, 2, "fragile_squad", Vec3::X * 3.0);
    make_physical(&mut world, target_id);
    let target_unit_id = squad_leader(&world, target_id);
    let execution_id = world
        .invoke_wave_power(&database, invocation(owner_id, true, PowerUserId::INVALID))
        .unwrap();

    super::update(&mut world, 0.1, &database, Some(&gameplay));
    assert!(world.active_wave_powers()[0].captured_objects().is_empty());
    assert!(world.get_unit(target_unit_id).is_none());
    super::update(&mut world, 0.01, &database, Some(&gameplay));
    let execution = &world.active_wave_powers()[0];
    assert_eq!(execution.captured_objects().len(), 1);
    let captured_unit_id = execution.captured_objects()[0].unit_id();
    assert_ne!(captured_unit_id, target_unit_id);
    let pickup_id = execution.captured_objects()[0].pickup_attachment_id();
    let source = assert_captured_debris_source(&mut world, captured_unit_id, pickup_id);

    assert!(world.submit_wave_power_input(&database, execution_id, NativePowerInput::Shutdown,));
    super::update(&mut world, 0.01, &database, Some(&gameplay));
    let execution = &world.active_wave_powers()[0];
    assert_eq!(execution.state(), WaveGravityBallState::Exploding);
    assert!(execution.captured_objects().is_empty());
    assert!(world.get_unit(captured_unit_id).is_none());
    assert_launched_debris(&world, captured_unit_id, pickup_id, source);
}

#[test]
fn alive_unit_rips_scenario_profile_part_into_short_lived_fake_spring_object() {
    let mut database = database(0.0, 2);
    set_base_value(&mut database, "ThrowPartChancePulling", "100");
    let mut gameplay = gameplay(&database);
    let part_material = PhysicsMaterial {
        mass: 150.0,
        angular_damping: 0.2,
        ..PhysicsMaterial::default()
    };
    let part = ThrownDamagePart::new(
        vec!["Panel".to_owned()],
        BoxCollider::new(Vec3::splat(0.5), Vec3::X),
        part_material,
        1.0,
    )
    .unwrap();
    gameplay.insert_test_damage_parts("target", DamagePartProfile::new(vec![Some(part)]));
    let mut world = test_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "owner_squad", -Vec3::X * 20.0);
    let target_id = spawn(&mut world, &database, 2, "target_squad", Vec3::X * 3.0);
    let target_unit_id = squad_leader(&world, target_id);
    world
        .invoke_wave_power(&database, invocation(owner_id, true, PowerUserId::INVALID))
        .unwrap();

    super::update(&mut world, 0.1, &database, Some(&gameplay));

    let execution = &world.active_wave_powers()[0];
    assert_eq!(execution.fake_objects().len(), 1);
    assert!(execution.captured_objects().is_empty());
    let part_id = execution.fake_objects()[0].unit_id();
    let source = world.get_unit(target_unit_id).unwrap();
    assert_eq!(source.visual_mesh_mask().hidden(), &["panel"]);
    let part = world.get_unit(part_id).unwrap();
    assert_eq!(part.base.player_id, 1);
    assert_eq!(part.proto_object_name, "target");
    assert_eq!(part.visual_mesh_mask().only(), &["panel"]);
    assert!(part.squad_id.is_none());
    assert!(part.base.velocity.length() > 0.0);
    assert_close(
        part.physics.as_ref().unwrap().material().angular_damping,
        0.8,
    );

    super::update(&mut world, 0.15, &database, Some(&gameplay));
    assert_close(world.get_unit(part_id).unwrap().visual_opacity(), 2.0 / 3.0);

    super::update(&mut world, 0.15, &database, Some(&gameplay));

    assert!(world.get_unit(part_id).is_none());
    assert!(world.get_unit(target_unit_id).is_some());
}

fn assert_captured_debris_source(
    world: &mut World,
    captured_unit_id: EntityId,
    pickup_id: EntityId,
) -> DebrisSourceSnapshot {
    assert_eq!(
        world.get_object(pickup_id).unwrap().proto_object_name,
        "wave_pickup"
    );
    let captured_unit = world.get_unit(captured_unit_id).unwrap();
    let body = captured_unit.physics.as_ref().unwrap();
    let center_offset = body.collider().center_offset;
    let captured_center = captured_unit.base.position + center_offset;
    let pickup = world.get_object(pickup_id).unwrap();
    assert!(pickup.base.position.abs_diff_eq(captured_center, 0.000_1));
    assert!(pickup.object_state.attachment_local_offset().abs_diff_eq(
        crate::entities::squads::formation_offset_to_local(
            captured_unit.base.forward,
            center_offset,
        ),
        0.000_1,
    ));
    assert!(captured_unit.is_physics_replacement());
    assert_eq!(captured_unit.base.player_id, 1);
    assert!(!captured_unit.is_auto_attackable());
    assert_eq!(captured_unit.obstruction_half_extents, Vec3::ZERO);
    assert_close(body.material().angular_damping, 0.8);
    assert_close(body.material().linear_damping, 0.1);
    assert!(captured_unit.base.velocity.length() > 0.0);
    world
        .get_unit_mut(captured_unit_id)
        .unwrap()
        .object_state
        .set_visual_variation_index(2);
    let captured_unit = world.get_unit(captured_unit_id).unwrap();
    let position = captured_unit.base.position
        + captured_unit
            .physics
            .as_ref()
            .unwrap()
            .collider()
            .center_offset;
    let ball_id = world.active_wave_powers()[0].ball_object_id();
    let ball = world.get_unit(ball_id).unwrap().base.position;
    DebrisSourceSnapshot {
        position,
        visual_center_offset: -captured_unit
            .physics
            .as_ref()
            .unwrap()
            .collider()
            .center_offset,
        expected_velocity: captured_unit.base.velocity
            + (position - ball).normalize_or(Vec3::X) * 15.0,
        forward: captured_unit.base.forward,
    }
}

fn assert_launched_debris(
    world: &World,
    captured_unit_id: EntityId,
    pickup_id: EntityId,
    source: DebrisSourceSnapshot,
) {
    let (debris_id, debris) = world
        .projectiles
        .iter()
        .find(|(_, projectile)| {
            projectile
                .proto_object_name
                .eq_ignore_ascii_case("wave_debris")
        })
        .expect("captured object should become one debris projectile");
    assert_eq!(debris.visual_proto_object_name(), "fragile");
    assert_eq!(debris.object_state.visual_variation_index(), Some(2));
    assert_eq!(debris.base.player_id, 2);
    assert_eq!(debris.created_by_player_id(), 1);
    assert_eq!(debris.visual_center_offset(), source.visual_center_offset);
    assert!(debris.base.position.abs_diff_eq(source.position, 0.000_1));
    assert!(
        debris
            .base
            .velocity
            .abs_diff_eq(source.expected_velocity, 0.000_1)
    );
    assert!(debris.base.forward.abs_diff_eq(source.forward, 0.000_1));
    assert!(debris.affected_by_gravity);
    assert_close(debris.gravity, 9.5);
    assert_eq!(
        world
            .get_object(pickup_id)
            .unwrap()
            .object_state
            .attached_to(),
        Some(debris_id)
    );
    assert_eq!(
        world
            .get_object(pickup_id)
            .unwrap()
            .object_state
            .attachment_local_offset(),
        Vec3::ZERO
    );
    assert!(debris.object_state.attachments().contains(&pickup_id));
    assert!(world.get_unit(captured_unit_id).is_none());
}

#[test]
fn explosion_throws_only_eligible_units_with_authored_action_velocity() {
    let database = database(20.0, 1);
    let gameplay = gameplay(&database);
    let mut world = test_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "owner_squad", -Vec3::X * 20.0);
    let fragile_id = spawn(&mut world, &database, 2, "fragile_squad", Vec3::X * 3.0);
    let target_id = spawn(&mut world, &database, 1, "target_squad", Vec3::X * 8.0);
    let covered_id = spawn(&mut world, &database, 1, "target_squad", Vec3::Z * 8.0);
    make_physical(&mut world, fragile_id);
    make_physical(&mut world, covered_id);
    world.get_squad_mut(covered_id).unwrap().mode = SquadMode::Cover;
    let target_unit_id = squad_leader(&world, target_id);
    let covered_unit_id = squad_leader(&world, covered_id);
    let execution_id = world
        .invoke_wave_power(&database, invocation(owner_id, true, PowerUserId::INVALID))
        .unwrap();

    super::update(&mut world, 0.1, &database, Some(&gameplay));
    super::update(&mut world, 0.01, &database, Some(&gameplay));
    assert_eq!(world.active_wave_powers()[0].captured_objects().len(), 1);
    world.power_manager.wave_executions[0].current_explosion_damage_bank = 600.0;
    let ball_id = world.active_wave_powers()[0].ball_object_id();
    assert!(world.submit_wave_power_input(&database, execution_id, NativePowerInput::Shutdown));
    super::update(&mut world, 0.01, &database, Some(&gameplay));

    let expected_speed = 20.0 * (600.0 / 1_500.0);
    let launch_angle = 30.0_f32.to_radians();
    let expected = Vec3::new(launch_angle.cos(), launch_angle.sin(), 0.0) * expected_speed;
    let target = world.get_unit(target_unit_id).unwrap();
    assert!(target.base.velocity.abs_diff_eq(expected, 0.000_1));
    assert!(target.physics.is_some());
    assert!(target.is_thrown());
    assert_eq!(target.thrown_by(), Some(ball_id));
    assert_eq!(
        world.get_unit(covered_unit_id).unwrap().base.velocity,
        Vec3::ZERO
    );
}

#[test]
fn packed_wave_input_routes_and_owner_profile_validation_match_native_contract() {
    let base_database = database(0.0, 2);
    let gameplay = gameplay(&base_database);
    let mut world = test_world(&base_database);
    assert_eq!(
        world.invoke_wave_power(
            &base_database,
            invocation(
                EntityId::INVALID,
                true,
                PowerUserId::new(1, WAVE_POWER_TYPE, 1),
            ),
        ),
        Err(NativePowerError::InvalidTarget)
    );
    let owner_id = spawn(
        &mut world,
        &base_database,
        1,
        "owner_squad",
        -Vec3::X * 20.0,
    );
    assert_eq!(
        world.invoke_wave_power(
            &base_database,
            invocation(owner_id, true, PowerUserId::new(1, 5, 1)),
        ),
        Err(NativePowerError::InvalidData("PowerUserID"))
    );
    let mut invalid_tick = database(0.0, 2);
    set_base_value(&mut invalid_tick, "TickLength", "0");
    assert_eq!(
        world.invoke_wave_power(
            &invalid_tick,
            invocation(owner_id, true, PowerUserId::INVALID),
        ),
        Err(NativePowerError::InvalidData("TickLength"))
    );

    let user_id = PowerUserId::new(1, WAVE_POWER_TYPE, 91);
    let executor = CommandExecutor::with_database(&base_database);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::Power(invoke_command(owner_id, user_id)),
    );
    let execution_id = world.active_wave_powers()[0].id();
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            PowerUserId::new(1, WAVE_POWER_TYPE, 92),
            PowerInputCommandType::Position,
            Vec3::X * 4.0,
        )),
    );
    assert_eq!(
        world.active_wave_powers()[0].desired_ball_position(),
        Vec3::ZERO
    );
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Position,
            Vec3::X * 4.0,
        )),
    );
    assert_eq!(
        world.active_wave_powers()[0].desired_ball_position(),
        Vec3::X * 4.0
    );
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Confirm,
            Vec3::ZERO,
        )),
    );
    super::update(&mut world, 0.01, &base_database, Some(&gameplay));
    assert_eq!(
        world
            .active_wave_powers()
            .iter()
            .find(|execution| execution.id() == execution_id)
            .unwrap()
            .state(),
        WaveGravityBallState::Exploding
    );
}

fn test_world(database: &Database) -> World {
    let mut world = World::with_seed(47);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world.configure_prototype_catalogs(database);
    world
}

fn gameplay(database: &Database) -> GameplayCatalog {
    let mut gameplay = GameplayCatalog::from_tactics(
        database,
        [
            (
                "wave_lightning".to_owned(),
                tactic("Lightning", "wave_lightning", 210.0, 5.0),
            ),
            (
                "wave_debris".to_owned(),
                tactic("ThrownDebris", "wave_debris", 325.0, 3.0),
            ),
            (
                "wave_explode".to_owned(),
                tactic("Wave", "wave_explode", 750.0, 20.0),
            ),
        ],
    );
    gameplay.insert_test_physics_replacement(
        "fragile",
        PhysicsReplacementProfile::new(
            "fragile_replacement",
            PhysicsMaterial::default(),
            BoxCollider::new(Vec3::splat(0.5), Vec3::new(0.25, 1.0, -0.5)),
        )
        .with_clamshell(),
    );
    gameplay.insert_test_physics_replacement(
        "target",
        PhysicsReplacementProfile::new(
            "target_replacement",
            PhysicsMaterial::default(),
            BoxCollider::new(Vec3::splat(0.75), Vec3::ZERO),
        ),
    );
    gameplay
}

fn spawn(
    world: &mut World,
    database: &Database,
    player_id: u8,
    prototype: &str,
    position: Vec3,
) -> EntityId {
    let prototype_id = squad_prototype_id(database, prototype).unwrap();
    spawn_squad_at(world, database, player_id, prototype_id, position, Vec3::Z).unwrap()
}

fn squad_leader(world: &World, squad_id: EntityId) -> EntityId {
    world.get_squad(squad_id).unwrap().unit_ids[0]
}

fn make_physical(world: &mut World, squad_id: EntityId) {
    let unit_id = squad_leader(world, squad_id);
    world.get_unit_mut(unit_id).unwrap().physics = Some(PhysicsBody::ground_vehicle(
        PhysicsMaterial::default(),
        BoxCollider::new(Vec3::splat(2.0), Vec3::ZERO),
        0.0,
        0.0,
        0.0,
        0.0,
    ));
}

fn invocation(
    squad_id: EntityId,
    ignore_requirements: bool,
    power_user_id: PowerUserId,
) -> WavePowerInvocation {
    WavePowerInvocation {
        player_id: 1,
        proto_power_id: POWER_ID,
        power_level: 0,
        squad_id,
        target_location: Vec3::ZERO,
        ignore_requirements,
        power_user_id,
    }
}

fn power_grant(squad_id: EntityId) -> PowerGrant {
    PowerGrant {
        proto_power_id: POWER_ID,
        squad_id,
        uses: 1,
        icon_location: -1,
        ignore_cost: false,
        ignore_tech_prerequisites: false,
        ignore_population: false,
    }
}

fn execute_command(executor: &CommandExecutor<'_>, world: &mut World, command: QueuedCommand) {
    executor.execute(
        world,
        &CommandEntry {
            command,
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn invoke_command(squad_id: EntityId, user_id: PowerUserId) -> PowerCommand {
    let mut base = Command {
        player_id: 1,
        ..Command::default()
    };
    base.set_flag(power_command_flags::NO_COST, true);
    PowerCommand {
        base,
        power_type: PowerCommandType::InvokePower2,
        proto_power_id: POWER_ID,
        target_location: Vec3::ZERO.extend(0.0),
        squad_id,
        power_user_id: user_id.raw().cast_signed(),
        ..PowerCommand::default()
    }
}

fn input_command(
    user_id: PowerUserId,
    input_type: PowerInputCommandType,
    vector: Vec3,
) -> PowerInputCommand {
    PowerInputCommand {
        base: Command {
            player_id: 1,
            ..Command::default()
        },
        input_type,
        vector: vector.extend(0.0),
        power_user_id: user_id,
    }
}

fn database(health_to_capture: f32, maximum_captured: i32) -> Database {
    Database {
        objects: vec![
            wave_ball("wave_ball", 0),
            projectile("wave_lightning", 1),
            projectile("wave_debris", 2),
            projectile("wave_explode", 3),
            visual_with_lifespan("wave_beam", 4, 0.25),
            visual("wave_pickup", 5),
            unit("owner", 6, 500.0, 20.0),
            unit("target", 7, 500.0, 50.0),
            unit("fragile", 8, 10.0, 10.0),
        ],
        squads: vec![
            squad("owner_squad", 10, "owner"),
            squad("target_squad", 11, "target"),
            squad("fragile_squad", 12, "fragile"),
        ],
        powers: vec![wave_power(health_to_capture, maximum_captured)],
        game_data: Some(GameData {
            projectile_gravity: Some(9.5),
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    ..ResourceDef::default()
                }],
            }),
            pops: Some(PopsWrapper::default()),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn wave_power(health_to_capture: f32, maximum_captured: i32) -> Power {
    Power {
        name: "TestWave".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Wave".to_owned()),
            cost: Some(PowerCost {
                supplies: Some(10.0),
                ..PowerCost::default()
            }),
            base_data_level: Some(DataLevel {
                entries: vec![
                    float_data("TickLength", 0.2),
                    float_data("SuppliesPerTick", 10.0),
                    float_data("CommandInterval", 0.2),
                    float_data("MinBallDistance", 0.0),
                    float_data("MaxBallDistance", 55.0),
                    float_data("MaxBallHeight", 25.0),
                    float_data("MaxBallSpeedStagnant", 30.0),
                    float_data("MaxBallSpeedPulling", 15.0),
                    float_data("ExplodeTime", 1.5),
                    float_data("ExplosionForceOnDebris", 15.0),
                    float_data("NudgeStrength", 1.0),
                    float_data("InitialLateralPullStrength", 25.0),
                    float_data("CapturedSpringStrength", 3500.0),
                    float_data("CapturedSpringDampening", 0.25),
                    float_data("CapturedSpringRestLength", 1.0),
                    float_data("CapturedMinLateralSpeed", 10.0),
                    float_data("CapturedRadialSpacing", 0.1),
                    float_data("PickupObjectRate", 0.0),
                    float_data("DebrisAngularDamping", 0.8),
                    int_data("LightningPerTick", 1),
                    int_data("NudgeChancePulling", 100),
                    int_data("ThrowPartChancePulling", 0),
                    int_data("LightningChancePulling", 35),
                    data("protoobject", "LightningProjectile", "wave_lightning"),
                    data("protoobject", "LightningBeamVisual", "wave_beam"),
                    data("protoobject", "DebrisProjectile", "wave_debris"),
                    data("protoobject", "ExplodeProjectile", "wave_explode"),
                    data("protoobject", "PickupAttachment", "wave_pickup"),
                    data("sound", "ExplodeSound", "wave_explosion"),
                    float_data("HealthToCapture", health_to_capture),
                    float_data("RipAttachmentChancePulling", 100.0),
                    data("bool", "ThrowUnitsOnExplosion", "true"),
                    float_data("MinDamageBankPercentToThrow", 0.15),
                ],
                ..DataLevel::default()
            }),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("protoobject", "BallObject", "wave_ball"),
                    float_data("MaxExplosionDamageBankPerCaptured", 1500.0),
                    float_data("ExplosionDamageBankPerTick", 150.0),
                    int_data("MaxCapturedObjects", maximum_captured),
                    float_data("MinBallHeight", 4.0),
                    float_data("PullingRange", 17.0),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn tactic(name: &str, projectile: &str, damage: f32, radius: f32) -> TacticData {
    TacticData {
        weapons: vec![Weapon {
            name: name.to_owned(),
            damage_per_second: Some(damage),
            weapon_type: Some("LeaderPower".to_owned()),
            projectile: Some(projectile.to_owned()),
            aoe_radius: Some(radius),
            throw_offset_angle: name.eq_ignore_ascii_case("Wave").then_some(0.0),
            throw_velocity: name.eq_ignore_ascii_case("Wave").then_some(20.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: format!("{name}Action"),
            weapon: Some(name.to_owned()),
            default: Some(true),
            ..Action::default()
        }],
        ..TacticData::default()
    }
}

fn projectile(name: &str, dbid: i32) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Projectile".to_owned()),
        tactics: Some(format!("{name}.tactics")),
        velocity: Some(if name == "wave_lightning" {
            1000.0
        } else {
            50.0
        }),
        lifespan: Some(if name == "wave_debris" { 10.0 } else { 1.0 }),
        max_projectile_height: (name == "wave_debris").then_some(8.0),
        flags: if name == "wave_debris" {
            vec![
                "IsAffectedByGravity".to_owned(),
                "ProjectileTumbles".to_owned(),
            ]
        } else {
            Vec::new()
        },
        ..ProtoObject::default()
    }
}

fn visual(name: &str, dbid: i32) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        ..ProtoObject::default()
    }
}

fn wave_ball(name: &str, dbid: i32) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Unit".to_owned()),
        flags: vec!["Invulnerable".to_owned(), "NonCollideable".to_owned()],
        hitpoints: Some(500.0),
        ..ProtoObject::default()
    }
}

fn visual_with_lifespan(name: &str, dbid: i32, lifespan: f32) -> ProtoObject {
    ProtoObject {
        lifespan: Some(lifespan),
        ..visual(name, dbid)
    }
}

fn unit(name: &str, dbid: i32, hitpoints: f32, combat_value: f32) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Unit".to_owned()),
        object_types: vec!["Infantry".to_owned()],
        flags: vec!["ProjectileObstructable".to_owned()],
        hitpoints: Some(hitpoints),
        combat_value: Some(combat_value),
        obstruction_radius_x: Some(2.0),
        obstruction_radius_y: Some(2.0),
        obstruction_radius_z: Some(2.0),
        ..ProtoObject::default()
    }
}

fn squad(name: &str, dbid: i32, member: &str) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        dbid: Some(dbid),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: member.to_owned(),
                count: 1,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn set_base_value(database: &mut Database, name: &str, value: &str) {
    let entry = database.powers[0]
        .attributes
        .as_mut()
        .unwrap()
        .base_data_level
        .as_mut()
        .unwrap()
        .entries
        .iter_mut()
        .find(|entry| entry.name == name)
        .unwrap();
    entry.value = value.to_owned();
}

fn float_data(name: &str, value: f32) -> DataEntry {
    data("float", name, &value.to_string())
}

fn int_data(name: &str, value: i32) -> DataEntry {
    data("int", name, &value.to_string())
}

fn data(data_type: &str, name: &str, value: &str) -> DataEntry {
    DataEntry {
        data_type: data_type.to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "expected {expected}, got {actual}"
    );
}
