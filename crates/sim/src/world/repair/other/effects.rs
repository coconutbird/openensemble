//! Sim-owned repair beams and fallback attachments consumed by the renderer.

use super::World;
use crate::entities::Object;
use crate::entity_id::EntityId;
use crate::gameplay::RepairOtherActionProfile;
use crate::player::PlayerId;
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

pub(super) struct RepairEffectContext<'profile> {
    pub source_leader_id: EntityId,
    pub target_leader_id: EntityId,
    pub target_player_id: PlayerId,
    pub profile: &'profile RepairOtherActionProfile,
}

impl World {
    pub(super) fn ensure_repair_other_effect(
        &mut self,
        source_squad_id: EntityId,
        context: &RepairEffectContext<'_>,
        database: &Database,
    ) {
        let existing = self.squads.get(source_squad_id).and_then(|squad| {
            squad.repair_other.effect_id().map(|effect_id| {
                (
                    effect_id,
                    squad.repair_other.beam_head_id(),
                    squad.repair_other.beam_tail_id(),
                )
            })
        });
        if let Some(ids) = existing
            && self.objects.get(ids.0).is_some()
        {
            self.sync_repair_other_effect(ids, context);
            return;
        }
        let stale = self
            .squads
            .get_mut(source_squad_id)
            .map(|squad| squad.repair_other.take_effect_ids())
            .unwrap_or_default();
        self.remove_repair_other_effects(stale);
        let Some(ids) = self.create_repair_other_effect(context, database) else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(source_squad_id) {
            squad.repair_other.set_effect_ids(ids.0, ids.1, ids.2);
        }
    }

    pub(super) fn remove_repair_other_effects(
        &mut self,
        effect_ids: impl IntoIterator<Item = EntityId>,
    ) {
        for effect_id in effect_ids {
            let _removed = self.remove_object(effect_id);
        }
    }

    fn create_repair_other_effect(
        &mut self,
        context: &RepairEffectContext<'_>,
        database: &Database,
    ) -> Option<(EntityId, Option<EntityId>, Option<EntityId>)> {
        let centers = self.repair_effect_centers(context)?;
        if let Some(logical_name) = context.profile.effect_proto_object() {
            return self.create_repair_other_beam(context, centers, database, logical_name);
        }
        let (prototype_id, prototype_name, _, _) =
            self.resolve_repair_visual(database, context.target_player_id, "fx_repairing")?;
        let effect_id = self.add_visual_attachment_to_unit(
            context.target_leader_id,
            prototype_id,
            &prototype_name,
        )?;
        Some((effect_id, None, None))
    }

    fn create_repair_other_beam(
        &mut self,
        context: &RepairEffectContext<'_>,
        centers: [Vec3; 2],
        database: &Database,
        logical_name: &str,
    ) -> Option<(EntityId, Option<EntityId>, Option<EntityId>)> {
        let (prototype_id, prototype_name, beam_head, beam_tail) =
            self.resolve_repair_visual(database, context.target_player_id, logical_name)?;
        let main = self.insert_repair_visual(
            context.target_player_id,
            prototype_id,
            &prototype_name,
            centers[0],
            Some(centers[1]),
        );
        let head = beam_head.and_then(|name| {
            let (id, effective, _, _) =
                self.resolve_repair_visual(database, context.target_player_id, &name)?;
            Some(self.insert_repair_visual(
                context.target_player_id,
                id,
                &effective,
                centers[0],
                None,
            ))
        });
        let tail = beam_tail.and_then(|name| {
            let (id, effective, _, _) =
                self.resolve_repair_visual(database, context.target_player_id, &name)?;
            Some(self.insert_repair_visual(
                context.target_player_id,
                id,
                &effective,
                centers[1],
                None,
            ))
        });
        Some((main, head, tail))
    }

    fn insert_repair_visual(
        &mut self,
        owner: PlayerId,
        prototype_id: i32,
        prototype_name: &str,
        position: Vec3,
        secondary: Option<Vec3>,
    ) -> EntityId {
        let id = self.objects.allocate_id();
        let forward = secondary
            .map(|target| (target - position).normalize_or_zero())
            .filter(|forward| *forward != Vec3::ZERO)
            .unwrap_or(Vec3::Z);
        let mut object = Object::new_visual(
            id,
            owner,
            position,
            forward,
            prototype_id,
            prototype_name.to_owned(),
        );
        object.set_visual_secondary_position(secondary);
        self.objects.insert(id, object);
        id
    }

    fn sync_repair_other_effect(
        &mut self,
        ids: (EntityId, Option<EntityId>, Option<EntityId>),
        context: &RepairEffectContext<'_>,
    ) {
        if context.profile.effect_proto_object().is_none() {
            return;
        }
        let Some(centers) = self.repair_effect_centers(context) else {
            return;
        };
        if let Some(beam) = self.objects.get_mut(ids.0) {
            beam.base.set_position(centers[0]);
            let forward = (centers[1] - centers[0]).normalize_or_zero();
            beam.base.set_forward(if forward == Vec3::ZERO {
                Vec3::Z
            } else {
                forward
            });
            beam.set_visual_secondary_position(Some(centers[1]));
        }
        if let Some(head) = ids.1.and_then(|id| self.objects.get_mut(id)) {
            head.base.set_position(centers[0]);
        }
        if let Some(tail) = ids.2.and_then(|id| self.objects.get_mut(id)) {
            tail.base.set_position(centers[1]);
        }
    }

    fn repair_effect_centers(&self, context: &RepairEffectContext<'_>) -> Option<[Vec3; 2]> {
        let source = self.units.get(context.source_leader_id)?;
        let target = self.units.get(context.target_leader_id)?;
        Some([source.simulation_center(), target.simulation_center()])
    }

    fn resolve_repair_visual(
        &self,
        database: &Database,
        player_id: PlayerId,
        logical_name: &str,
    ) -> Option<(i32, String, Option<String>, Option<String>)> {
        let effective_name = self
            .get_player(player_id)
            .map_or(logical_name, |player| {
                player.technologies.resolved_unit_prototype(logical_name)
            })
            .to_owned();
        let (index, prototype) = find_object(database, &effective_name)
            .or_else(|| find_object(database, logical_name))?;
        Some((
            prototype
                .dbid
                .unwrap_or_else(|| i32::try_from(index).unwrap_or(-1)),
            prototype.name.clone(),
            clean_name(prototype.beam_head.as_deref()),
            clean_name(prototype.beam_tail.as_deref()),
        ))
    }
}

fn find_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<(usize, &'database ProtoObject)> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, object)| object.name.trim().eq_ignore_ascii_case(name.trim()))
}

fn clean_name(name: Option<&str>) -> Option<String> {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}
