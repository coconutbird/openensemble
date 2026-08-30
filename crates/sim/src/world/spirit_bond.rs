//! Source-backed execution of persistent squad `SpiritBond` actions.

use super::World;
use crate::entities::squads::SpiritBondPhase;
use crate::entities::{Object, Squad};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, SpiritBondProfile};
use crate::player::PlayerId;
use glam::Vec3;
use pipeline::database::hw1::Database;

#[derive(Debug, Clone)]
struct SpiritBondContext {
    player_id: PlayerId,
    leader_id: EntityId,
    profile: SpiritBondProfile,
}

impl World {
    pub(super) fn update_spirit_bonds(&mut self, database: &Database, gameplay: &GameplayCatalog) {
        let squad_ids = self.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_spirit_bond(squad_id, database, gameplay);
        }
    }

    fn update_spirit_bond(
        &mut self,
        squad_id: EntityId,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(phase) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.spirit_bond.phase())
        else {
            return;
        };
        if phase == SpiritBondPhase::Done {
            return;
        }
        let Some(context) = self.spirit_bond_context(squad_id, gameplay) else {
            if phase == SpiritBondPhase::Active {
                self.finish_spirit_bond(squad_id);
            }
            return;
        };
        if !self.spirit_bond_enabled(&context) {
            return;
        }
        let Some((member_ids, centers)) = self.spirit_bond_members(squad_id) else {
            self.finish_spirit_bond(squad_id);
            return;
        };
        let beam_id = if phase == SpiritBondPhase::Dormant {
            for member_id in member_ids {
                if let Some(unit) = self.units.get_mut(member_id) {
                    unit.set_spirit_bond_damage_multiplier(context.profile.damage_modifier());
                }
            }
            let beam_id = self.create_spirit_bond_beam(&context, centers, database);
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.spirit_bond.activate(beam_id);
            }
            beam_id
        } else {
            self.ensure_spirit_bond_beam(squad_id, &context, centers, database)
        };
        if let Some(beam_id) = beam_id {
            self.sync_spirit_bond_beam(beam_id, centers);
        }
    }

    fn spirit_bond_context(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<SpiritBondContext> {
        let squad = self.squads.get(squad_id)?;
        if !squad.is_alive() {
            return None;
        }
        let leader_id = *squad.unit_ids.first()?;
        let leader = self.units.get(leader_id)?;
        Some(SpiritBondContext {
            player_id: squad.base.player_id,
            leader_id,
            profile: gameplay.spirit_bond(&leader.proto_object_name)?.clone(),
        })
    }

    fn spirit_bond_enabled(&self, context: &SpiritBondContext) -> bool {
        let Some(leader) = self.units.get(context.leader_id) else {
            return false;
        };
        let authored_enabled = !context.profile.starts_disabled();
        let player_enabled =
            self.get_player(context.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &leader.proto_object_name,
                        context.profile.action_name(),
                        authored_enabled,
                    )
                });
        leader
            .actions
            .is_enabled(context.profile.action_name(), !player_enabled)
    }

    fn spirit_bond_members(&self, squad_id: EntityId) -> Option<([EntityId; 2], [Vec3; 2])> {
        let squad = self.squads.get(squad_id)?;
        let [first_id, second_id] = squad.unit_ids.as_slice() else {
            return None;
        };
        let first = self.units.get(*first_id).filter(|unit| unit.is_alive())?;
        let second = self.units.get(*second_id).filter(|unit| unit.is_alive())?;
        Some((
            [*first_id, *second_id],
            [first.simulation_center(), second.simulation_center()],
        ))
    }

    fn create_spirit_bond_beam(
        &mut self,
        context: &SpiritBondContext,
        centers: [Vec3; 2],
        database: &Database,
    ) -> Option<EntityId> {
        let logical_name = context.profile.beam_proto_object()?;
        let effective_name = self
            .get_player(context.player_id)
            .map_or(logical_name, |player| {
                player.technologies.resolved_unit_prototype(logical_name)
            });
        let (prototype_index, prototype) = database
            .objects
            .iter()
            .enumerate()
            .find(|(_, prototype)| prototype.name.eq_ignore_ascii_case(effective_name))
            .or_else(|| {
                database
                    .objects
                    .iter()
                    .enumerate()
                    .find(|(_, prototype)| prototype.name.eq_ignore_ascii_case(logical_name))
            })?;
        let prototype_id = prototype
            .dbid
            .unwrap_or_else(|| i32::try_from(prototype_index).unwrap_or(-1));
        let object_id = self.objects.allocate_id();
        let mut beam = Object::new_visual(
            object_id,
            context.player_id,
            centers[0],
            bond_forward(centers),
            prototype_id,
            prototype.name.clone(),
        );
        beam.set_visual_secondary_position(Some(centers[1]));
        self.objects.insert(object_id, beam);
        Some(object_id)
    }

    fn ensure_spirit_bond_beam(
        &mut self,
        squad_id: EntityId,
        context: &SpiritBondContext,
        centers: [Vec3; 2],
        database: &Database,
    ) -> Option<EntityId> {
        let current = self.squads.get(squad_id).and_then(Squad::spirit_bond_beam);
        if current.is_some_and(|beam_id| self.objects.get(beam_id).is_some()) {
            return current;
        }
        let replacement = self.create_spirit_bond_beam(context, centers, database);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.spirit_bond.set_beam(replacement);
        }
        replacement
    }

    fn sync_spirit_bond_beam(&mut self, beam_id: EntityId, centers: [Vec3; 2]) {
        if let Some(beam) = self.objects.get_mut(beam_id) {
            beam.base.set_position(centers[0]);
            beam.base.set_forward(bond_forward(centers));
            beam.set_visual_secondary_position(Some(centers[1]));
        }
    }

    fn finish_spirit_bond(&mut self, squad_id: EntityId) {
        let Some((beam_id, member_ids)) = self
            .squads
            .get_mut(squad_id)
            .map(|squad| (squad.spirit_bond.finish(), squad.unit_ids.clone()))
        else {
            return;
        };
        for member_id in member_ids {
            if let Some(unit) = self.units.get_mut(member_id) {
                unit.set_spirit_bond_damage_multiplier(1.0);
            }
        }
        if let Some(beam_id) = beam_id {
            let _removed = self.remove_object(beam_id);
        }
    }

    pub(super) fn prepare_remove_squad_spirit_bond(&mut self, squad_id: EntityId) {
        self.finish_spirit_bond(squad_id);
    }

    pub(super) fn prepare_remove_unit_spirit_bond(&mut self, unit_id: EntityId) {
        let Some(squad_id) = self.units.get(unit_id).and_then(|unit| unit.squad_id) else {
            return;
        };
        self.prepare_squad_membership_change_spirit_bond(squad_id);
    }

    pub(super) fn prepare_squad_membership_change_spirit_bond(&mut self, squad_id: EntityId) {
        if self
            .squads
            .get(squad_id)
            .is_some_and(crate::entities::Squad::spirit_bond_active)
        {
            self.finish_spirit_bond(squad_id);
        }
    }
}

fn bond_forward(centers: [Vec3; 2]) -> Vec3 {
    (centers[1] - centers[0]).try_normalize().unwrap_or(Vec3::Z)
}

#[cfg(test)]
mod tests;
