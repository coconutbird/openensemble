//! In-place static unit replacements selected by database death metadata.

use super::World;
use crate::entities::{SquadMode, SquadState, UnitKind};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::GAIA_PLAYER;
use crate::scenario::{PlacedUnitKind, classify_proto_object, configure_unit_from_proto};
use pipeline::database::hw1::{Database, ProtoObject};

#[derive(Debug)]
struct DeathReplacementRequest {
    source_name: String,
    target_name: String,
    target_index: usize,
    target_kind: UnitKind,
    kind: DeathReplacementKind,
    flags: DeathReplacementFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeathReplacementKind {
    Static,
    Shatter,
}

#[derive(Debug, Clone, Copy, Default)]
struct DeathReplacementFlags(u8);

impl DeathReplacementFlags {
    const DAMAGED: u8 = 1 << 0;
    const FORCE_GAIA: u8 = 1 << 1;
    const INVULNERABLE: u8 = 1 << 2;
    const INVULNERABLE_WHEN_GAIA: u8 = 1 << 3;

    fn from_target(target: &ProtoObject) -> Self {
        let mut flags = 0;
        for (name, flag) in [
            ("DamagedDeathReplacement", Self::DAMAGED),
            ("ForceToGaiaPlayer", Self::FORCE_GAIA),
            ("Invulnerable", Self::INVULNERABLE),
            ("InvulnerableWhenGaia", Self::INVULNERABLE_WHEN_GAIA),
        ] {
            if has_flag(target, name) {
                flags |= flag;
            }
        }
        Self(flags)
    }

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

impl World {
    /// Transform regular deaths before death actions and dead-pool cleanup run.
    pub(super) fn resolve_dead_unit_death_replacements(
        &mut self,
        database: &Database,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let dead_units = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (!unit.is_alive() && !unit.is_static_death_replacement()).then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in dead_units {
            let Some(request) = self.death_replacement_request(unit_id, database) else {
                continue;
            };
            self.execute_death_replacement(unit_id, &request, database, gameplay);
        }
    }

    fn death_replacement_request(
        &self,
        unit_id: EntityId,
        database: &Database,
    ) -> Option<DeathReplacementRequest> {
        let source = self.get_unit(unit_id)?;
        let source_proto = find_proto_object(database, &source.proto_object_name)?.1;
        let shatter_replacement = has_flag(source_proto, "ShatterDeathReplacement");
        if shatter_replacement && !source.is_shatter_on_death() {
            return None;
        }
        let target_name = source_proto.death_replacement.as_deref()?.trim();
        if target_name.is_empty() {
            return None;
        }
        let (target_index, target) = find_proto_object(database, target_name)?;
        let target_kind = classify_proto_object(target).map_or(source.kind, |kind| match kind {
            PlacedUnitKind::Mobile => UnitKind::Mobile,
            PlacedUnitKind::Building => UnitKind::Building,
        });
        Some(DeathReplacementRequest {
            source_name: source.proto_object_name.clone(),
            target_name: target.name.trim().to_owned(),
            target_index,
            target_kind,
            kind: if shatter_replacement {
                DeathReplacementKind::Shatter
            } else {
                DeathReplacementKind::Static
            },
            flags: DeathReplacementFlags::from_target(target),
        })
    }

    fn execute_death_replacement(
        &mut self,
        unit_id: EntityId,
        request: &DeathReplacementRequest,
        database: &Database,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let Some(source) = self.get_unit(unit_id).cloned() else {
            return;
        };
        let ammunition_ratio = if source.ammunition.maximum() > 0.0 {
            source.ammunition.current() / source.ammunition.maximum()
        } else {
            1.0
        };
        let Some(target) = database.objects.get(request.target_index) else {
            return;
        };
        self.prepare_remove_unit_garrison(unit_id);
        self.refund_production_for_removed_unit(&source);
        self.deactivate_unit_built_economy(unit_id);
        let now_ms = self.game_time_ms;
        let Some(unit) = self.get_unit_mut(unit_id) else {
            return;
        };
        unit.reset_for_prototype_transform(request.target_kind, now_ms);
        configure_unit_from_proto(
            self,
            unit_id,
            &request.target_name,
            request.target_index,
            target,
        );
        if let Some(unit) = self.get_unit_mut(unit_id)
            && ammunition_ratio.is_finite()
            && unit.ammunition.maximum() > 0.0
        {
            unit.ammunition
                .set_current(unit.ammunition.maximum() * ammunition_ratio);
        }
        if let Some(gameplay) = gameplay {
            let _configured = self.configure_unit_revival(unit_id, gameplay);
        }
        let squad_id = self.get_unit(unit_id).and_then(|unit| unit.squad_id);
        if let Some(squad_id) = squad_id {
            crate::scenario::placed::transform_synthetic_squad(
                self,
                squad_id,
                &request.source_name,
                target,
                database,
            );
        }
        let force_gaia = request.flags.contains(DeathReplacementFlags::FORCE_GAIA);
        if request.kind == DeathReplacementKind::Static {
            self.retain_death_replacement_squad(squad_id, force_gaia);
        } else if force_gaia {
            self.normalize_death_replacement_squad(squad_id);
        }
        if force_gaia {
            self.transfer_death_replacement_to_gaia(unit_id, squad_id);
        }
        let final_owner = self
            .get_unit(unit_id)
            .map_or(source.base.player_id, |unit| unit.base.player_id);
        let persistently_invulnerable = request.flags.contains(DeathReplacementFlags::INVULNERABLE)
            || (final_owner == GAIA_PLAYER
                && request
                    .flags
                    .contains(DeathReplacementFlags::INVULNERABLE_WHEN_GAIA));
        if let Some(unit) = self.get_unit_mut(unit_id) {
            unit.set_invulnerable(persistently_invulnerable);
            if request.kind == DeathReplacementKind::Static {
                unit.retain_static_death_replacement(
                    request.flags.contains(DeathReplacementFlags::DAMAGED),
                    persistently_invulnerable,
                );
            }
        }
        self.recompute_unit_base_child_damage(unit_id);
    }

    fn retain_death_replacement_squad(&mut self, squad_id: Option<EntityId>, force_normal: bool) {
        let Some(squad_id) = squad_id else {
            return;
        };
        let Some(squad) = self.get_squad_mut(squad_id) else {
            return;
        };
        if !squad.is_alive() {
            squad.base.alive = true;
            squad.state = SquadState::Idle;
        }
        if force_normal {
            squad.mode = SquadMode::Normal;
        }
    }

    fn normalize_death_replacement_squad(&mut self, squad_id: Option<EntityId>) {
        if let Some(squad) = squad_id.and_then(|squad_id| self.get_squad_mut(squad_id)) {
            squad.mode = SquadMode::Normal;
        }
    }

    fn transfer_death_replacement_to_gaia(
        &mut self,
        unit_id: EntityId,
        squad_id: Option<EntityId>,
    ) {
        if let Some(squad_id) = squad_id {
            let _changed = self.change_squad_owner(squad_id, GAIA_PLAYER);
        } else {
            let _changed = self.change_unit_owner(unit_id, GAIA_PLAYER);
        }
    }
}

fn find_proto_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<(usize, &'database ProtoObject)> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name))
}

fn has_flag(proto: &ProtoObject, expected: &str) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests;
