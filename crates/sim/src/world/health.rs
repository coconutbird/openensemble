//! Authoritative unit health queries and direct scripted mutation.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;

/// One unit's current and player-modified maximum health values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitHealth {
    pub hitpoints: f32,
    pub maximum_hitpoints: f32,
    pub shieldpoints: f32,
    pub maximum_shieldpoints: f32,
}

impl World {
    /// Read the health values used by retail trigger aggregation.
    #[must_use]
    pub fn unit_health(&self, unit_id: EntityId) -> Option<UnitHealth> {
        self.units.get(unit_id).map(|unit| UnitHealth {
            hitpoints: unit.hitpoints,
            maximum_hitpoints: unit.max_hitpoints,
            shieldpoints: unit.shields.current,
            maximum_shieldpoints: unit.shields.maximum,
        })
    }

    /// Add direct scripted HP and shield values to a live or healing replacement.
    ///
    /// This intentionally does not emit a combat-damage event or revive a
    /// killed unit. Retail's `teRepair` clamps only at each maximum.
    pub fn repair_unit(&mut self, unit_id: EntityId, hitpoints: f32, shields: f32) -> bool {
        if !hitpoints.is_finite() || !shields.is_finite() {
            return false;
        }
        let Some(unit) = self
            .units
            .get_mut(unit_id)
            .filter(|unit| unit.is_alive() || unit.is_death_replacement_healing())
        else {
            return false;
        };
        unit.hitpoints = (unit.hitpoints + hitpoints).min(unit.max_hitpoints);
        unit.shields.current = (unit.shields.current + shields).min(unit.shields.maximum);
        let _finished = unit.finish_death_replacement_healing();
        true
    }

    /// Subtract direct scripted HP and shield values from a live unit.
    ///
    /// Unlike [`World::damage_unit`], the channels are independent and do not
    /// emit a damage event. Retail's `teDamage` clamps only at zero and does
    /// not clear the unit's explicit alive flag when HP reaches zero.
    pub fn damage_unit_direct(&mut self, unit_id: EntityId, hitpoints: f32, shields: f32) -> bool {
        if !hitpoints.is_finite() || !shields.is_finite() {
            return false;
        }
        let Some(unit) = self.units.get_mut(unit_id).filter(|unit| unit.is_alive()) else {
            return false;
        };
        unit.hitpoints = (unit.hitpoints - hitpoints).max(0.0);
        unit.shields.current = (unit.shields.current - shields).max(0.0);
        true
    }
}
