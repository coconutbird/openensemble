//! A synthetic world with one barracks for the HUD tests.
//!
//! The barracks offers a technology, a squad and a standalone unit. A second technology turns
//! the squad command off with a `CommandEnable` effect.

use glam::Vec3;
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::{
    ObjectCommand, PopulationAmount, ResourceCost, TrainLimit, TrainLimitType,
};
use pipeline::database::hw1::squads::{Cost as SquadCost, UnitEntry, UnitsWrapper};
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper, TechCost, TechEffect};
use pipeline::database::hw1::{Database, GameData, ProtoObject, Squad as ProtoSquad, Tech};
use sim::{EntityId, World};

pub(super) const BARRACKS: &str = "test_barracks";
pub(super) const MARINE: &str = "test_marine";
pub(super) const UPGRADE: &str = "test_upgrade";
pub(super) const DISABLE_MARINES: &str = "disable_test_marines";
pub(super) const PLAYER: sim::PlayerId = 1;
/// The string table ID of the marine squad display name.
pub(super) const MARINE_NAME_ID: i32 = 7001;
/// The string table ID of the upgrade display name.
pub(super) const UPGRADE_NAME_ID: i32 = 7002;
const DRONE: &str = "test_drone";

/// Returns a world with a barracks of [`PLAYER`], one population slot and one resource.
pub(super) fn menu_world(database: &Database, cap: f32, supplies: f32) -> (World, EntityId) {
    let mut world = World::new();
    world.init_players(2);
    let player = world.get_player_mut(PLAYER).unwrap();
    player.configure_population_slots(1);
    assert!(player.set_population_limits(0, cap, cap));
    player.resources.set(0, supplies);
    let barracks_proto_id = sim::object_prototype_id(database, BARRACKS).unwrap();
    let barracks_id = sim::spawn_object_at(
        &mut world,
        database,
        PLAYER,
        barracks_proto_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .unwrap();
    (world, barracks_id)
}

/// Returns the database of [`menu_world`]. A train limit allows one marine squad.
pub(super) fn menu_database(with_train_limit: bool) -> Database {
    Database {
        objects: menu_objects(with_train_limit),
        squads: vec![marine_squad()],
        techs: menu_techs(),
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    deductable: Some(true),
                }],
            }),
            pops: Some(PopsWrapper {
                entries: vec!["Unit".to_owned()],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn menu_objects(with_train_limit: bool) -> Vec<ProtoObject> {
    let train_limits = if with_train_limit {
        vec![TrainLimit {
            target: MARINE.to_owned(),
            limit_type: Some(TrainLimitType::Squad),
            count: Some(1),
            bucket: None,
        }]
    } else {
        Vec::new()
    };
    vec![
        ProtoObject {
            name: BARRACKS.to_owned(),
            dbid: Some(100),
            object_class: Some("Building".to_owned()),
            obstruction_radius_x: Some(4.0),
            obstruction_radius_z: Some(5.0),
            commands: vec![
                command(DRONE, Some("TrainUnit"), None),
                command(MARINE, None, Some(1)),
                command(UPGRADE, Some("Research"), Some(0)),
                command("test_other", Some("Build"), Some(2)),
            ],
            train_limits,
            ..ProtoObject::default()
        },
        ProtoObject {
            name: MARINE.to_owned(),
            dbid: Some(101),
            object_class: Some("Unit".to_owned()),
            population: vec![population(0.25)],
            ..ProtoObject::default()
        },
        ProtoObject {
            name: DRONE.to_owned(),
            dbid: Some(102),
            object_class: Some("Unit".to_owned()),
            build_points: Some(0.05),
            costs: vec![ResourceCost {
                resource_type: "Supplies".to_owned(),
                amount: 25.0,
            }],
            population: vec![population(1.0)],
            ..ProtoObject::default()
        },
    ]
}

fn command(target: &str, command_type: Option<&str>, position: Option<i32>) -> ObjectCommand {
    ObjectCommand {
        target: target.to_owned(),
        command_type: command_type.map(str::to_owned),
        position,
        ..ObjectCommand::default()
    }
}

fn marine_squad() -> ProtoSquad {
    ProtoSquad {
        name: MARINE.to_owned(),
        dbid: Some(200),
        display_name_id: Some(MARINE_NAME_ID),
        build_points: Some(0.1),
        costs: vec![SquadCost {
            resource_type: "Supplies".to_owned(),
            amount: 100.0,
        }],
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: MARINE.to_owned(),
                count: 4,
                role: None,
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn menu_techs() -> Vec<Tech> {
    vec![
        Tech {
            name: UPGRADE.to_owned(),
            display_name_id: Some(UPGRADE_NAME_ID),
            research_points: Some(0.05),
            status: Some("OBTAINABLE".to_owned()),
            costs: vec![TechCost {
                resource_type: "Supplies".to_owned(),
                amount: 50.0,
            }],
            ..Tech::default()
        },
        Tech {
            name: DISABLE_MARINES.to_owned(),
            research_points: Some(0.0),
            effects: Some(EffectsWrapper {
                entries: vec![TechEffect {
                    effect_type: "Data".to_owned(),
                    amount: Some(0.0),
                    subtype: Some("CommandEnable".to_owned()),
                    relativity: Some("Absolute".to_owned()),
                    command_data: Some(MARINE.to_owned()),
                    command_type: Some("TrainSquad".to_owned()),
                    target: Some(EffectTarget {
                        target_type: Some("ProtoUnit".to_owned()),
                        value: Some(BARRACKS.to_owned()),
                    }),
                    ..TechEffect::default()
                }],
            }),
            ..Tech::default()
        },
    ]
}

fn population(amount: f32) -> PopulationAmount {
    PopulationAmount {
        population_type: Some("Unit".to_owned()),
        amount,
    }
}
