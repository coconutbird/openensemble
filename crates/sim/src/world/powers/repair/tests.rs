use super::*;
use crate::player::{PowerGrant, TeamRelation};
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use crate::world::NativePowerInvocation;
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{GameData, Power, ProtoObject, Squad as ProtoSquad};

#[test]
fn cast_pays_and_owns_field_healing_reinforcement_and_attachment_lifetime() {
    let database = database();
    let mut world = test_world(&database, 1);
    world.get_player_mut(1).unwrap().set_resource(0, 1_000.0);
    assert!(world.grant_player_power(1, &database, power_grant(0)));
    let squad_id = spawn(&mut world, &database, 1, "repair_squad", Vec3::ZERO);
    let original = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert!(world.damage_unit_direct(original[0], 50.0, 0.0));
    world.remove_unit(original[1]).unwrap();

    let execution_id = world
        .invoke_native_power(
            &database,
            NativePowerInvocation {
                player_id: 1,
                proto_power_id: 0,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: false,
                power_user_id: crate::commands::PowerUserId::INVALID,
            },
        )
        .unwrap();
    let execution = &world.active_repair_powers()[0];
    assert_eq!(execution.id(), execution_id);
    assert_eq!(execution.radius().to_bits(), 10.0_f32.to_bits());
    assert_eq!(execution.repair_object_prototype(), "repair_field");
    assert_eq!(
        execution.repair_attachment_prototype(),
        Some("repair_attachment")
    );
    assert_eq!(
        world.get_player(1).unwrap().get_resource(0).to_bits(),
        900.0_f32.to_bits()
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
    let field_id = execution.repair_object_id();
    assert_eq!(
        world.get_object(field_id).unwrap().proto_object_name,
        "repair_field"
    );

    advance(&mut world, &database, 100, 0.1);

    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.unit_ids.len(), 2);
    assert_eq!(squad.repair_regen_source_count(), 1);
    assert_eq!(
        world.active_repair_powers()[0].repairing_squads(),
        &[squad_id]
    );
    let hitpoints = squad
        .unit_ids
        .iter()
        .map(|unit_id| world.get_unit(*unit_id).unwrap().hitpoints)
        .collect::<Vec<_>>();
    assert_eq!(hitpoints, vec![100.0, 50.0]);
    let attachment_id = attachment_id(&world).expect("repair attachment");
    assert_eq!(
        world
            .entity_object_state(attachment_id)
            .unwrap()
            .attached_to(),
        Some(squad.unit_ids[0])
    );

    advance(&mut world, &database, 200, 0.1);
    assert!(world.active_repair_powers().is_empty());
    assert!(world.get_object(field_id).is_none());
    assert!(world.get_object(attachment_id).is_none());
    assert_eq!(
        world
            .get_squad(squad_id)
            .unwrap()
            .repair_regen_source_count(),
        0
    );
}

#[test]
fn recent_damage_refreshes_cooldown_until_a_scheduled_tick_reaches_expiration() {
    let mut database = database();
    set_repair_ticks(&mut database, 10);
    let mut world = test_world(&database, 1);
    let squad_id = spawn(&mut world, &database, 1, "repair_squad", Vec3::ZERO);
    let unit_id = world.get_squad(squad_id).unwrap().unit_ids[0];
    world
        .invoke_repair_power(&database, repair_invocation())
        .unwrap();
    world.game_time_ms = 50;
    assert!(world.damage_unit(unit_id, 50.0));

    advance(&mut world, &database, 100, 0.05);
    assert_eq!(
        world.get_unit(unit_id).unwrap().hitpoints.to_bits(),
        50.0_f32.to_bits()
    );
    assert!(
        world.active_repair_powers()[0]
            .repairing_squads()
            .is_empty()
    );
    advance(&mut world, &database, 599, 0.05);
    assert_eq!(
        world.get_unit(unit_id).unwrap().hitpoints.to_bits(),
        50.0_f32.to_bits()
    );

    advance(&mut world, &database, 600, 0.05);
    assert!(world.get_unit(unit_id).unwrap().hitpoints > 50.0);
    assert_eq!(
        world.active_repair_powers()[0].repairing_squads(),
        &[squad_id]
    );
}

#[test]
fn ally_relation_and_leader_object_type_filter_the_tick_query() {
    let database = database();
    let mut world = test_world(&database, 3);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 1;
    world.get_player_mut(3).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    assert!(world.set_mutual_team_relation(1, 2, TeamRelation::Enemy));
    let own = spawn(&mut world, &database, 1, "repair_squad", Vec3::ZERO);
    let ally = spawn(&mut world, &database, 2, "repair_squad", Vec3::X);
    let enemy = spawn(&mut world, &database, 3, "repair_squad", Vec3::X * 2.0);
    let civilian = spawn(&mut world, &database, 1, "civilian_squad", Vec3::X * 3.0);
    let units =
        [own, ally, enemy, civilian].map(|squad_id| world.get_squad(squad_id).unwrap().unit_ids[0]);
    for unit_id in units {
        assert!(world.damage_unit_direct(unit_id, 50.0, 0.0));
    }

    world
        .invoke_repair_power(&database, repair_invocation())
        .unwrap();
    advance(&mut world, &database, 100, 0.1);

    assert_eq!(
        world.get_unit(units[0]).unwrap().hitpoints.to_bits(),
        100.0_f32.to_bits()
    );
    assert_eq!(
        world.get_unit(units[1]).unwrap().hitpoints.to_bits(),
        100.0_f32.to_bits()
    );
    assert_eq!(
        world.get_unit(units[2]).unwrap().hitpoints.to_bits(),
        50.0_f32.to_bits()
    );
    assert_eq!(
        world.get_unit(units[3]).unwrap().hitpoints.to_bits(),
        50.0_f32.to_bits()
    );
}

#[test]
fn disruption_stops_an_existing_repair_only_when_its_next_tick_is_due() {
    let mut database = database();
    set_repair_ticks(&mut database, 10);
    let mut world = test_world(&database, 1);
    let squad_id = spawn(&mut world, &database, 1, "repair_squad", Vec3::ZERO);
    let unit_id = world.get_squad(squad_id).unwrap().unit_ids[0];
    assert!(world.damage_unit_direct(unit_id, 100.0, 0.0));
    world
        .invoke_repair_power(&database, repair_invocation())
        .unwrap();
    advance(&mut world, &database, 100, 0.1);
    let repair_field_id = world.active_repair_powers()[0].repair_object_id();
    let repair_attachment_id = attachment_id(&world).unwrap();

    world
        .invoke_disruption_power(
            &database,
            super::super::DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: 1,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .unwrap();
    world.game_time_ms = 150;
    world.update_entities_with_database(0.05, &database);
    assert_eq!(world.active_repair_powers().len(), 1);

    advance(&mut world, &database, 200, 0.05);
    assert!(world.active_disruption_powers()[0].is_active());
    assert!(world.active_repair_powers().is_empty());
    assert!(world.get_object(repair_field_id).is_none());
    assert!(world.get_object(repair_attachment_id).is_none());
    assert_eq!(
        world
            .get_squad(squad_id)
            .unwrap()
            .repair_regen_source_count(),
        0
    );
}

#[test]
fn no_cost_and_ignore_placement_do_not_bypass_active_disruption() {
    let database = database();
    let mut world = test_world(&database, 1);
    world
        .invoke_disruption_power(
            &database,
            super::super::DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: 1,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .unwrap();
    advance(&mut world, &database, 100, 0.1);
    let disruption_id = world.active_disruption_powers()[0].id();

    assert_eq!(
        world.invoke_repair_power(&database, repair_invocation()),
        Err(RepairPowerError::Disrupted(disruption_id))
    );
    assert!(world.active_repair_powers().is_empty());
}

fn test_world(database: &Database, players: u8) -> World {
    let mut world = World::with_seed(7);
    world.init_players(players);
    world.configure_prototype_catalogs(database);
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

fn attachment_id(world: &World) -> Option<EntityId> {
    world.objects.iter().find_map(|(id, object)| {
        object
            .proto_object_name
            .eq_ignore_ascii_case("repair_attachment")
            .then_some(id)
    })
}

fn repair_invocation() -> RepairPowerInvocation {
    RepairPowerInvocation {
        player_id: 1,
        proto_power_id: 0,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location: Vec3::ZERO,
        ignore_requirements: true,
    }
}

fn power_grant(proto_power_id: i32) -> PowerGrant {
    PowerGrant {
        proto_power_id,
        squad_id: EntityId::INVALID,
        uses: 1,
        icon_location: -1,
        ignore_cost: false,
        ignore_tech_prerequisites: false,
        ignore_population: false,
    }
}

fn set_repair_ticks(database: &mut Database, ticks: i32) {
    let entry = database.powers[0].attributes.as_mut().unwrap().data_levels[0]
        .entries
        .iter_mut()
        .find(|entry| entry.name == "NumTicks")
        .unwrap();
    entry.value = ticks.to_string();
}

fn database() -> Database {
    Database {
        objects: vec![
            object("repair_field", 1, 0.0, 0.0, &[]),
            object("repair_attachment", 2, 0.0, 0.0, &[]),
            object("repair_unit", 3, 100.0, 10.0, &["Military"]),
            object("civilian", 4, 100.0, 10.0, &["Civilian"]),
            object("disruption_field", 5, 0.0, 0.0, &[]),
            object("disruption_pulse", 6, 0.0, 0.0, &[]),
            object("disruption_strike", 7, 0.0, 0.0, &[]),
            object("disruption_bomber", 8, 0.0, 0.0, &[]),
        ],
        squads: vec![
            squad("repair_squad", 20, "repair_unit", 2),
            squad("civilian_squad", 21, "civilian", 1),
        ],
        powers: vec![repair_power(), disruption_power()],
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

fn repair_power() -> Power {
    Power {
        name: "TestRepair".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Repair".to_owned()),
            cost: Some(PowerCost {
                supplies: Some(100.0),
                ..PowerCost::default()
            }),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("protoobject", "RepairObject", "repair_field"),
                    data("protoobject", "RepairAttachment", "repair_attachment"),
                    float_data("RepairRadius", 10.0),
                    data("objecttype", "FilterType", "Military"),
                    float_data("TickDuration", 0.1),
                    data("int", "NumTicks", "2"),
                    float_data("RepairCombatValuePerTick", 10.0),
                    data("bool", "SpreadAmongSquads", "false"),
                    data("bool", "AllowReinforce", "true"),
                    float_data("CooldownTimeIfDamaged", 0.5),
                    data("bool", "IgnorePlacement", "true"),
                    data("bool", "HealAny", "false"),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
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

fn object(
    name: &str,
    dbid: i32,
    hitpoints: f32,
    combat_value: f32,
    object_types: &[&str],
) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        hitpoints: (hitpoints > 0.0).then_some(hitpoints),
        combat_value: (combat_value > 0.0).then_some(combat_value),
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
