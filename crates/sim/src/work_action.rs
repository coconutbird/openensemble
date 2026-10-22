//! Default action that a squad's tactic picks for a Work target.
//!
//! Retail asks the same tactic lookup for the cursor hover
//! (`BUser_classifyHoverTarget`, `0x1405820C0`) and for the execution of a
//! right-click Work command (`BSquad_getDefaultOrderType`, `0x140399E30`).
//! Both call `BSquad_getTacticAction` (`0x14039C730`), which uses the first
//! squad unit as the source and calls `BTactic_getTacticAction`
//! (`0x14040B070`). That function walks the tactic target rules in authored
//! order and returns the first match. When no rule matches, it returns the
//! action that has `<Default>true</Default>` (tactic data `+96`, set by
//! `BProtoAction_loadProtoAction`).

use crate::entities::{SquadMode, Unit};
use crate::gameplay::{AttackQuery, AttackQueryFlags, GameplayCatalog, TacticRelation};
use crate::player::TeamRelation;
use crate::{EntityId, World};
use pipeline::database::hw1::tactics::Action;

/// Action that a squad's tactic picks for a Work target.
///
/// The cases are the actions that `BUser_classifyHoverTarget` reports. The
/// shipped tactics give no other action type to a target rule without an
/// ability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefaultWorkAction {
    /// Attack the target (`RangedAttack` and `HandAttack`, action type 6, and
    /// `SecondaryTurretAttack`, action type 29; order type 2).
    Attack,
    /// Gather the target resource (`Gather`, action type 25; order type 3).
    Gather,
    /// Capture the target (`Capture`, action type 17; order type 8).
    Capture,
    /// Enter the target container (`Garrison`, action type 12; order type 10).
    Garrison,
    /// Join the target squad (`Join`, action type 18; order type 9).
    Join,
    /// Hitch to the target (`Hitch`, action type 32; order type 21).
    Hitch,
    /// Repair the target (`RepairOther`, action type 66; order type 5).
    RepairOther,
}

impl DefaultWorkAction {
    /// Map an authored `ActionType` the way `BSquad_getDefaultOrderType` maps
    /// the proto-action type to an order type.
    ///
    /// Returns `None` for `Move`, `GaggleMove` and the action types that need
    /// an ability. A Work command then moves the squad to the waypoint.
    #[must_use]
    pub fn from_action_type(action_type: &str) -> Option<Self> {
        const ACTIONS: [(&str, DefaultWorkAction); 9] = [
            ("RangedAttack", DefaultWorkAction::Attack),
            ("HandAttack", DefaultWorkAction::Attack),
            ("SecondaryTurretAttack", DefaultWorkAction::Attack),
            ("Gather", DefaultWorkAction::Gather),
            ("Capture", DefaultWorkAction::Capture),
            ("Garrison", DefaultWorkAction::Garrison),
            ("Join", DefaultWorkAction::Join),
            ("Hitch", DefaultWorkAction::Hitch),
            ("RepairOther", DefaultWorkAction::RepairOther),
        ];
        ACTIONS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(action_type))
            .map(|&(_, action)| action)
    }
}

/// Return the action that the squad's tactic picks for a Work target.
///
/// `target_id` is a unit or a squad (its leader unit). The source is the
/// first operational unit of the squad. Returns `None` when the squad, its
/// source unit or the target is missing, when the tactic has no action for
/// the target, or when the picked action is not a [`DefaultWorkAction`].
#[must_use]
pub fn default_work_action(
    world: &World,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
    target_id: EntityId,
) -> Option<DefaultWorkAction> {
    let (target_unit_id, _) = world.squad_move_entity_target(target_id)?;
    let (source, target) = work_units(world, squad_id, target_unit_id)?;
    let query = work_query(world, squad_id, target_id, source, target, None);
    let action = gameplay
        .select_work_action(&source.proto_object_name, &query, |action| {
            work_action_enabled(world, source, action)
        })
        .or_else(|| tactic_default_action(gameplay, &source.proto_object_name))?;
    DefaultWorkAction::from_action_type(action.action_type.as_deref()?)
}

/// `BTactic_getTacticAction` falls back to the `<Default>` action of the
/// tactic when no target rule matches.
fn tactic_default_action<'catalog>(
    gameplay: &'catalog GameplayCatalog,
    proto_object_name: &str,
) -> Option<&'catalog Action> {
    gameplay
        .object(proto_object_name)?
        .tactics()
        .actions
        .iter()
        .find(|action| action.default == Some(true))
}

/// Return the source unit of a squad and the target unit of a Work order.
pub(crate) fn work_units(
    world: &World,
    squad_id: EntityId,
    target_unit_id: EntityId,
) -> Option<(&Unit, &Unit)> {
    let squad = world.get_squad(squad_id)?;
    let source = squad.unit_ids.iter().find_map(|unit_id| {
        world
            .get_unit(*unit_id)
            .filter(|unit| unit.is_operational())
    })?;
    let target = world.get_unit(target_unit_id)?;
    Some((source, target))
}

/// Build the tactic-rule query for one squad and Work target.
pub(crate) fn work_query<'target>(
    world: &World,
    squad_id: EntityId,
    target_id: EntityId,
    source: &Unit,
    target: &'target Unit,
    ability_id: Option<u8>,
) -> AttackQuery<'target> {
    let mut flags = AttackQueryFlags::empty();
    if target.base.player_id == 0 {
        flags.insert(AttackQueryFlags::TARGET_GAIA);
    }
    if target.hitpoints < target.max_hitpoints
        || world
            .repair_other_target_squad_id(target_id)
            .and_then(|target_squad_id| world.get_squad(target_squad_id))
            .is_some_and(|squad| {
                squad.unit_ids.iter().any(|unit_id| {
                    world
                        .get_unit(*unit_id)
                        .is_some_and(|unit| unit.hitpoints < unit.max_hitpoints)
                })
            })
    {
        flags.insert(AttackQueryFlags::TARGET_DAMAGED);
    }
    if target.is_building() && !target.built {
        flags.insert(AttackQueryFlags::TARGET_UNBUILT);
    }
    if world.can_squad_capture_target(source.base.player_id, squad_id, target_id) {
        flags.insert(AttackQueryFlags::TARGET_CAPTURABLE);
    }
    AttackQuery {
        relation: work_relation(world, source.base.player_id, target.base.player_id),
        squad_mode: world
            .get_squad(squad_id)
            .map_or(SquadMode::Normal, |squad| squad.mode),
        ability_id,
        target_proto_object_name: Some(&target.proto_object_name),
        tactic_state: source.tactic_state(),
        flags,
    }
}

/// Return the tactic relation from the source player to the target player.
pub(crate) fn work_relation(world: &World, source: u8, target: u8) -> TacticRelation {
    if source == target {
        return TacticRelation::SelfPlayer;
    }
    match world.player_relation(source, target) {
        Some(TeamRelation::Ally) => TacticRelation::Ally,
        Some(TeamRelation::Enemy) => TacticRelation::Enemy,
        Some(TeamRelation::Neutral) | None => TacticRelation::Neutral,
    }
}

/// Return whether the authored, technology and runtime state enable an action.
pub(crate) fn work_action_enabled(world: &World, source: &Unit, action: &Action) -> bool {
    let authored_enabled = action.start_disabled != Some(true);
    let player_enabled =
        world
            .get_player(source.base.player_id)
            .map_or(authored_enabled, |player| {
                player.technologies.action_enabled(
                    &source.proto_object_name,
                    &action.name,
                    authored_enabled,
                )
            });
    source.actions.is_enabled(&action.name, !player_enabled)
}

#[cfg(test)]
pub(crate) mod tests;
