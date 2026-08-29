use super::*;
use crate::player::PowerGrant;
use crate::world::{CryoPowerInvocation, NativePowerError, NativePowerInvocation};
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::{GameData, Power, ProtoObject};

#[test]
fn cast_pays_and_owns_bomber_field_pulses_and_death_lifetime() {
    let database = database(1.0);
    let mut world = World::with_seed(17);
    world.init_players(1);
    world.get_player_mut(1).unwrap().set_resource(0, 1_000.0);
    assert!(world.grant_player_power(
        1,
        &database,
        PowerGrant {
            proto_power_id: 0,
            squad_id: EntityId::INVALID,
            uses: 1,
            icon_location: -1,
            ignore_cost: false,
            ignore_tech_prerequisites: false,
            ignore_population: false,
        },
    ));
    world.game_time_ms = 1_000;
    let used_power = world.subscribe_general_event(GeneralEventType::UsedPower, Some(1), false);

    let execution_id = world
        .invoke_native_power(
            &database,
            NativePowerInvocation {
                player_id: 1,
                proto_power_id: 0,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::new(20.0, 0.0, 30.0),
                ignore_requirements: false,
                power_user_id: crate::commands::PowerUserId::INVALID,
            },
        )
        .unwrap();

    let execution = &world.active_disruption_powers()[0];
    assert_eq!(execution.id(), execution_id);
    assert_eq!(execution.radius().to_bits(), 12.0_f32.to_bits());
    assert!(!execution.is_active());
    assert_eq!(
        world.get_player(1).unwrap().get_resource(0).to_bits(),
        550.0_f32.to_bits()
    );
    assert_eq!(
        world
            .get_player(1)
            .unwrap()
            .power_entry(0)
            .unwrap()
            .finite_uses_remaining(),
        0
    );
    assert!(world.general_event_fired(used_power));
    let bomber_id = execution.bomber_object_id();
    assert_eq!(
        world.get_object(bomber_id).unwrap().proto_object_name,
        "disruption_bomber"
    );

    advance(&mut world, &database, 1_500, 0.5);
    assert!(
        world.active_disruption_powers()[0]
            .disruption_object_id()
            .is_invalid()
    );
    advance(&mut world, &database, 2_000, 0.5);
    let field_id = world.active_disruption_powers()[0].disruption_object_id();
    assert_eq!(
        world.get_object(field_id).unwrap().proto_object_name,
        "disruption_field"
    );
    assert!(!world.active_disruption_powers()[0].is_active());

    advance(&mut world, &database, 2_500, 0.5);
    let execution = &world.active_disruption_powers()[0];
    assert!(execution.is_active());
    assert_eq!(execution.pulse_count(), 1);
    let first_pulses = pulse_ids(&world);
    assert_eq!(first_pulses.len(), 1);
    assert_eq!(
        world
            .entity_object_state(first_pulses[0])
            .unwrap()
            .attached_to(),
        Some(field_id)
    );

    assert_shutdown_death_lifetime(&mut world, &database, bomber_id, field_id);
}

fn assert_shutdown_death_lifetime(
    world: &mut World,
    database: &Database,
    bomber_id: EntityId,
    field_id: EntityId,
) {
    advance(world, database, 3_000, 0.5);
    assert!(world.active_disruption_powers().is_empty());
    assert!(world.get_object(bomber_id).is_none());
    let field = world.get_object(field_id).expect("death-state field");
    assert_eq!(
        field
            .object_state
            .scripted_animation()
            .unwrap()
            .animation_type(),
        "Death"
    );
    assert_eq!(pulse_ids(world).len(), 2);

    world.game_time_ms = 5_499;
    world.update_entities(0.05);
    assert!(world.get_object(field_id).is_some());
    world.game_time_ms = 5_500;
    world.update_entities(0.05);
    assert!(world.get_object(field_id).is_none());
    assert!(pulse_ids(world).is_empty());
}

#[test]
fn active_field_uses_strict_planar_radius_and_no_cost_does_not_bypass_it() {
    let mut database = database(10.0);
    let mut world = World::with_seed(29);
    world.init_players(1);
    assert!(world.grant_player_power(
        1,
        &database,
        PowerGrant {
            proto_power_id: 1,
            squad_id: EntityId::INVALID,
            uses: 1,
            icon_location: -1,
            ignore_cost: false,
            ignore_tech_prerequisites: false,
            ignore_population: false,
        },
    ));
    let disruption_id = world
        .invoke_disruption_power(
            &database,
            DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: 0,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .unwrap();
    advance(&mut world, &database, 500, 0.5);
    advance(&mut world, &database, 1_000, 0.5);
    advance(&mut world, &database, 1_500, 0.5);
    assert!(world.active_disruption_powers()[0].is_active());

    let blocked = world.invoke_cryo_power(
        &database,
        CryoPowerInvocation {
            player_id: 1,
            proto_power_id: 1,
            power_level: 0,
            squad_id: EntityId::INVALID,
            target_location: Vec3::new(11.99, 900.0, 0.0),
            ignore_requirements: false,
        },
    );
    assert_eq!(blocked, Err(NativePowerError::Disrupted(disruption_id)));
    assert!(world.active_cryo_powers().is_empty());
    assert_eq!(
        world
            .get_player(1)
            .unwrap()
            .power_entry(1)
            .unwrap()
            .finite_uses_remaining(),
        1
    );
    assert_eq!(
        world.invoke_cryo_power(
            &database,
            CryoPowerInvocation {
                target_location: Vec3::ZERO,
                ..cryo_invocation()
            },
        ),
        Err(NativePowerError::Disrupted(disruption_id)),
        "NO_COST must not bypass disruption"
    );

    world
        .invoke_cryo_power(
            &database,
            CryoPowerInvocation {
                target_location: Vec3::new(12.0, 0.0, 0.0),
                ..cryo_invocation()
            },
        )
        .expect("retail radius comparison is strict");
    assert_eq!(world.active_cryo_powers().len(), 1);

    database.powers[1]
        .attributes
        .as_mut()
        .unwrap()
        .not_disruptable = Some(true);
    world
        .invoke_cryo_power(
            &database,
            CryoPowerInvocation {
                target_location: Vec3::ZERO,
                ..cryo_invocation()
            },
        )
        .expect("NotDisruptable bypasses the field");
    assert_eq!(world.active_cryo_powers().len(), 2);
}

#[test]
fn overdue_pulse_update_emits_only_one_and_uses_growing_spacing() {
    let database = database(10.0);
    let mut world = World::new();
    world.init_players(1);
    world
        .invoke_disruption_power(
            &database,
            DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: 0,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .unwrap();

    advance(&mut world, &database, 1_500, 1.5);
    assert_eq!(world.active_disruption_powers()[0].pulse_count(), 1);
    advance(&mut world, &database, 2_500, 1.0);
    let execution = &world.active_disruption_powers()[0];
    assert_eq!(execution.pulse_count(), 2);
    assert_eq!(
        execution.next_pulse_time_seconds.to_bits(),
        3.0_f32.to_bits()
    );
}

fn advance(world: &mut World, database: &Database, game_time_ms: u32, dt: f32) {
    world.game_time_ms = game_time_ms;
    world.update_entities_with_database(dt, database);
}

fn cryo_invocation() -> CryoPowerInvocation {
    CryoPowerInvocation {
        player_id: 1,
        proto_power_id: 1,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location: Vec3::ZERO,
        ignore_requirements: true,
    }
}

fn pulse_ids(world: &World) -> Vec<EntityId> {
    world
        .objects
        .iter()
        .filter_map(|(id, object)| (object.proto_object_name == "disruption_pulse").then_some(id))
        .collect()
}

fn database(disruption_duration: f32) -> Database {
    Database {
        objects: vec![
            visual("disruption_field", None, &[]),
            visual("disruption_pulse", Some(5.0), &[]),
            visual("disruption_strike", Some(1.0), &[]),
            visual("disruption_bomber", None, &[]),
            visual("cryo_fx", Some(0.2), &[]),
            visual("cryo_bomber", None, &[]),
            visual("cryo_target", None, &["CanCryo"]),
        ],
        powers: vec![disruption_power(disruption_duration), cryo_power()],
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

fn disruption_power(duration: f32) -> Power {
    Power {
        name: "TestDisruption".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Disruption".to_owned()),
            cost: Some(PowerCost {
                supplies: Some(450.0),
                ..PowerCost::default()
            }),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("protoobject", "DisruptionObject", "disruption_field"),
                    data("protoobject", "PulseObject", "disruption_pulse"),
                    data("protoobject", "StrikeObject", "disruption_strike"),
                    data("sound", "PulseSound", "play_disruption_pulse"),
                    float_data("PulseSpacing", 0.25),
                    float_data("DisruptionRadius", 12.0),
                    float_data("DisruptionTimeSec", duration),
                    float_data("DisruptionStartTime", 1.5),
                    data("protoobject", "Bomber", "disruption_bomber"),
                    float_data("BomberBombTime", 1.0),
                    float_data("BomberFlyinDistance", 100.0),
                    float_data("BomberFlyinHeight", 30.0),
                    float_data("BomberBombHeight", 10.0),
                    float_data("BomberSpeed", 50.0),
                    float_data("BomberFlyOutTime", 5.0),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn cryo_power() -> Power {
    Power {
        name: "TestCryo".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Cryo".to_owned()),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("protoobject", "CryoObject", "cryo_fx"),
                    float_data("CryoRadius", 5.0),
                    float_data("MinCryoFalloff", 0.25),
                    data("objecttype", "FilterType", "CanCryo"),
                    float_data("TickDuration", 0.05),
                    data("int", "NumTicks", "1"),
                    float_data("CryoAmountPerTick", 1.0),
                    float_data("EffectStartTime", 0.0),
                    float_data("MaxKillHp", 0.0),
                    float_data("FreezingThawTime", 1.0),
                    float_data("FrozenThawTime", 1.0),
                    data("protoobject", "Bomber", "cryo_bomber"),
                    float_data("BomberBombTime", 0.1),
                    float_data("BomberFlyinDistance", 10.0),
                    float_data("BomberFlyinHeight", 3.0),
                    float_data("BomberBombHeight", 1.0),
                    float_data("BomberSpeed", 5.0),
                    float_data("BomberFlyOutTime", 0.5),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn visual(name: &str, lifespan: Option<f32>, object_types: &[&str]) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        lifespan,
        object_types: object_types
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        ..ProtoObject::default()
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
