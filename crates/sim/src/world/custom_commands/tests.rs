use super::*;
use crate::entities::{Unit, UnitKind};
use crate::entity_id::EntityClass;

fn world_with_building() -> (World, EntityId) {
    let mut world = World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().resources.set(0, 100.0);
    let id = world.units.allocate_id();
    let mut unit = Unit::new(id, 1);
    unit.kind = UnitKind::Building;
    unit.built = true;
    world.units.insert(id, unit);
    (world, id)
}

#[test]
fn timed_persistent_custom_commands_are_authoritative_and_repeatable() {
    let (mut world, unit_id) = world_with_building();
    let id = world.add_custom_command(CustomCommand {
        unit_id,
        cost: Resources {
            amounts: [10.0, 0.0, 0.0, 0.0],
        },
        timer_seconds: 0.25,
        limit: 2,
        flags: CustomCommandFlags::default()
            .with_queue(true)
            .with_persistent(true)
            .with_allow_multiple(true)
            .with_allow_cancel(true),
        ..CustomCommand::default()
    });

    assert!(world.queue_custom_command(1, unit_id, id));
    assert!(world.queue_custom_command(1, unit_id, id));
    assert!(!world.queue_custom_command(1, unit_id, id));
    assert_close(world.get_player(1).unwrap().resources.get(0), 80.0);
    assert_eq!(world.update_custom_commands(0.25), 1);
    assert_eq!(world.custom_command(id).unwrap().finished_count, 1);
    assert_eq!(world.update_custom_commands(0.25), 1);
    assert_eq!(world.custom_command(id).unwrap().finished_count, 2);
}

#[test]
fn nonpersistent_instant_completion_leaves_a_valid_consumed_id() {
    let (mut world, unit_id) = world_with_building();
    let id = world.add_custom_command(CustomCommand {
        unit_id,
        ..CustomCommand::default()
    });

    assert!(world.queue_custom_command(1, unit_id, id));
    assert!(world.custom_command(id).is_none());
    assert_eq!(world.next_custom_command_id(), id + 1);
}

#[test]
fn removing_a_queued_command_refunds_payment() {
    let (mut world, unit_id) = world_with_building();
    let id = world.add_custom_command(CustomCommand {
        unit_id,
        cost: Resources {
            amounts: [25.0, 0.0, 0.0, 0.0],
        },
        timer_seconds: 1.0,
        flags: CustomCommandFlags::default()
            .with_queue(true)
            .with_persistent(true),
        ..CustomCommand::default()
    });

    assert!(world.queue_custom_command(1, unit_id, id));
    assert_close(world.get_player(1).unwrap().resources.get(0), 75.0);
    assert!(world.remove_custom_command(id).is_some());
    assert_close(world.get_player(1).unwrap().resources.get(0), 100.0);
}

#[test]
fn invalid_unit_does_not_queue() {
    let (mut world, _) = world_with_building();
    let invalid = EntityId::new(EntityClass::Unit, 99);
    let id = world.add_custom_command(CustomCommand {
        unit_id: invalid,
        ..CustomCommand::default()
    });
    assert!(!world.queue_custom_command(1, invalid, id));
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON);
}
