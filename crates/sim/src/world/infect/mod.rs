//! Persistent Flood infection scanning, exposure work, and lifecycle dispatch.

mod conversion;

use super::World;
use crate::entities::units::{InfectionExposure, InfectionVisual};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, InfectActionProfile};
use crate::player::{GAIA_PLAYER, PlayerId};
use crate::spawn::object_prototype_id;
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::gamedata::InfectionMapEntry;
use pipeline::database::hw1::{Database, ProtoObject};

const SCAN_INTERVAL_SECONDS: f32 = 0.5;

#[derive(Debug, Clone)]
struct InfectUpdateContext {
    profile: InfectActionProfile,
    source_squad_id: EntityId,
    source_player_id: PlayerId,
    position: Vec3,
    enabled: bool,
    work_rate: f32,
    conversion_limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExposureDisposition {
    Retain,
    Complete,
    SourceLimit,
}

impl World {
    pub(in crate::world) fn update_infections(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.resolve_infection_lifecycles(database);
        let unit_ids = self.units.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for unit_id in unit_ids {
            self.update_unit_infect(unit_id, dt, database, gameplay);
        }
    }

    pub(super) fn prepare_remove_unit_infection(&mut self, unit_id: EntityId) {
        self.disconnect_unit_infect_action(unit_id);
    }

    fn update_unit_infect(
        &mut self,
        unit_id: EntityId,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(context) = self.infect_update_context(unit_id, database, gameplay) else {
            self.disconnect_unit_infect_action(unit_id);
            return;
        };
        self.connect_unit_infect_action(unit_id, &context);
        if !context.enabled {
            return;
        }
        if self.advance_infection_exposures(unit_id, dt, database, &context) {
            return;
        }
        self.update_infection_scan(unit_id, dt, database, &context);
    }

    fn infect_update_context(
        &self,
        unit_id: EntityId,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> Option<InfectUpdateContext> {
        let unit = self.units.get(unit_id).filter(|unit| unit.is_alive())?;
        let source_squad_id = unit.squad_id.filter(|id| self.squads.get(*id).is_some())?;
        let profile = gameplay.infect(&unit.proto_object_name)?.clone();
        let authored_enabled = !profile.starts_disabled();
        let (player_enabled, work_rate) = self.get_player(unit.base.player_id).map_or(
            (authored_enabled, profile.work_rate()),
            |player| {
                (
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    ),
                    player.technologies.action_work_rate(
                        &unit.proto_object_name,
                        profile.action_name(),
                        profile.work_rate(),
                    ),
                )
            },
        );
        let conversion_limit = unit_prototype(database, unit)
            .and_then(|prototype| prototype.num_conversions)
            .filter(|limit| *limit > 0)
            .and_then(|limit| u32::try_from(limit).ok());
        Some(InfectUpdateContext {
            enabled: unit
                .actions
                .is_enabled(profile.action_name(), !player_enabled),
            profile,
            source_squad_id,
            source_player_id: unit.base.player_id,
            position: unit.base.position,
            work_rate: finite_nonnegative(work_rate),
            conversion_limit,
        })
    }

    fn connect_unit_infect_action(&mut self, unit_id: EntityId, context: &InfectUpdateContext) {
        let matches = self.units.get(unit_id).is_some_and(|unit| {
            unit.infection
                .action_matches(&unit.proto_object_name, context.profile.action_name())
        });
        if matches {
            return;
        }
        self.disconnect_unit_infect_action(unit_id);
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.infection.connect_action(
                &unit.proto_object_name.clone(),
                context.profile.action_name(),
            );
        }
    }

    fn disconnect_unit_infect_action(&mut self, unit_id: EntityId) {
        let attachment_ids = self
            .units
            .get_mut(unit_id)
            .map_or_else(Vec::new, |unit| unit.infection.disconnect_action());
        for attachment_id in attachment_ids {
            let _removed = self.remove_object(attachment_id);
        }
    }

    fn advance_infection_exposures(
        &mut self,
        source_id: EntityId,
        dt: f32,
        database: &Database,
        context: &InfectUpdateContext,
    ) -> bool {
        let mut index = 0;
        while index
            < self
                .units
                .get(source_id)
                .map_or(0, |unit| unit.infection.exposures.len())
        {
            let Some(target_squad_id) =
                self.advance_infection_exposure_clock(source_id, index, dt, context)
            else {
                break;
            };
            let disposition =
                self.spend_infection_bank(source_id, index, target_squad_id, database, context);
            if disposition == ExposureDisposition::Retain {
                index += 1;
                continue;
            }
            self.remove_infection_exposure(source_id, index);
            if disposition == ExposureDisposition::SourceLimit {
                self.disconnect_unit_infect_action(source_id);
                let _killed = self.kill_squad(context.source_squad_id, false);
                return true;
            }
        }
        false
    }

    fn advance_infection_exposure_clock(
        &mut self,
        source_id: EntityId,
        index: usize,
        dt: f32,
        context: &InfectUpdateContext,
    ) -> Option<EntityId> {
        let exposure = self
            .units
            .get_mut(source_id)?
            .infection
            .exposures
            .get_mut(index)?;
        exposure.elapsed_seconds += dt;
        let minimum_ms = context
            .profile
            .min_idle_duration_ms()
            .to_f32()
            .unwrap_or(f32::MAX);
        if exposure.elapsed_seconds * 1_000.0 >= minimum_ms {
            exposure.combat_value_bank += dt * context.work_rate;
        }
        Some(exposure.squad_id)
    }

    fn spend_infection_bank(
        &mut self,
        source_id: EntityId,
        exposure_index: usize,
        target_squad_id: EntityId,
        database: &Database,
        context: &InfectUpdateContext,
    ) -> ExposureDisposition {
        let Some(unit_ids) = self
            .squads
            .get(target_squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return ExposureDisposition::Complete;
        };
        let mut infected_any = false;
        for unit_id in unit_ids {
            let Some(combat_value) = infection_combat_value(self, database, unit_id) else {
                continue;
            };
            let Some((attachment_id, infected_count)) =
                self.commit_infection_work(source_id, exposure_index, unit_id, combat_value)
            else {
                continue;
            };
            if let Some(attachment_id) = attachment_id {
                let _removed = self.remove_object(attachment_id);
            }
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.mark_for_infection(context.source_player_id);
            }
            infected_any = true;
            if context
                .conversion_limit
                .is_some_and(|limit| infected_count >= limit)
            {
                return ExposureDisposition::SourceLimit;
            }
        }
        if infected_any {
            ExposureDisposition::Complete
        } else {
            ExposureDisposition::Retain
        }
    }

    fn commit_infection_work(
        &mut self,
        source_id: EntityId,
        exposure_index: usize,
        target_id: EntityId,
        combat_value: f32,
    ) -> Option<(Option<EntityId>, u32)> {
        let infection = &mut self.units.get_mut(source_id)?.infection;
        let exposure = infection.exposures.get_mut(exposure_index)?;
        if combat_value > exposure.combat_value_bank {
            return None;
        }
        exposure.combat_value_bank -= combat_value;
        let attachment_id = exposure
            .visuals
            .iter()
            .position(|visual| visual.unit_id == target_id)
            .map(|index| exposure.visuals.swap_remove(index).attachment_id);
        infection.infected_count = infection.infected_count.saturating_add(1);
        Some((attachment_id, infection.infected_count))
    }

    fn remove_infection_exposure(&mut self, source_id: EntityId, index: usize) {
        let visuals = self.units.get_mut(source_id).map_or_else(Vec::new, |unit| {
            if index >= unit.infection.exposures.len() {
                return Vec::new();
            }
            unit.infection.exposures.swap_remove(index).visuals
        });
        for visual in visuals {
            let _removed = self.remove_object(visual.attachment_id);
        }
    }

    fn update_infection_scan(
        &mut self,
        source_id: EntityId,
        dt: f32,
        database: &Database,
        context: &InfectUpdateContext,
    ) {
        let scan_due = self.units.get_mut(source_id).is_some_and(|unit| {
            unit.infection.time_until_next_scan -= dt;
            if unit.infection.time_until_next_scan > 0.0 {
                return false;
            }
            unit.infection.time_until_next_scan = SCAN_INTERVAL_SECONDS;
            true
        });
        if !scan_due {
            return;
        }
        let squad_ids = self.find_live_squads(
            None,
            None,
            None,
            Some((context.position, context.profile.work_range())),
        );
        for squad_id in squad_ids {
            if self.is_new_infection_target(source_id, squad_id, database, context) {
                self.begin_infection_exposure(source_id, squad_id, database, &context.profile);
            }
        }
    }

    fn is_new_infection_target(
        &self,
        source_id: EntityId,
        target_squad_id: EntityId,
        database: &Database,
        context: &InfectUpdateContext,
    ) -> bool {
        if target_squad_id == context.source_squad_id {
            return false;
        }
        let Some(squad) = self.squads.get(target_squad_id) else {
            return false;
        };
        if squad.base.player_id == context.source_player_id || squad.base.player_id == GAIA_PLAYER {
            return false;
        }
        let already_tracked = self.units.get(source_id).is_some_and(|unit| {
            unit.infection
                .exposures
                .iter()
                .any(|exposure| exposure.squad_id == target_squad_id)
        });
        !already_tracked
            && squad.unit_ids.iter().any(|unit_id| {
                self.units.get(*unit_id).is_some_and(|unit| {
                    unit.is_alive()
                        && !unit.is_undergoing_infection()
                        && infection_mapping(database, unit).is_some()
                        && !context
                            .profile
                            .invalid_targets()
                            .iter()
                            .any(|target| unit.is_object_type(target))
                })
            })
    }

    fn begin_infection_exposure(
        &mut self,
        source_id: EntityId,
        squad_id: EntityId,
        database: &Database,
        profile: &InfectActionProfile,
    ) {
        let unit_ids = self
            .squads
            .get(squad_id)
            .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
        let attachment_proto = profile
            .attachment_proto_object()
            .and_then(|name| object_prototype_id(database, name));
        let mut visuals = Vec::new();
        if let Some(prototype_id) = attachment_proto {
            for unit_id in unit_ids {
                if let Some(attachment_id) =
                    self.add_prototype_attachment_to_unit(database, unit_id, prototype_id)
                {
                    visuals.push(InfectionVisual {
                        unit_id,
                        attachment_id,
                    });
                }
            }
        }
        if let Some(source) = self.units.get_mut(source_id) {
            source.infection.exposures.push(InfectionExposure {
                squad_id,
                elapsed_seconds: 0.0,
                combat_value_bank: 0.0,
                visuals,
            });
        }
    }
}

fn infection_combat_value(world: &World, database: &Database, unit_id: EntityId) -> Option<f32> {
    let unit = world
        .units
        .get(unit_id)
        .filter(|unit| unit.is_alive() && !unit.is_undergoing_infection())?;
    let prototype = unit_prototype(database, unit)?;
    Some(finite_nonnegative(
        prototype.combat_value.unwrap_or_default(),
    ))
}

fn unit_prototype<'a>(
    database: &'a Database,
    unit: &crate::entities::Unit,
) -> Option<&'a ProtoObject> {
    database.objects.iter().find(|prototype| {
        prototype.name.eq_ignore_ascii_case(&unit.proto_object_name)
            || prototype
                .name
                .eq_ignore_ascii_case(unit.logical_proto_object_name())
    })
}

pub(super) fn infection_mapping<'a>(
    database: &'a Database,
    unit: &crate::entities::Unit,
) -> Option<&'a InfectionMapEntry> {
    let map = database.game_data.as_ref()?.infection_map.as_ref()?;
    map.entries.iter().find(|entry| {
        entry.base.eq_ignore_ascii_case(&unit.proto_object_name)
            || entry
                .base
                .eq_ignore_ascii_case(unit.logical_proto_object_name())
    })
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
