//! Allied squad buffs and renderer-facing aura attachments owned by Rage.

use super::execution::{AuraAttachmentSet, RagePowerExecution};
use crate::EntityId;
use crate::entities::UnitDataScalar;
use crate::entity::Entity;
use crate::world::World;
use pipeline::database::hw1::Database;

pub(super) fn update(world: &mut World, database: &Database, execution: &mut RagePowerExecution) {
    if execution.aura_radius < f32::EPSILON {
        return;
    }
    let Some(center) = world
        .get_squad(execution.owner_squad_id)
        .map(|squad| squad.base.position)
    else {
        return;
    };
    let candidates = world
        .find_squads_by_leader_type_in_area(
            &execution.aura_filter_type,
            center,
            execution.aura_radius,
        )
        .into_iter()
        .filter(|id| {
            world.get_squad(*id).is_some_and(|squad| {
                squad.is_alive()
                    && world.players_are_allied(execution.player_id, squad.base.player_id)
            })
        })
        .collect::<Vec<_>>();
    let leaving = execution
        .aura_squad_ids
        .iter()
        .filter(|id| !candidates.contains(id))
        .copied()
        .collect::<Vec<_>>();
    let arriving = candidates
        .iter()
        .filter(|id| !execution.aura_squad_ids.contains(id))
        .copied()
        .collect::<Vec<_>>();
    for squad_id in leaving {
        leave_squad(world, execution, squad_id);
    }
    for squad_id in arriving {
        enter_squad(world, database, execution, squad_id);
    }
    execution.aura_squad_ids = candidates;
    execution
        .aura_attachments
        .sort_by_key(|attachments| attachments.squad_id);
}

pub(super) fn clear(world: &mut World, execution: &mut RagePowerExecution) {
    let squad_ids = execution.aura_squad_ids.clone();
    for squad_id in squad_ids {
        leave_squad(world, execution, squad_id);
    }
    execution.aura_squad_ids.clear();
}

fn enter_squad(
    world: &mut World,
    database: &Database,
    execution: &mut RagePowerExecution,
    squad_id: EntityId,
) {
    let mut attachment_ids = Vec::new();
    if squad_id != execution.owner_squad_id {
        let unit_ids = squad_unit_ids(world, squad_id);
        if execution.aura_damage_bonus > f32::EPSILON {
            adjust_units(
                world,
                &unit_ids,
                UnitDataScalar::Damage,
                execution.aura_damage_bonus,
            );
        }
        for unit_id in unit_ids {
            let prototype = aura_prototype(world, execution, unit_id);
            if let Some(id) = world.add_prototype_attachment_to_unit(database, unit_id, prototype) {
                attachment_ids.push(id);
            }
        }
    }
    execution.aura_attachments.push(AuraAttachmentSet {
        squad_id,
        attachment_ids,
    });
}

fn leave_squad(world: &mut World, execution: &mut RagePowerExecution, squad_id: EntityId) {
    if squad_id != execution.owner_squad_id && execution.aura_damage_bonus > f32::EPSILON {
        let unit_ids = squad_unit_ids(world, squad_id);
        adjust_units(
            world,
            &unit_ids,
            UnitDataScalar::Damage,
            execution.aura_damage_bonus.recip(),
        );
    }
    if let Some(index) = execution
        .aura_attachments
        .iter()
        .position(|attachments| attachments.squad_id == squad_id)
    {
        let attachments = execution.aura_attachments.remove(index);
        for attachment_id in attachments.attachment_ids {
            let _removed = world.remove_object(attachment_id);
        }
    }
}

fn aura_prototype(world: &World, execution: &RagePowerExecution, unit_id: EntityId) -> i32 {
    let radius = world
        .get_unit(unit_id)
        .map_or(0.0, crate::entities::Unit::obstruction_radius);
    let index = if radius < 2.9 {
        0
    } else if radius < 7.9 {
        1
    } else {
        2
    };
    execution.aura_attachment_prototype_ids[index]
}

fn squad_unit_ids(world: &World, squad_id: EntityId) -> Vec<EntityId> {
    world
        .get_squad(squad_id)
        .map_or_else(Vec::new, |squad| squad.unit_ids.clone())
}

fn adjust_units(world: &mut World, ids: &[EntityId], scalar: UnitDataScalar, multiplier: f32) {
    for id in ids {
        if let Some(unit) = world.get_unit_mut(*id) {
            unit.adjust_data_scalar(scalar, multiplier);
        }
    }
}
