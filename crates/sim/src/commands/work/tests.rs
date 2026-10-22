use super::{UserWorkTarget, WorkCommand};
use crate::EntityId;
use crate::command::{Command, CommandType, EntityType};
use crate::serialize::{deserialize_command, serialize_command};
use glam::Vec3;

#[test]
fn user_work_builds_the_retail_right_click_fields() {
    let squads = vec![
        EntityId::from_u32(0x2000_0001),
        EntityId::from_u32(0x2000_0002),
    ];
    let waypoint = Vec3::new(10.0, 2.0, 30.0);
    let target = UserWorkTarget {
        entity: EntityId::from_u32(0x1000_0005),
        hit_zone: 2,
    };

    let command = WorkCommand::user_work(1, squads.clone(), waypoint, Some(target), true);

    assert_eq!(command.base.player_id, 1);
    assert_eq!(command.base.id, -1);
    assert_eq!(command.base.command_type, CommandType::Work);
    assert_eq!(command.base.sender_type, EntityType::Player);
    assert_eq!(command.base.senders, vec![1]);
    assert_eq!(command.base.recipient_type, EntityType::Army);
    assert_eq!(command.base.recipients, squads);
    assert_eq!(command.base.waypoints, vec![waypoint]);
    assert!(command.base.has_flag(Command::ALTERNATE_FLAG));
    assert!(!command.base.has_flag(super::command_flags::ATTACK_MOVE));
    assert_eq!(command.unit_id, target.entity);
    assert_eq!(command.hit_zone_index, 2);
    assert_eq!(command.ability_id, -1);
    assert_eq!(command.squad_mode, -1);
    assert_eq!(command.terrain_point, None);
}

#[test]
fn ground_user_work_has_no_target_and_no_queue_flag() {
    let command = WorkCommand::user_work(2, Vec::new(), Vec3::ONE, None, false);

    assert!(command.unit_id.is_invalid());
    assert_eq!(command.hit_zone_index, -1);
    assert!(!command.base.has_flag(Command::ALTERNATE_FLAG));
}

#[test]
fn user_work_base_survives_the_wire_format() {
    let command = WorkCommand::user_work(
        1,
        vec![EntityId::from_u32(0x2000_0001)],
        Vec3::new(10.0, 2.0, 30.0),
        None,
        true,
    );
    let mut bytes = Vec::new();
    serialize_command(&command.base, &mut bytes).unwrap();

    let decoded = deserialize_command(&mut bytes.as_slice()).unwrap();

    assert_eq!(decoded.id, -1);
    assert_eq!(decoded.recipient_type, EntityType::Army);
    assert_eq!(decoded.recipients, command.base.recipients);
    assert_eq!(decoded.waypoints, command.base.waypoints);
    assert!(decoded.has_flag(Command::ALTERNATE_FLAG));
}
