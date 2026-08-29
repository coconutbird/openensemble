use super::*;
use crate::entities::SquadCryoState;
use crate::entity::Entity;
use crate::player::PowerGrant;
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::{GameData, Power, ProtoObject, Squad as ProtoSquad};

#[test]
fn cast_pays_once_and_retail_ticks_freeze_only_one_nearest_new_squad_per_update() {
    let database = database(3, 10_000.0, false);
    let mut world = World::with_seed(7);
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
    let nearest = add_target(&mut world, Vec3::new(2.0, 0.0, 0.0), 100.0, false);
    let farther = add_target(&mut world, Vec3::new(4.0, 0.0, 0.0), 100.0, false);
    world.game_time_ms = 1_000;
    let used_power = world.subscribe_general_event(GeneralEventType::UsedPower, Some(1), false);

    let execution_id = world
        .invoke_cryo_power(
            &database,
            CryoPowerInvocation {
                player_id: 1,
                proto_power_id: 0,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: false,
            },
        )
        .unwrap();

    let execution = &world.active_cryo_powers()[0];
    assert_eq!(execution.id(), execution_id);
    assert_eq!(execution.next_tick_time_ms(), 1_050);
    assert_eq!(
        execution.cryo_amount_per_tick().to_bits(),
        40.0_f32.to_bits()
    );
    assert_eq!(execution.ticks_remaining(), 3);
    assert_eq!(
        world.get_player(1).unwrap().get_resource(0).to_bits(),
        400.0_f32.to_bits()
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
    let bomber = world.get_object(bomber_id).unwrap();
    assert_eq!(bomber.proto_object_name, "cryo_bomber");
    assert_eq!(bomber.base.position, execution.bomber_position());

    world.game_time_ms = 1_050;
    world.update_entities_with_database(0.05, &database);
    assert_eq!(
        world.get_squad(nearest).unwrap().cryo_state(),
        SquadCryoState::None
    );
    assert_eq!(
        world.get_squad(farther).unwrap().cryo_state(),
        SquadCryoState::None
    );
    assert_eq!(world.active_cryo_powers()[0].ticks_remaining(), 3);

    world.game_time_ms = 1_100;
    world.update_entities_with_database(0.05, &database);
    assert!(world.get_squad(nearest).unwrap().is_cryo_frozen());
    assert!(!world.get_squad(farther).unwrap().is_cryo_frozen());
    let execution = &world.active_cryo_powers()[0];
    assert_eq!(execution.ticks_remaining(), 2);
    assert!(execution.bomb_released());
    let cryo_object_id = execution.cryo_object_id();
    assert_eq!(
        world.get_object(cryo_object_id).unwrap().proto_object_name,
        "cryo_fx"
    );

    world.game_time_ms = 1_150;
    world.update_entities_with_database(0.05, &database);
    assert!(world.get_squad(farther).unwrap().is_cryo_frozen());
    assert!(world.get_squad(nearest).unwrap().cryo_thaw_delay() > 11.8);

    world.game_time_ms = 1_200;
    world.update_entities_with_database(0.05, &database);
    assert!(world.active_cryo_powers().is_empty());
    assert!(world.get_object(bomber_id).is_none());
    assert!(world.get_object(cryo_object_id).is_some());
    world.game_time_ms = 1_300;
    world.update_entities(0.05);
    assert!(world.get_object(cryo_object_id).is_none());
}

#[test]
fn frozen_kill_budget_is_strict_but_transporters_always_die() {
    let database = database(3, 100.0, true);
    let mut world = World::new();
    world.init_players(1);
    let equal_budget = add_target(&mut world, Vec3::new(1.0, 0.0, 0.0), 100.0, false);
    let transporter = add_target(&mut world, Vec3::new(2.0, 0.0, 0.0), 500.0, true);

    world
        .invoke_cryo_power(
            &database,
            CryoPowerInvocation {
                player_id: 1,
                proto_power_id: 0,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .unwrap();
    world.game_time_ms = 50;
    world.update_entities_with_database(0.05, &database);
    assert!(!world.get_squad(equal_budget).unwrap().is_cryo_frozen());

    world.game_time_ms = 100;
    world.update_entities_with_database(0.05, &database);
    assert!(world.get_squad(equal_budget).unwrap().is_cryo_frozen());
    assert!(world.get_squad(equal_budget).unwrap().is_alive());

    world.game_time_ms = 150;
    world.update_entities_with_database(0.05, &database);
    assert!(world.get_squad(transporter).is_none());
    assert_eq!(
        world.active_cryo_powers()[0]
            .killable_hitpoints_left()
            .to_bits(),
        (-400.0_f32).to_bits()
    );
}

#[test]
fn an_overdue_empty_cast_burns_every_scheduled_tick_in_one_update() {
    let database = database(3, 100.0, false);
    let mut world = World::new();
    world.init_players(1);
    world
        .invoke_cryo_power(
            &database,
            CryoPowerInvocation {
                player_id: 1,
                proto_power_id: 0,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .unwrap();

    world.game_time_ms = 1_000;
    world.update_entities_with_database(0.05, &database);
    assert!(!world.active_cryo_powers().is_empty());
    world.game_time_ms = 1_050;
    world.update_entities_with_database(0.05, &database);

    assert!(world.active_cryo_powers().is_empty());
}

fn add_target(world: &mut World, position: Vec3, hitpoints: f32, transporter: bool) -> EntityId {
    let squad_id = world.create_squad_at(1, position);
    world.get_squad_mut(squad_id).unwrap().proto_squad_name = "target_squad".to_owned();
    let unit_id = world.create_unit_at(1, position);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_name = "target_member".to_owned();
    unit.object_types = vec!["CanCryo".to_owned()];
    if transporter {
        unit.object_types.push("_Transporter".to_owned());
    }
    unit.set_max_hitpoints(hitpoints);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    squad_id
}

fn database(ticks: i32, max_kill_hitpoints: f32, dies_when_frozen: bool) -> Database {
    let mut flags = Vec::new();
    if dies_when_frozen {
        flags.push("DiesWhenFrozen".to_owned());
    }
    Database {
        objects: vec![
            ProtoObject {
                name: "cryo_fx".to_owned(),
                lifespan: Some(0.2),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "cryo_bomber".to_owned(),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "target_member".to_owned(),
                object_types: vec!["CanCryo".to_owned()],
                ..ProtoObject::default()
            },
        ],
        squads: vec![ProtoSquad {
            name: "target_squad".to_owned(),
            cryo_points: Some(100.0),
            flags,
            ..ProtoSquad::default()
        }],
        powers: vec![Power {
            name: "TestCryo".to_owned(),
            attributes: Some(PowerAttributes {
                power_type: Some("Cryo".to_owned()),
                auto_recharge: Some(1_000),
                cost: Some(PowerCost {
                    supplies: Some(600.0),
                    ..PowerCost::default()
                }),
                base_data_level: Some(base_data(ticks)),
                data_levels: vec![DataLevel {
                    level: Some(0),
                    entries: vec![
                        float_data("MaxKillHp", max_kill_hitpoints),
                        float_data("FrozenThawTime", 12.0),
                    ],
                }],
                ..PowerAttributes::default()
            }),
            ..Power::default()
        }],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    ..ResourceDef::default()
                }],
            }),
            pops: Some(PopsWrapper::default()),
            default_cryo_points: Some(50.0),
            default_thaw_speed: Some(10.0),
            time_freezing_to_thaw: Some(3.0),
            time_frozen_to_thaw: Some(7.0),
            freezing_speed_modifier: Some(0.5),
            freezing_damage_modifier: Some(1.25),
            frozen_damage_modifier: Some(2.0),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn base_data(ticks: i32) -> DataLevel {
    DataLevel {
        entries: vec![
            data("protoobject", "CryoObject", "cryo_fx"),
            float_data("CryoRadius", 45.0),
            float_data("MinCryoFalloff", 0.25),
            data("objecttype", "FilterType", "CanCryo"),
            float_data("TickDuration", 0.05),
            data("int", "NumTicks", &ticks.to_string()),
            float_data("CryoAmountPerTick", 40.0),
            float_data("EffectStartTime", 0.0),
            float_data("MaxKillHp", 1.0),
            float_data("FreezingThawTime", 5.0),
            float_data("FrozenThawTime", 7.0),
            data("protoobject", "Bomber", "cryo_bomber"),
            float_data("BomberBombTime", 0.1),
            float_data("BomberFlyinDistance", 100.0),
            float_data("BomberFlyinHeight", 30.0),
            float_data("BomberBombHeight", 10.0),
            float_data("BomberSpeed", 50.0),
            float_data("BomberFlyOutTime", 0.5),
        ],
        ..DataLevel::default()
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
