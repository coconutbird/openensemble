//! Squad Work orders Unpack, `PointBlankAttack` and `PlayBlockingAnimation`, and the update of
//! the squad work actions.
//!
//! `BSquad_executeOrder` @ `0x140399780` gives these orders their actions:
//!
//! - Order 20 Unpack (`BSquad_doUnpack` @ `0x1403A2B90`): squad action 74 (`BSquadActionWork`)
//!   with unit opportunity 19 and target range 0. When the squad reaches the target,
//!   `BSquadActionWork::unpack` creates the first object of the order ability at the target
//!   for the co-op player of the owner (else the owner) and kills the squad.
//! - Order 30 `PointBlankAttack` (`BSquad_doPointBlankAttack` @ `0x1403A2990`): a
//!   `PointBlankAttack` unit opportunity (type 33, priority 5 command) for each member.
//! - Order 17 `PlayBlockingAnimation` (`BSquad_doPlayBlockingAnimation` @ `0x14039B880`): a
//!   looping Misc-state animation of the order animation type (`BSimOrder+88`).

use super::World;
use crate::entities::SquadWorkAction;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::resolve_database_ability;
use crate::math::sin_cos;
use crate::player::PlayerId;
use crate::scenario::create_object_from_prototype;
use glam::Vec3;
use pipeline::database::hw1::Database;

/// The squad move arrival threshold (`Squad` movement): an unpacking squad at most this far
/// from its target (on the ground plane) has reached it.
const UNPACK_ARRIVAL_DISTANCE: f32 = 0.5;

impl World {
    /// Start Work order 20 (Unpack): move to `target`, then unpack there.
    ///
    /// Returns `false` when the command does not reach the squad, the order has no target
    /// position, or the squad does not accept the move.
    pub fn issue_unpack_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        target: Option<Vec3>,
        ability_id: Option<u8>,
        angle: f32,
    ) -> bool {
        let Some(target) = target.filter(|position| position.is_finite()) else {
            return false;
        };
        if !self.issue_squad_move_order_to_position(player_id, squad_id, target, false, false) {
            return false;
        }
        self.set_squad_work_action(
            squad_id,
            SquadWorkAction::Unpack {
                target,
                ability_id,
                angle: if angle.is_finite() { angle } else { 0.0 },
            },
        )
    }

    /// Start Work order 30 (`PointBlankAttack`) on a squad.
    ///
    /// The order records the command-priority opportunity for the members. The unit
    /// `PointBlankAttack` action is not modelled yet, so no member attacks.
    pub fn issue_point_blank_attack_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        target_id: Option<EntityId>,
        target_position: Option<Vec3>,
    ) -> bool {
        if !self.work_order_reaches(player_id, squad_id) {
            return false;
        }
        self.set_squad_work_action(
            squad_id,
            SquadWorkAction::PointBlankAttack {
                target_id,
                target_position: target_position.filter(|position| position.is_finite()),
            },
        )
    }

    /// Start Work order 17 (`PlayBlockingAnimation`) on a squad.
    ///
    /// The order records the blocking animation request. The squad blocking-animation action is
    /// not modelled yet, so no animation plays.
    pub fn issue_play_blocking_animation_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        animation_type: u8,
    ) -> bool {
        if !self.work_order_reaches(player_id, squad_id) {
            return false;
        }
        self.set_squad_work_action(
            squad_id,
            SquadWorkAction::PlayBlockingAnimation { animation_type },
        )
    }

    fn work_order_reaches(&self, player_id: PlayerId, squad_id: EntityId) -> bool {
        self.squads.get(squad_id).is_some_and(|squad| {
            Self::command_reaches_squad_owner(player_id, squad.base.player_id)
                && squad.is_alive()
                && !squad.garrison.is_garrisoned()
                && !self.is_squad_incapacitated(squad_id)
        })
    }

    fn set_squad_work_action(&mut self, squad_id: EntityId, action: SquadWorkAction) -> bool {
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        squad.work_action = Some(action);
        true
    }

    /// Advance the Repair and Unpack work actions of all squads.
    pub(super) fn update_squad_work_actions(&mut self, dt: f32, database: &Database) {
        let active = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                matches!(
                    squad.work_action,
                    Some(SquadWorkAction::Repair(_) | SquadWorkAction::Unpack { .. })
                )
                .then_some(squad_id)
            })
            .collect::<Vec<_>>();
        for squad_id in active {
            let Some(action) = self
                .squads
                .get_mut(squad_id)
                .and_then(|squad| squad.work_action.take())
            else {
                continue;
            };
            let next = match action {
                SquadWorkAction::Repair(repair) => self
                    .update_squad_repair(squad_id, repair, dt, database)
                    .map(SquadWorkAction::Repair),
                SquadWorkAction::Unpack {
                    target,
                    ability_id,
                    angle,
                } => self.update_unpack(squad_id, target, ability_id, angle, database),
                other => Some(other),
            };
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.work_action = next;
            }
        }
    }

    /// Wait for the squad to reach the target, then unpack. A squad that stops elsewhere (its
    /// move failed or another order moved it) ends the action.
    fn update_unpack(
        &mut self,
        squad_id: EntityId,
        target: Vec3,
        ability_id: Option<u8>,
        angle: f32,
        database: &Database,
    ) -> Option<SquadWorkAction> {
        let squad = self.squads.get(squad_id).filter(|squad| squad.is_alive())?;
        let offset = squad.base.position - target;
        let arrived = offset.x * offset.x + offset.z * offset.z
            <= UNPACK_ARRIVAL_DISTANCE * UNPACK_ARRIVAL_DISTANCE;
        if squad.move_target.is_some() && !arrived {
            return Some(SquadWorkAction::Unpack {
                target,
                ability_id,
                angle,
            });
        }
        if arrived {
            self.unpack_squad(squad_id, target, ability_id, angle, database);
        }
        None
    }

    /// `BSquadActionWork::unpack`: create the first ability object at the target and kill the
    /// squad. The placement check (`BWorld_checkPlacement`) is not modelled.
    fn unpack_squad(
        &mut self,
        squad_id: EntityId,
        target: Vec3,
        ability_id: Option<u8>,
        angle: f32,
        database: &Database,
    ) {
        let Some(ability_id) = ability_id else {
            return;
        };
        let Some((owner_id, proto_object_name)) = self.squads.get(squad_id).and_then(|squad| {
            let leader = squad
                .unit_ids
                .iter()
                .find_map(|unit_id| self.units.get(*unit_id))?;
            Some((squad.base.player_id, leader.proto_object_name.clone()))
        }) else {
            return;
        };
        let Some(object_name) = resolve_database_ability(database, &proto_object_name, ability_id)
            .and_then(|(_, ability)| ability.objects.first())
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
        else {
            return;
        };
        let player_id = self
            .get_player(owner_id)
            .and_then(crate::player::Player::coop_player_id)
            .unwrap_or(owner_id);
        let (sin, cos) = sin_cos(angle.to_radians());
        let forward = Vec3::new(sin, 0.0, cos);
        if create_object_from_prototype(self, player_id, target, forward, &object_name, database)
            .is_some()
        {
            let _killed = self.kill_squad(squad_id, true);
        }
    }
}
