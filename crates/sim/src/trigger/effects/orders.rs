//! Retail scripted squad-order effects backed by authoritative sim state.

use super::support::{
    EntityListKind, bool_at, entity_list, unique_add, variable_is_used, vector_at,
};
use super::{EffectOutcome, value_at};
use crate::gameplay::{AttackQuery, AttackQueryFlags, GameplayCatalog, TacticRelation};
use crate::player::TeamRelation;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::{EntityId, World};
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

pub(super) fn unload(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let use_squad = variable_is_used(effect, script, 3);
    let use_squad_list = variable_is_used(effect, script, 5);
    if !use_squad && !use_squad_list {
        return EffectOutcome::Skipped;
    }
    let mut containers = if use_squad_list {
        list_at(effect, script, 5).cloned().unwrap_or_default()
    } else {
        Vec::new()
    };
    if use_squad && let Some(squad_id) = squad_at(effect, script, 3) {
        unique_add(&mut containers, squad_id);
    }
    let passenger_filter = if effect.version == 4 {
        let mut passengers = if variable_is_used(effect, script, 7) {
            list_at(effect, script, 7).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        if variable_is_used(effect, script, 6)
            && let Some(squad_id) = squad_at(effect, script, 6)
        {
            passengers.push(squad_id);
        }
        passengers
    } else {
        Vec::new()
    };
    for container_id in containers {
        let _accepted = world.trigger_unload_squad(container_id, &passenger_filter);
    }
    EffectOutcome::Applied
}

pub(super) fn move_squads(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if effect.version != 6 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    execute_location_order(
        effect,
        script,
        world,
        OrderSlots {
            squad: 1,
            squad_list: 5,
            target_unit: 6,
            target_location: 2,
            target_squad: 7,
            attack_move: 8,
            queue_order: 9,
        },
    )
}

pub(super) fn move_path(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if effect.version != 3 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let use_squad = variable_is_used(effect, script, 1);
    let use_squad_list = variable_is_used(effect, script, 3);
    if !use_squad && !use_squad_list {
        return EffectOutcome::Skipped;
    }
    let mut working_squads = if use_squad_list {
        list_at(effect, script, 3).cloned().unwrap_or_default()
    } else {
        Vec::new()
    };
    if use_squad && let Some(squad_id) = squad_at(effect, script, 1) {
        unique_add(&mut working_squads, squad_id);
    }
    if working_squads.is_empty() {
        return EffectOutcome::Skipped;
    }
    let Some(mut waypoints) = waypoint_list_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    if waypoints.is_empty() {
        return EffectOutcome::Skipped;
    }
    if bool_at(effect, script, 6).unwrap_or(false) {
        trim_path_to_closest(&mut waypoints, &working_squads, world);
    }
    issue_path_to_squads(
        world,
        &working_squads,
        &waypoints,
        bool_at(effect, script, 4).unwrap_or(false),
        bool_at(effect, script, 5).unwrap_or(false),
        bool_at(effect, script, 7).unwrap_or(false),
    )
}

pub(super) fn work(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
    gameplay: Option<&GameplayCatalog>,
) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let use_squad = variable_is_used(effect, script, 1);
    let use_squad_list = variable_is_used(effect, script, 2);
    if !use_squad && !use_squad_list {
        return EffectOutcome::Skipped;
    }
    let Some(target) = work_destination(effect, script, world) else {
        return EffectOutcome::Skipped;
    };
    let mut working_squads = if use_squad_list {
        list_at(effect, script, 2).cloned().unwrap_or_default()
    } else {
        Vec::new()
    };
    if use_squad
        && let Some(squad_id) = squad_at(effect, script, 1)
        && world.get_squad(squad_id).is_some()
    {
        unique_add(&mut working_squads, squad_id);
    }
    let mut unique_squads = Vec::new();
    for squad_id in working_squads {
        unique_add(&mut unique_squads, squad_id);
    }
    if unique_squads.is_empty() {
        return EffectOutcome::Skipped;
    }

    let attack_move = bool_at(effect, script, 6).unwrap_or(false);
    let queue_order = bool_at(effect, script, 7).unwrap_or(false);
    let do_ability = effect.version == 4 && bool_at(effect, script, 8).unwrap_or(false);
    let mut orders = Vec::new();
    for squad_id in unique_squads {
        let Some(player_id) = world.get_squad(squad_id).map(|squad| squad.base.player_id) else {
            continue;
        };
        let order = match target {
            MoveDestination::Position(position) => {
                resolve_contextual_location_work(world, squad_id, position, do_ability, gameplay)
            }
            MoveDestination::Entity(target_id) => {
                resolve_contextual_work(world, squad_id, target_id, do_ability, gameplay)
            }
        };
        if matches!(order, ContextualWorkOrder::Unsupported)
            || (queue_order && !matches!(order, ContextualWorkOrder::Move(_)))
        {
            return EffectOutcome::Unsupported(effect.raw_type);
        }
        orders.push((player_id, squad_id, order));
    }

    let assets = WorkAssets { database, gameplay };
    let mut issued = false;
    for (player_id, squad_id, order) in orders {
        issued |= issue_contextual_work(
            world,
            player_id,
            squad_id,
            order,
            attack_move,
            queue_order,
            assets,
        );
    }
    if issued {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

#[derive(Debug, Clone, Copy)]
enum ContextualWorkOrder {
    Move(Vec3),
    Attack {
        target: EntityId,
        ability_id: Option<u8>,
    },
    Garrison {
        target: EntityId,
        range: f32,
    },
    Join {
        target: EntityId,
        ability_id: Option<u8>,
    },
    Gather(EntityId),
    Capture(EntityId),
    RepairOther {
        target: EntityId,
        ability_id: Option<u8>,
    },
    Hitch(EntityId),
    Unhitch(EntityId),
    Mines {
        target: MoveDestination,
        ability_id: u8,
    },
    Unsupported,
}

#[derive(Debug, Clone, Copy)]
struct WorkAssets<'assets> {
    database: Option<&'assets Database>,
    gameplay: Option<&'assets GameplayCatalog>,
}

fn resolve_contextual_location_work(
    world: &World,
    squad_id: EntityId,
    target: Vec3,
    do_ability: bool,
    gameplay: Option<&GameplayCatalog>,
) -> ContextualWorkOrder {
    if !do_ability {
        return ContextualWorkOrder::Move(target);
    }
    let Some(gameplay) = gameplay else {
        return ContextualWorkOrder::Move(target);
    };
    let Some(ability_id) = gameplay.command_ability_id() else {
        return ContextualWorkOrder::Move(target);
    };
    let Some(squad) = world.get_squad(squad_id) else {
        return ContextualWorkOrder::Unsupported;
    };
    let Some(source) = squad.unit_ids.iter().find_map(|unit_id| {
        world
            .get_unit(*unit_id)
            .filter(|unit| unit.is_operational())
    }) else {
        return ContextualWorkOrder::Unsupported;
    };
    let query = AttackQuery {
        relation: TacticRelation::Enemy,
        squad_mode: squad.mode,
        ability_id: Some(ability_id),
        target_proto_object_name: None,
        tactic_state: source.tactic_state(),
        flags: AttackQueryFlags::empty(),
    };
    let selected = gameplay.select_mine_action(&source.proto_object_name, &query, |action| {
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
    });
    selected.map_or(ContextualWorkOrder::Move(target), |_| {
        ContextualWorkOrder::Mines {
            target: MoveDestination::Position(target),
            ability_id,
        }
    })
}

fn work_destination(
    effect: &Effect,
    script: &TriggerScript,
    world: &World,
) -> Option<MoveDestination> {
    let unit_target = variable_is_used(effect, script, 3)
        .then(|| unit_at(effect, script, 3))
        .flatten()
        .and_then(|unit_id| world.squad_move_entity_target(unit_id).map(|_| unit_id));
    let squad_target = variable_is_used(effect, script, 5)
        .then(|| squad_at(effect, script, 5))
        .flatten()
        .and_then(|squad_id| world.squad_move_entity_target(squad_id).map(|_| squad_id));
    let location_target = variable_is_used(effect, script, 4)
        .then(|| vector_at(effect, script, 4))
        .flatten()
        .filter(|target| target.is_finite());
    unit_target
        .or(squad_target)
        .map(MoveDestination::Entity)
        .or_else(|| location_target.map(MoveDestination::Position))
}

fn resolve_contextual_work(
    world: &World,
    squad_id: EntityId,
    target_id: EntityId,
    do_ability: bool,
    gameplay: Option<&GameplayCatalog>,
) -> ContextualWorkOrder {
    let Some((target_unit_id, target_position)) = world.squad_move_entity_target(target_id) else {
        return ContextualWorkOrder::Unsupported;
    };
    let Some(gameplay) = gameplay else {
        return fallback_contextual_work(
            world,
            squad_id,
            target_id,
            target_unit_id,
            target_position,
        );
    };
    let Some((source, target)) = work_units(world, squad_id, target_unit_id) else {
        return ContextualWorkOrder::Unsupported;
    };
    let ability_id = do_ability.then(|| gameplay.command_ability_id()).flatten();
    let query = contextual_work_query(world, squad_id, target_id, source, target, ability_id);
    let selected = gameplay.select_work_action(&source.proto_object_name, &query, |action| {
        work_action_enabled(world, source, action)
    });
    if let Some(action) = selected {
        return contextual_action_order(action, target_id, target_position, ability_id);
    }
    ruleless_contextual_work(
        world,
        gameplay,
        source,
        target,
        target_id,
        target_position,
        &query,
    )
}

fn contextual_work_query<'target>(
    world: &World,
    squad_id: EntityId,
    target_id: EntityId,
    source: &crate::entities::Unit,
    target: &'target crate::entities::Unit,
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
            .map_or(crate::entities::SquadMode::Normal, |squad| squad.mode),
        ability_id,
        target_proto_object_name: Some(&target.proto_object_name),
        tactic_state: source.tactic_state(),
        flags,
    }
}

fn ruleless_contextual_work(
    world: &World,
    gameplay: &GameplayCatalog,
    source: &crate::entities::Unit,
    target: &crate::entities::Unit,
    target_id: EntityId,
    target_position: Vec3,
    query: &AttackQuery<'_>,
) -> ContextualWorkOrder {
    if query.flags.contains(AttackQueryFlags::TARGET_DAMAGED)
        && matches!(
            query.relation,
            TacticRelation::SelfPlayer | TacticRelation::Ally
        )
        && gameplay
            .select_repair_other_action(&source.proto_object_name, query, |action| {
                work_action_enabled(world, source, action)
            })
            .is_some()
    {
        return ContextualWorkOrder::RepairOther {
            target: target_id,
            ability_id: query.ability_id,
        };
    }
    if query.flags.contains(AttackQueryFlags::TARGET_CAPTURABLE)
        && gameplay
            .select_capture_action(&source.proto_object_name, query, |action| {
                work_action_enabled(world, source, action)
            })
            .is_some()
    {
        return ContextualWorkOrder::Capture(target_id);
    }
    if let Some(resource_name) = target.resource_name()
        && let Some(profile) = gameplay.gather_action(&source.proto_object_name, resource_name)
        && gameplay
            .object(&source.proto_object_name)
            .and_then(|object| {
                object.tactics().actions.iter().find(|action| {
                    action.name.eq_ignore_ascii_case(profile.action_name())
                        && work_action_enabled(world, source, action)
                })
            })
            .is_some()
    {
        return ContextualWorkOrder::Gather(target_id);
    }
    ContextualWorkOrder::Move(target_position)
}

fn contextual_action_order(
    action: &pipeline::database::hw1::tactics::Action,
    target_id: EntityId,
    target_position: Vec3,
    ability_id: Option<u8>,
) -> ContextualWorkOrder {
    let range = action
        .work_range
        .filter(|range| range.is_finite() && *range >= 0.0)
        .unwrap_or_default();
    match action.action_type.as_deref() {
        Some(kind)
            if kind.eq_ignore_ascii_case("RangedAttack")
                || kind.eq_ignore_ascii_case("HandAttack")
                || kind.eq_ignore_ascii_case("SecondaryTurretAttack") =>
        {
            ContextualWorkOrder::Attack {
                target: target_id,
                ability_id,
            }
        }
        Some(kind) if kind.eq_ignore_ascii_case("Garrison") => ContextualWorkOrder::Garrison {
            target: target_id,
            range,
        },
        Some(kind) if kind.eq_ignore_ascii_case("Join") => ContextualWorkOrder::Join {
            target: target_id,
            ability_id,
        },
        Some(kind) if kind.eq_ignore_ascii_case("Gather") => ContextualWorkOrder::Gather(target_id),
        Some(kind) if kind.eq_ignore_ascii_case("Capture") => {
            ContextualWorkOrder::Capture(target_id)
        }
        Some(kind) if kind.eq_ignore_ascii_case("RepairOther") => {
            ContextualWorkOrder::RepairOther {
                target: target_id,
                ability_id,
            }
        }
        Some(kind)
            if kind.eq_ignore_ascii_case("Move") || kind.eq_ignore_ascii_case("GaggleMove") =>
        {
            ContextualWorkOrder::Move(target_position)
        }
        Some(kind) if kind.eq_ignore_ascii_case("Hitch") => ContextualWorkOrder::Hitch(target_id),
        Some(kind) if kind.eq_ignore_ascii_case("Unhitch") => {
            ContextualWorkOrder::Unhitch(target_id)
        }
        Some(kind) if kind.eq_ignore_ascii_case("Mines") => {
            ability_id.map_or(ContextualWorkOrder::Unsupported, |ability_id| {
                ContextualWorkOrder::Mines {
                    target: MoveDestination::Entity(target_id),
                    ability_id,
                }
            })
        }
        _ => ContextualWorkOrder::Unsupported,
    }
}

fn work_units(
    world: &World,
    squad_id: EntityId,
    target_unit_id: EntityId,
) -> Option<(&crate::entities::Unit, &crate::entities::Unit)> {
    let squad = world.get_squad(squad_id)?;
    let source = squad.unit_ids.iter().find_map(|unit_id| {
        world
            .get_unit(*unit_id)
            .filter(|unit| unit.is_operational())
    })?;
    let target = world.get_unit(target_unit_id)?;
    Some((source, target))
}

fn work_relation(world: &World, source: u8, target: u8) -> TacticRelation {
    if source == target {
        return TacticRelation::SelfPlayer;
    }
    match world.player_relation(source, target) {
        Some(TeamRelation::Ally) => TacticRelation::Ally,
        Some(TeamRelation::Enemy) => TacticRelation::Enemy,
        Some(TeamRelation::Neutral) | None => TacticRelation::Neutral,
    }
}

fn fallback_contextual_work(
    world: &World,
    squad_id: EntityId,
    target_id: EntityId,
    target_unit_id: EntityId,
    target_position: Vec3,
) -> ContextualWorkOrder {
    if world
        .get_unit(target_unit_id)
        .is_some_and(|unit| unit.garrison.can_contain())
    {
        return ContextualWorkOrder::Garrison {
            target: target_id,
            range: 0.0,
        };
    }
    let Some((source_player, target_player)) = world
        .get_squad(squad_id)
        .zip(world.get_unit(target_unit_id))
        .map(|(squad, target)| (squad.base.player_id, target.base.player_id))
    else {
        return ContextualWorkOrder::Unsupported;
    };
    if world.players_are_enemies(source_player, target_player) {
        ContextualWorkOrder::Attack {
            target: target_id,
            ability_id: None,
        }
    } else {
        ContextualWorkOrder::Move(target_position)
    }
}

fn issue_contextual_work(
    world: &mut World,
    player_id: u8,
    squad_id: EntityId,
    order: ContextualWorkOrder,
    attack_move: bool,
    queue_order: bool,
    assets: WorkAssets<'_>,
) -> bool {
    match order {
        ContextualWorkOrder::Move(target) => world.issue_squad_move_order_to_position(
            player_id,
            squad_id,
            target,
            attack_move,
            queue_order,
        ),
        ContextualWorkOrder::Attack { target, ability_id } => world
            .issue_attack_order_with_context(player_id, squad_id, target, 0.0, None, ability_id),
        ContextualWorkOrder::Garrison { target, range } => world
            .issue_garrison_order(player_id, squad_id, target, range)
            .is_ok(),
        ContextualWorkOrder::Join { target, ability_id } => {
            world.issue_join_order(player_id, squad_id, target, ability_id)
        }
        ContextualWorkOrder::Gather(target) => assets.gameplay.is_some_and(|gameplay| {
            world.issue_gather_order(player_id, squad_id, target, gameplay)
        }),
        ContextualWorkOrder::Capture(target) => {
            assets
                .database
                .zip(assets.gameplay)
                .is_some_and(|(database, gameplay)| {
                    world.issue_capture_order(player_id, squad_id, target, database, gameplay)
                })
        }
        ContextualWorkOrder::RepairOther { target, ability_id } => assets
            .database
            .zip(assets.gameplay)
            .is_some_and(|(database, gameplay)| {
                world.issue_repair_other_order(
                    player_id, squad_id, target, ability_id, database, gameplay,
                )
            }),
        ContextualWorkOrder::Hitch(target) => {
            world.issue_hitch_order(player_id, squad_id, target).is_ok()
        }
        ContextualWorkOrder::Unhitch(target) => world
            .issue_unhitch_order(player_id, squad_id, target)
            .is_ok(),
        ContextualWorkOrder::Mines { target, ability_id } => {
            let (target_entity, target_position) = match target {
                MoveDestination::Entity(entity) => (Some(entity), None),
                MoveDestination::Position(position) => (None, Some(position)),
            };
            world.issue_mines_order(
                player_id,
                squad_id,
                target_entity,
                target_position,
                None,
                ability_id,
            )
        }
        ContextualWorkOrder::Unsupported => false,
    }
}

fn work_action_enabled(
    world: &World,
    source: &crate::entities::Unit,
    action: &pipeline::database::hw1::tactics::Action,
) -> bool {
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

#[derive(Debug, Clone, Copy)]
struct OrderSlots {
    squad: u16,
    squad_list: u16,
    target_unit: u16,
    target_location: u16,
    target_squad: u16,
    attack_move: u16,
    queue_order: u16,
}

fn execute_location_order(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    slots: OrderSlots,
) -> EffectOutcome {
    let attack_move = bool_at(effect, script, slots.attack_move).unwrap_or(false);
    let queue_order = bool_at(effect, script, slots.queue_order).unwrap_or(false);
    let use_squad = variable_is_used(effect, script, slots.squad);
    let use_squad_list = variable_is_used(effect, script, slots.squad_list);
    if !use_squad && !use_squad_list {
        return EffectOutcome::Skipped;
    }

    let unit_target = variable_is_used(effect, script, slots.target_unit)
        .then(|| unit_at(effect, script, slots.target_unit))
        .flatten()
        .and_then(|unit_id| world.squad_move_entity_target(unit_id).map(|_| unit_id));
    let squad_target = variable_is_used(effect, script, slots.target_squad)
        .then(|| squad_at(effect, script, slots.target_squad))
        .flatten()
        .and_then(|squad_id| world.squad_move_entity_target(squad_id).map(|_| squad_id));
    let location_target = variable_is_used(effect, script, slots.target_location)
        .then(|| vector_at(effect, script, slots.target_location))
        .flatten()
        .filter(|target| target.is_finite());
    let target = unit_target
        .or(squad_target)
        .map(MoveDestination::Entity)
        .or_else(|| location_target.map(MoveDestination::Position));
    let Some(target) = target else {
        return EffectOutcome::Skipped;
    };

    let mut working_squads = if use_squad_list {
        list_at(effect, script, slots.squad_list)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if use_squad
        && let Some(squad_id) = squad_at(effect, script, slots.squad)
        && world.get_squad(squad_id).is_some()
    {
        unique_add(&mut working_squads, squad_id);
    }
    let mut issued = false;
    let mut unique_squads = Vec::new();
    for squad_id in working_squads {
        unique_add(&mut unique_squads, squad_id);
    }
    for squad_id in unique_squads {
        let Some(player_id) = world.get_squad(squad_id).map(|squad| squad.base.player_id) else {
            continue;
        };
        issued |= match target {
            MoveDestination::Position(position) => world.issue_squad_move_order_to_position(
                player_id,
                squad_id,
                position,
                attack_move,
                queue_order,
            ),
            MoveDestination::Entity(target_id) => world.issue_squad_move_order_to_entity(
                player_id,
                squad_id,
                target_id,
                attack_move,
                queue_order,
            ),
        };
    }
    if issued {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

#[derive(Debug, Clone, Copy)]
enum MoveDestination {
    Position(glam::Vec3),
    Entity(EntityId),
}

fn issue_path_to_squads(
    world: &mut World,
    working_squads: &[EntityId],
    waypoints: &[Vec3],
    attack_move: bool,
    queue_order: bool,
    reverse_move: bool,
) -> EffectOutcome {
    let mut changed = false;
    if reverse_move {
        for &squad_id in working_squads {
            changed |= world.set_squad_reverse_move(squad_id, true);
        }
    }
    let mut unique_squads = Vec::new();
    for &squad_id in working_squads {
        unique_add(&mut unique_squads, squad_id);
    }
    for squad_id in unique_squads {
        let Some(player_id) = world.get_squad(squad_id).map(|squad| squad.base.player_id) else {
            continue;
        };
        changed |=
            world.issue_squad_move_path(player_id, squad_id, waypoints, attack_move, queue_order);
    }
    if changed {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

fn trim_path_to_closest(waypoints: &mut Vec<Vec3>, squads: &[EntityId], world: &World) {
    let invalid = Vec3::splat(-1.0);
    let mut starting_point = invalid;
    let mut valid_squads = 0_usize;
    for &squad_id in squads {
        if let Some(squad) = world.get_squad(squad_id) {
            starting_point += squad.position();
            valid_squads += 1;
        }
    }
    if starting_point == invalid {
        return;
    }
    starting_point /= valid_squads.to_f32().unwrap_or(1.0);
    let mut close_index = closest_waypoint_index(waypoints, starting_point);
    let previous = (close_index > 0).then(|| {
        closest_point_on_line(
            waypoints[close_index - 1],
            waypoints[close_index],
            starting_point,
        )
    });
    let next_index = close_index + 1;
    let next = (next_index < waypoints.len()).then(|| {
        closest_point_on_line(
            waypoints[close_index],
            waypoints[next_index],
            starting_point,
        )
    });
    match (previous, next) {
        (Some(previous), Some(next)) => {
            if xz_distance_squared(starting_point, previous)
                < xz_distance_squared(starting_point, next)
            {
                close_index -= 1;
                waypoints[close_index] = previous;
            } else {
                waypoints[close_index] = next;
            }
        }
        (Some(previous), None) => {
            close_index -= 1;
            waypoints[close_index] = previous;
        }
        (None, next) => waypoints[close_index] = next.unwrap_or(invalid),
    }
    waypoints.drain(..close_index);
}

fn closest_waypoint_index(waypoints: &[Vec3], point: Vec3) -> usize {
    let mut closest = 0;
    let mut shortest = xz_distance_squared(point, waypoints[0]);
    for (index, waypoint) in waypoints.iter().enumerate().skip(1) {
        let distance = xz_distance_squared(point, *waypoint);
        if shortest > distance {
            shortest = distance;
            closest = index;
        }
    }
    closest
}

fn closest_point_on_line(start: Vec3, end: Vec3, point: Vec3) -> Vec3 {
    let segment = end - start;
    let length = segment.length();
    if length <= f32::EPSILON {
        return start;
    }
    let direction = segment / length;
    let distance = direction.dot(point - start);
    if distance < 0.0 {
        start
    } else if distance > length {
        end
    } else {
        start + direction * distance
    }
}

fn xz_distance_squared(left: Vec3, right: Vec3) -> f32 {
    Vec3::new(left.x - right.x, 0.0, left.z - right.z).length_squared()
}

fn waypoint_list_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<Vec<Vec3>> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::LocationList(values) | TriggerValue::VectorList(values) => Some(
            values
                .iter()
                .map(|value| Vec3::new(value.x, value.y, value.z))
                .collect(),
        ),
        _ => None,
    }
}

fn unit_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<EntityId> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Unit(value) => Some(*value),
        _ => None,
    }
}

fn squad_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<EntityId> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Squad(value) => Some(*value),
        _ => None,
    }
}

fn list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
) -> Option<&'a Vec<EntityId>> {
    entity_list(
        EntityListKind::Squad,
        value_at(effect, script, signature_id)?,
    )
}

#[cfg(test)]
mod tests;
