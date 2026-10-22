//! The right-click order, like `BUser_doWorkAtCurrentLocation` (`0x14058C2E0`).
//!
//! The command is a retail Work command for an army (`sim::WorkCommand::user_work`). The
//! client does not model the command metering of `BWorkCommand_meter` (`0x140196200`) or the
//! move acknowledgment object that retail puts at the ground point.

use glam::Vec3;
use sim::{EntityId, PlayerId, UserWorkTarget, WorkCommand, World};

use super::hover::Hover;

/// The hit zone of a hover target. The client does not model hit zones (`user+496` stays -1).
const NO_HIT_ZONE: i32 = -1;

/// Returns the squads that receive a right-click order.
///
/// Retail drops a squad that is dead, that is not owned by the player, or that has engine
/// flags that the simulation does not model (`unit+1106 & 4`, `unit+759 & 0x20`,
/// `unit+1104 & 2`, `squad+741 & 0x80`).
pub(crate) fn work_recipients(
    world: &World,
    player: PlayerId,
    squads: &[EntityId],
) -> Vec<EntityId> {
    squads
        .iter()
        .copied()
        .filter(|&squad_id| {
            world
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.base.is_alive() && squad.base.player_id == player)
        })
        .collect()
}

/// Makes the right-click Work command, or `None` if no squad can take it or the cursor has no
/// ground point.
///
/// The waypoint is the cursor ground point. The hover entity is the target only when its hover
/// type is above 1; a target in a selected squad moves the waypoint to that squad. `queue` is
/// the Shift modifier (`DoWorkQueue`).
pub(crate) fn work_command(
    world: &World,
    player: PlayerId,
    squads: &[EntityId],
    hover: &Hover,
    queue: bool,
) -> Option<WorkCommand> {
    let recipients = work_recipients(world, player, squads);
    if recipients.is_empty() {
        return None;
    }
    let mut waypoint: Vec3 = hover.point?;
    let target = hover
        .entity
        .filter(|_| hover.kind.has_target())
        .map(|entity| UserWorkTarget {
            entity,
            hit_zone: NO_HIT_ZONE,
        });
    if let Some(target) = target {
        let target_squad = world
            .units
            .get(target.entity)
            .and_then(|unit| unit.squad_id)
            .filter(|squad| squads.contains(squad))
            .and_then(|squad| world.squads.get(squad));
        if let Some(squad) = target_squad {
            waypoint = squad.base.position;
        }
    }
    Some(WorkCommand::user_work(
        i32::from(player),
        recipients,
        waypoint,
        target,
        queue,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::hover::HoverType;

    fn world() -> (World, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        world.init_players(2);
        let own = world.create_squad_at(1, Vec3::ZERO);
        let unit = world.create_unit_at(1, Vec3::ZERO);
        assert!(world.attach_unit_to_squad(unit, own));
        let enemy = world.create_squad_at(2, Vec3::X);
        let enemy_unit = world.create_unit_at(2, Vec3::X);
        assert!(world.attach_unit_to_squad(enemy_unit, enemy));
        (world, own, enemy, enemy_unit)
    }

    #[test]
    fn ground_click_moves_own_squads_only() {
        let (world, own, enemy, _) = world();
        let hover = Hover {
            point: Some(Vec3::new(5.0, 1.0, 5.0)),
            ..Hover::default()
        };
        let command = work_command(&world, 1, &[own, enemy], &hover, true).unwrap();
        let expected = WorkCommand::user_work(1, vec![own], Vec3::new(5.0, 1.0, 5.0), None, true);
        assert_eq!(format!("{command:?}"), format!("{expected:?}"));
        assert!(work_command(&world, 1, &[enemy], &hover, false).is_none());
        assert!(work_command(&world, 1, &[own], &Hover::default(), false).is_none());
    }

    #[test]
    fn only_an_action_hover_sends_the_target() {
        let (world, own, _, enemy_unit) = world();
        let mut hover = Hover {
            point: Some(Vec3::X),
            entity: Some(enemy_unit),
            kind: HoverType::Attack,
            candidates: Vec::new(),
        };
        let command = work_command(&world, 1, &[own], &hover, false).unwrap();
        let target = UserWorkTarget {
            entity: enemy_unit,
            hit_zone: -1,
        };
        let expected = WorkCommand::user_work(1, vec![own], Vec3::X, Some(target), false);
        assert_eq!(format!("{command:?}"), format!("{expected:?}"));

        hover.kind = HoverType::Selectable;
        let command = work_command(&world, 1, &[own], &hover, false).unwrap();
        let expected = WorkCommand::user_work(1, vec![own], Vec3::X, None, false);
        assert_eq!(format!("{command:?}"), format!("{expected:?}"));
    }
}
