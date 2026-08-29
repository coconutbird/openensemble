//! World-level unit and proto-squad ammunition reconciliation.

use super::World;
use crate::entities::UnitAmmunition;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[derive(Debug, Clone, Copy)]
pub(super) struct AmmunitionSnapshot {
    unit_id: EntityId,
    base_maximum: f32,
    base_regeneration_rate: f32,
}

impl World {
    /// Return a copy of one unit's synchronized ammunition state.
    #[must_use]
    pub fn unit_ammunition(&self, unit_id: EntityId) -> Option<UnitAmmunition> {
        self.units.get(unit_id).map(|unit| unit.ammunition)
    }

    /// Return `(current, maximum)` ammunition for one squad.
    ///
    /// Current ammunition sums surviving enabled members. Maximum ammunition
    /// comes from the live player proto squad and therefore survives member loss.
    #[must_use]
    pub fn squad_ammunition(&self, squad_id: EntityId) -> Option<(f32, f32)> {
        let squad = self.squads.get(squad_id)?;
        let current = squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .map(|unit| unit.ammunition)
            .filter(|ammunition| ammunition.is_enabled())
            .map(UnitAmmunition::current)
            .sum();
        Some((current, squad.ammunition_maximum()))
    }

    pub(crate) fn set_unit_ammunition(&mut self, unit_id: EntityId, amount: f32) -> bool {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        unit.ammunition.set_current(amount);
        true
    }

    pub(crate) fn set_squad_ammunition_amount(&mut self, squad_id: EntityId, amount: f32) -> bool {
        let Some((_, maximum)) = self.squad_ammunition(squad_id) else {
            return false;
        };
        if maximum < f32::EPSILON {
            return true;
        }
        self.set_squad_ammunition_percentage(squad_id, amount / maximum)
    }

    pub(crate) fn set_squad_ammunition_percentage(
        &mut self,
        squad_id: EntityId,
        percentage: f32,
    ) -> bool {
        let Some(unit_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return false;
        };
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.ammunition
                    .set_current(unit.ammunition.maximum() * percentage);
            }
        }
        true
    }

    pub(crate) fn refresh_squad_ammunition(&mut self, squad_id: EntityId, database: &Database) {
        let Some((player_id, logical_name)) = self
            .squads
            .get(squad_id)
            .map(|squad| (squad.base.player_id, squad.proto_squad_name.clone()))
        else {
            return;
        };
        let effective_name = self
            .get_player(player_id)
            .map_or(logical_name.as_str(), |player| {
                player.technologies.resolved_squad_prototype(&logical_name)
            });
        let maximum = find_squad(database, effective_name).map_or(0.0, |prototype| {
            self.proto_squad_ammunition(player_id, prototype, database)
        });
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.set_ammunition_maximum(maximum);
        }
    }

    pub(super) fn ammunition_snapshots(
        &self,
        player_id: PlayerId,
        database: &Database,
    ) -> Vec<AmmunitionSnapshot> {
        self.units
            .iter()
            .filter(|(_, unit)| unit.base.player_id == player_id)
            .filter_map(|(unit_id, unit)| {
                let prototype = find_object(database, &unit.proto_object_name)?;
                Some(AmmunitionSnapshot {
                    unit_id,
                    base_maximum: finite_or_zero(prototype.ammo_max),
                    base_regeneration_rate: finite_or_zero(prototype.ammo_regen_rate),
                })
            })
            .collect()
    }

    pub(super) fn reconcile_player_ammunition(
        &mut self,
        player_id: PlayerId,
        snapshots: &[AmmunitionSnapshot],
        database: &Database,
    ) {
        for snapshot in snapshots {
            let Some(logical_name) = self
                .units
                .get(snapshot.unit_id)
                .map(|unit| unit.logical_proto_object_name().to_owned())
            else {
                continue;
            };
            let Some(player) = self.get_player(player_id) else {
                continue;
            };
            let maximum = player
                .technologies
                .ammunition_maximum(&logical_name, snapshot.base_maximum);
            let rate = player
                .technologies
                .ammunition_regeneration_rate(&logical_name, snapshot.base_regeneration_rate);
            if let Some(unit) = self.units.get_mut(snapshot.unit_id) {
                unit.ammunition.reconcile_profile(maximum, rate);
            }
        }
        self.refresh_player_squad_ammunition(player_id, database);
    }

    pub(crate) fn refresh_player_squad_ammunition(
        &mut self,
        player_id: PlayerId,
        database: &Database,
    ) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| (squad.base.player_id == player_id).then_some(squad_id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.refresh_squad_ammunition(squad_id, database);
        }
    }

    fn proto_squad_ammunition(
        &self,
        player_id: PlayerId,
        prototype: &ProtoSquad,
        database: &Database,
    ) -> f32 {
        let Some(entries) = prototype.units.as_ref().map(|units| &units.entries) else {
            return 0.0;
        };
        entries
            .iter()
            .filter_map(|entry| {
                let object = find_object(database, entry.proto_object.trim())?;
                let base = finite_or_zero(object.ammo_max);
                let maximum = self.get_player(player_id).map_or(base, |player| {
                    player.technologies.ammunition_maximum(&object.name, base)
                });
                let count = f32::from(u16::try_from(entry.count.max(0)).unwrap_or(u16::MAX));
                Some(maximum * count)
            })
            .sum()
    }
}

fn find_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name.trim()))
}

fn find_squad<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoSquad> {
    database
        .squads
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name.trim()))
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}
