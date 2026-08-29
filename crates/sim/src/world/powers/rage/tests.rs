use super::*;
use crate::command::Command;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{
    PowerCommand, PowerCommandType, PowerInputCommand, PowerInputCommandType, PowerUserId,
    power_input_command_flags,
};
use crate::entities::{SquadMode, UnitDataScalar};
use crate::entity::Entity;
use crate::executor::CommandExecutor;
use crate::gameplay::GameplayCatalog;
use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
use crate::player::{PowerGrant, TeamRelation};
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use crate::world::{DisruptionPowerInvocation, NativePowerError, RagePowerInvocation};
use glam::{Vec3, Vec4};
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::{Database, GameData, Power, ProtoObject, Squad as ProtoSquad};

#[test]
fn paid_cast_binds_charge_applies_owner_state_and_shutdown_reverses_it() {
    let database = database();
    let mut world = test_world(&database, 1);
    let owner_id = spawn(&mut world, &database, 1, "rage_owner_squad", Vec3::ZERO);
    world.get_player_mut(1).unwrap().set_resource(0, 1_000.0);
    assert!(world.grant_player_power(1, &database, power_grant(0, owner_id)));

    let execution_id = world
        .invoke_rage_power(&database, rage_invocation(owner_id, false))
        .unwrap();
    let execution = &world.active_rage_powers()[0];
    assert_eq!(execution.id(), execution_id);
    assert!(execution.uses_pather());
    assert_eq!(execution.hand_attachment_ids().len(), 2);
    assert_close(world.get_player(1).unwrap().get_resource(0), 900.0);
    assert_eq!(
        world
            .get_player(1)
            .unwrap()
            .power_entry(0)
            .unwrap()
            .finite_uses_remaining(),
        0
    );
    assert_owner_active(&world, owner_id);
    let hand_ids = execution.hand_attachment_ids().to_vec();

    world
        .get_squad_mut(owner_id)
        .unwrap()
        .move_to(Vec3::X * 100.0);
    assert!(world.get_squad(owner_id).unwrap().move_target.is_none());
    advance(&mut world, &database, 10, 0.01);
    assert_close(world.get_player(1).unwrap().get_resource(0), 899.0);

    assert!(world.submit_rage_power_input(&database, execution_id, NativePowerInput::Shutdown,));
    assert!(world.active_rage_powers().is_empty());
    assert_owner_inactive(&world, owner_id);
    assert!(hand_ids.iter().all(|id| world.get_object(*id).is_none()));
}

#[test]
fn wire_input_targets_in_radius_charges_jump_lands_and_honors_no_cost() {
    let database = database();
    let gameplay = gameplay(&database);
    let mut world = hostile_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "rage_owner_squad", Vec3::ZERO);
    let target_id = spawn(&mut world, &database, 3, "enemy_squad", Vec3::X * 10.0);
    let outside_id = spawn(&mut world, &database, 3, "outside_squad", Vec3::X * 60.0);
    make_target_physical(&mut world, target_id);
    world.get_player_mut(1).unwrap().set_resource(0, 1_000.0);
    assert!(world.grant_player_power(1, &database, power_grant(0, owner_id)));
    let user_id = PowerUserId::new(1, RAGE_POWER_TYPE, 17);
    let executor = CommandExecutor::with_database(&database);

    execute_command(
        &executor,
        &mut world,
        QueuedCommand::Power(invoke_command(owner_id, user_id)),
    );
    assert_eq!(world.active_rage_powers()[0].power_user_id(), user_id);
    assert!(!world.active_rage_powers()[0].uses_pather());
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(user_id, Vec3::ZERO, false)),
    );
    let execution = &world.active_rage_powers()[0];
    assert_eq!(execution.target_squad_id(), Some(target_id));
    assert_ne!(execution.target_squad_id(), Some(outside_id));
    assert_eq!(execution.phase(), RagePowerPhase::Jumping);
    assert_close(world.get_player(1).unwrap().get_resource(0), 860.0);

    super::update(&mut world, 0.3, &database, Some(&gameplay));
    let execution = &world.active_rage_powers()[0];
    assert_eq!(execution.phase(), RagePowerPhase::Attacking);
    assert!(
        world
            .get_unit(squad_leader(&world, target_id))
            .unwrap()
            .hitpoints
            < 100.0
    );
    assert_close(world.get_player(1).unwrap().get_resource(0), 850.0);
    let velocity = world
        .get_unit(squad_leader(&world, target_id))
        .unwrap()
        .base
        .velocity;
    assert!(velocity.x > 0.0 && velocity.y > 0.0);

    let before = world.get_player(1).unwrap().get_resource(0);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(user_id, Vec3::X, false)),
    );
    assert_close(world.get_player(1).unwrap().get_resource(0), before);
    world.power_manager.rage_executions[0].retarget_remaining = 0.0;
    world.get_player_mut(1).unwrap().set_resource(0, 0.0);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(user_id, Vec3::X, true)),
    );
    assert_eq!(world.active_rage_powers().len(), 1);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(user_id, Vec3::X, false)),
    );
    assert!(world.active_rage_powers().is_empty());
    assert_owner_inactive(&world, owner_id);
}

#[test]
fn aura_buffs_allied_members_chooses_size_fx_and_reverses_on_leave() {
    let database = database();
    let mut world = hostile_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "rage_owner_squad", Vec3::ZERO);
    let small_id = spawn(&mut world, &database, 2, "ally_small_squad", Vec3::X * 4.0);
    let medium_id = spawn(&mut world, &database, 2, "ally_medium_squad", Vec3::X * 7.0);
    let large_id = spawn(&mut world, &database, 2, "ally_large_squad", Vec3::X * 10.0);
    let execution_id = world
        .invoke_rage_power(&database, rage_invocation(owner_id, true))
        .unwrap();

    advance(&mut world, &database, 10, 0.01);
    let execution = &world.active_rage_powers()[0];
    for id in [owner_id, small_id, medium_id, large_id] {
        assert!(execution.aura_squad_ids().contains(&id));
    }
    assert_close(unit_scalar(&world, small_id, UnitDataScalar::Damage), 1.25);
    assert_close(unit_scalar(&world, medium_id, UnitDataScalar::Damage), 1.25);
    assert_close(unit_scalar(&world, large_id, UnitDataScalar::Damage), 1.25);
    assert_eq!(attached_names(&world, small_id), vec!["rage_aura_small"]);
    assert_eq!(attached_names(&world, medium_id), vec!["rage_aura_medium"]);
    assert_eq!(attached_names(&world, large_id), vec!["rage_aura_large"]);
    assert!(
        !attached_names(&world, owner_id)
            .iter()
            .any(|name| name.starts_with("rage_aura_"))
    );

    assert!(world.teleport_squad(medium_id, Vec3::X * 100.0));
    advance(&mut world, &database, 20, 0.01);
    assert_close(unit_scalar(&world, medium_id, UnitDataScalar::Damage), 1.0);
    assert!(attached_names(&world, medium_id).is_empty());

    assert!(world.submit_rage_power_input(&database, execution_id, NativePowerInput::Shutdown,));
    for id in [small_id, large_id] {
        assert_close(unit_scalar(&world, id, UnitDataScalar::Damage), 1.0);
        assert!(attached_names(&world, id).is_empty());
    }
}

#[test]
fn attributed_landing_kill_repairs_reinforces_and_attaches_heal_fx() {
    let database = database();
    let gameplay = gameplay(&database);
    let mut world = hostile_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "rage_owner_squad", Vec3::ZERO);
    let target_id = spawn(&mut world, &database, 3, "fragile_squad", Vec3::X * 10.0);
    let target_unit_id = squad_leader(&world, target_id);
    let removed_id = world.get_squad(owner_id).unwrap().unit_ids[1];
    world.remove_unit(removed_id).unwrap();
    assert_eq!(world.get_squad(owner_id).unwrap().unit_ids.len(), 1);
    let execution_id = world
        .invoke_rage_power(&database, rage_invocation(owner_id, true))
        .unwrap();
    assert!(world.submit_rage_power_input(
        &database,
        execution_id,
        NativePowerInput::Direction(Vec3::X),
    ));

    world.game_time_ms = 300;
    world.update_entities_with_database_and_gameplay(0.3, &database, &gameplay);

    assert!(
        world
            .get_unit(target_unit_id)
            .is_none_or(|target| !target.is_alive())
    );
    let owner = world.get_squad(owner_id).unwrap();
    assert_eq!(owner.unit_ids.len(), 2);
    assert!(owner.unit_ids.iter().all(|id| {
        let unit = world.get_unit(*id).unwrap();
        unit.hitpoints.to_bits() == unit.max_hitpoints.to_bits()
    }));
    assert!(world.objects.iter().any(|(_, object)| {
        object.proto_object_name == "rage_heal"
            && object.object_state.attached_to() == Some(owner.unit_ids[0])
    }));
}

#[test]
fn disruption_stops_active_rage_and_no_cost_cannot_restart_it() {
    let database = database();
    let mut world = test_world(&database, 1);
    let owner_id = spawn(&mut world, &database, 1, "rage_owner_squad", Vec3::ZERO);
    world
        .invoke_rage_power(&database, rage_invocation(owner_id, true))
        .unwrap();
    world
        .invoke_disruption_power(
            &database,
            DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: 1,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .unwrap();

    for tick in 1..=10 {
        advance(&mut world, &database, tick * 50, 0.05);
        if world.active_rage_powers().is_empty() {
            break;
        }
    }
    assert!(world.active_rage_powers().is_empty());
    assert_owner_inactive(&world, owner_id);
    let disruption_id = world
        .active_disruption_powers()
        .iter()
        .find(|execution| execution.is_active())
        .expect("active disruption field")
        .id();
    assert_eq!(
        world.invoke_rage_power(&database, rage_invocation(owner_id, true)),
        Err(NativePowerError::Disrupted(disruption_id))
    );
}

#[test]
fn incomplete_retail_profile_is_rejected_even_for_no_cost_casts() {
    let mut database = database();
    database.powers[0].attributes.as_mut().unwrap().data_levels[0]
        .entries
        .retain(|entry| entry.name != "MotionBlurTime");
    let mut world = test_world(&database, 1);
    let owner_id = spawn(&mut world, &database, 1, "rage_owner_squad", Vec3::ZERO);

    assert_eq!(
        world.invoke_rage_power(&database, rage_invocation(owner_id, true)),
        Err(NativePowerError::MissingData("MotionBlurTime"))
    );
}

fn assert_owner_active(world: &World, squad_id: EntityId) {
    let squad = world.get_squad(squad_id).unwrap();
    assert!(squad.is_raging());
    assert!(squad.is_sprinting());
    assert_eq!(squad.mode, SquadMode::Power);
    for unit_id in &squad.unit_ids {
        let unit = world.get_unit(*unit_id).unwrap();
        assert_close(unit.data_scalar(UnitDataScalar::Damage), 2.0);
        assert_close(unit.data_scalar(UnitDataScalar::DamageTaken), 1.25);
        assert_close(unit.data_scalar(UnitDataScalar::Velocity), 2.0);
    }
}

fn assert_owner_inactive(world: &World, squad_id: EntityId) {
    let squad = world.get_squad(squad_id).unwrap();
    assert!(!squad.is_raging());
    assert!(!squad.is_sprinting());
    assert_eq!(squad.mode, SquadMode::Normal);
    for unit_id in &squad.unit_ids {
        let unit = world.get_unit(*unit_id).unwrap();
        assert_close(unit.data_scalar(UnitDataScalar::Damage), 1.0);
        assert_close(unit.data_scalar(UnitDataScalar::DamageTaken), 1.0);
        assert_close(unit.data_scalar(UnitDataScalar::Velocity), 1.0);
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
    PowerCommand {
        base: Command {
            player_id: 1,
            ..Command::default()
        },
        power_type: PowerCommandType::InvokePower2,
        proto_power_id: 0,
        power_level: 0,
        target_location: Vec4::ZERO,
        squad_id,
        power_user_id: user_id.raw().cast_signed(),
        ..PowerCommand::default()
    }
}

fn input_command(user_id: PowerUserId, direction: Vec3, no_cost: bool) -> PowerInputCommand {
    let mut base = Command {
        player_id: 1,
        ..Command::default()
    };
    base.set_flag(power_input_command_flags::NO_COST, no_cost);
    PowerInputCommand {
        base,
        input_type: PowerInputCommandType::Direction,
        vector: direction.extend(0.0),
        power_user_id: user_id,
    }
}

fn test_world(database: &Database, players: u8) -> World {
    let mut world = World::with_seed(31);
    world.init_players(players);
    world.configure_prototype_catalogs(database);
    world
}

fn hostile_world(database: &Database) -> World {
    let mut world = test_world(database, 3);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 1;
    world.get_player_mut(3).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    assert_eq!(world.player_relation(1, 2), Some(TeamRelation::Ally));
    world
}

fn advance(world: &mut World, database: &Database, game_time_ms: u32, dt: f32) {
    world.game_time_ms = game_time_ms;
    world.update_entities_with_database(dt, database);
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

fn unit_scalar(world: &World, squad_id: EntityId, scalar: UnitDataScalar) -> f32 {
    world
        .get_unit(squad_leader(world, squad_id))
        .unwrap()
        .data_scalar(scalar)
}

fn attached_names(world: &World, squad_id: EntityId) -> Vec<&str> {
    let unit_id = squad_leader(world, squad_id);
    let mut names = world
        .objects
        .iter()
        .filter_map(|(_, object)| {
            (object.object_state.attached_to() == Some(unit_id))
                .then_some(object.proto_object_name.as_str())
        })
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn make_target_physical(world: &mut World, squad_id: EntityId) {
    let unit = world.get_unit_mut(squad_leader(world, squad_id)).unwrap();
    unit.physics = Some(PhysicsBody::ground_vehicle(
        PhysicsMaterial::default(),
        BoxCollider::new(Vec3::ONE, Vec3::ZERO),
        0.0,
        0.0,
        0.0,
        0.0,
    ));
}

fn rage_invocation(squad_id: EntityId, ignore_requirements: bool) -> RagePowerInvocation {
    RagePowerInvocation {
        player_id: 1,
        proto_power_id: 0,
        power_level: 0,
        squad_id,
        target_location: Vec3::ZERO,
        ignore_requirements,
    }
}

fn power_grant(proto_power_id: i32, squad_id: EntityId) -> PowerGrant {
    PowerGrant {
        proto_power_id,
        squad_id,
        uses: 1,
        icon_location: -1,
        ignore_cost: false,
        ignore_tech_prerequisites: false,
        ignore_population: false,
    }
}

fn gameplay(database: &Database) -> GameplayCatalog {
    GameplayCatalog::from_tactics(
        database,
        [("rage_projectile".to_owned(), rage_impact_tactics())],
    )
}

fn rage_impact_tactics() -> TacticData {
    TacticData {
        weapons: vec![Weapon {
            name: "impact".to_owned(),
            damage_per_second: Some(20.0),
            aoe_radius: Some(5.0),
            aoe_primary_target_factor: Some(1.0),
            aoe_distance_factor: Some(0.0),
            aoe_damage_factor: Some(1.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "rage".to_owned(),
            weapon: Some("impact".to_owned()),
            ..Action::default()
        }],
        ..TacticData::default()
    }
}

fn database() -> Database {
    Database {
        objects: objects(),
        squads: squads(),
        powers: vec![rage_power(), disruption_power()],
        game_data: Some(GameData {
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

fn objects() -> Vec<ProtoObject> {
    vec![
        visual("rage_projectile", 1, None),
        visual("rage_hands", 2, None),
        visual("rage_teleport", 3, Some(0.5)),
        visual("rage_aura_small", 4, None),
        visual("rage_aura_medium", 5, None),
        visual("rage_aura_large", 6, None),
        visual("rage_heal", 7, Some(0.5)),
        unit("rage_owner", 20, 100.0, 20.0, 1.0),
        unit("enemy", 21, 100.0, 10.0, 1.0),
        unit("fragile", 22, 30.0, 10.0, 1.0),
        unit("ally_small", 23, 100.0, 10.0, 1.0),
        unit("ally_medium", 24, 100.0, 10.0, 4.0),
        unit("ally_large", 25, 100.0, 10.0, 8.0),
        unit("outside", 26, 100.0, 1_000.0, 1.0),
        visual("disruption_field", 30, None),
        visual("disruption_pulse", 31, Some(0.5)),
        visual("disruption_strike", 32, Some(0.5)),
        visual("disruption_bomber", 33, None),
    ]
}

fn squads() -> Vec<ProtoSquad> {
    vec![
        squad("rage_owner_squad", 100, "rage_owner", 2),
        squad("enemy_squad", 101, "enemy", 1),
        squad("fragile_squad", 102, "fragile", 1),
        squad("ally_small_squad", 103, "ally_small", 1),
        squad("ally_medium_squad", 104, "ally_medium", 1),
        squad("ally_large_squad", 105, "ally_large", 1),
        squad("outside_squad", 106, "outside", 1),
    ]
}

fn rage_power() -> Power {
    Power {
        name: "TestRage".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Rage".to_owned()),
            cost: Some(PowerCost {
                supplies: Some(100.0),
                ..PowerCost::default()
            }),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: rage_data(),
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn rage_data() -> Vec<DataEntry> {
    vec![
        float_data("TickLength", 0.2),
        float_data("SuppliesPerTick", 1.0),
        float_data("SuppliesPerTickAttacking", 5.0),
        float_data("SuppliesPerJump", 40.0),
        float_data("DamageMultiplier", 2.0),
        float_data("DamageTakenMultiplier", 1.25),
        float_data("SpeedMultiplier", 2.0),
        float_data("NudgeMultiplier", 1.0),
        float_data("ScanRadius", 40.0),
        float_data("TeleportTime", 0.3),
        float_data("TeleportLateralDistance", 5.0),
        float_data("TeleportJumpDistance", 10.0),
        float_data("TimeBetweenRetarget", 1.0),
        float_data("MotionBlurAmount", 0.5),
        float_data("MotionBlurDistance", 5.0),
        float_data("MotionBlurTime", 0.1),
        float_data("DistanceVsAngleWeight", 0.15),
        data("protoobject", "Projectile", "rage_projectile"),
        data("protoobject", "HandAttachObject", "rage_hands"),
        data("protoobject", "TeleportAttachObject", "rage_teleport"),
        data("protoobject", "AuraAttachFxSmall", "rage_aura_small"),
        data("protoobject", "AuraAttachFxMedium", "rage_aura_medium"),
        data("protoobject", "AuraAttachFxLarge", "rage_aura_large"),
        data("protoobject", "HealAttachFx", "rage_heal"),
        data("objecttype", "AuraFilterType", "Military"),
        float_data("HealPerKillCombatValue", 20.0),
        float_data("AuraRadius", 20.0),
        float_data("AuraDamageBonus", 1.25),
    ]
}

fn disruption_power() -> Power {
    Power {
        name: "TestDisruption".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Disruption".to_owned()),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("protoobject", "DisruptionObject", "disruption_field"),
                    data("protoobject", "PulseObject", "disruption_pulse"),
                    data("protoobject", "StrikeObject", "disruption_strike"),
                    data("sound", "PulseSound", "pulse"),
                    float_data("PulseSpacing", 1.0),
                    float_data("DisruptionRadius", 10.0),
                    float_data("DisruptionTimeSec", 5.0),
                    float_data("DisruptionStartTime", 0.1),
                    data("protoobject", "Bomber", "disruption_bomber"),
                    float_data("BomberBombTime", 0.05),
                    float_data("BomberFlyinDistance", 10.0),
                    float_data("BomberFlyinHeight", 3.0),
                    float_data("BomberBombHeight", 1.0),
                    float_data("BomberSpeed", 5.0),
                    float_data("BomberFlyOutTime", 1.0),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn visual(name: &str, dbid: i32, lifespan: Option<f32>) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        lifespan,
        tactics: (name == "rage_projectile").then(|| "rage_impact".to_owned()),
        ..ProtoObject::default()
    }
}

fn unit(name: &str, dbid: i32, hitpoints: f32, combat_value: f32, radius: f32) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Unit".to_owned()),
        object_types: vec!["Military".to_owned()],
        hitpoints: Some(hitpoints),
        combat_value: Some(combat_value),
        max_velocity: Some(6.0),
        obstruction_radius_x: Some(radius),
        obstruction_radius_y: Some(1.0),
        obstruction_radius_z: Some(radius),
        ..ProtoObject::default()
    }
}

fn squad(name: &str, dbid: i32, member: &str, count: i32) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        dbid: Some(dbid),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: member.to_owned(),
                count,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn float_data(name: &str, value: f32) -> DataEntry {
    data("float", name, &value.to_string())
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
        "{actual} != {expected}"
    );
}
