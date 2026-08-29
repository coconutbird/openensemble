//! Player base ownership records.
//!
//! Bases are not a separate vanilla `BEntityID` class. Instead, buildings live
//! in the `BUnit` pool and carry a small `mBaseNumber`. The simulation mirrors
//! that relationship with a compact [`BaseId`] and a deterministic set of
//! building entity IDs.

use crate::entity_id::{EntityClass, EntityId};
use crate::player::PlayerId;
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet};

/// Identifier for a player's base record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BaseId(pub u16);

impl BaseId {
    /// Create a base ID from its numeric value.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Get the serialized numeric value.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }
}

/// A base anchored by one building and optionally containing more buildings.
#[derive(Debug, Clone)]
pub struct Base {
    /// Base number.
    pub id: BaseId,
    /// Owning player.
    pub player_id: PlayerId,
    /// Building whose creation established this base.
    pub anchor_building_id: EntityId,
    /// Base origin, initially taken from the anchor building.
    pub position: Vec3,
    building_ids: BTreeSet<EntityId>,
    pub(crate) plasma_shield: BasePlasmaShield,
}

/// Authoritative lifecycle state for a base's persistent plasma shield.
#[derive(Debug, Clone, Default)]
pub(crate) struct BasePlasmaShield {
    pub primary_generator_id: Option<EntityId>,
    pub shield_squad_id: Option<EntityId>,
    pub rebuild_remaining: f32,
    pub attack_wait_remaining: f32,
    pub subshield_squads: BTreeMap<EntityId, EntityId>,
}

impl Base {
    /// Create a base record around an anchor building.
    ///
    /// # Panics
    ///
    /// Panics if `anchor_building_id` is not a unit-pool ID.
    #[must_use]
    pub fn new(
        id: BaseId,
        player_id: PlayerId,
        anchor_building_id: EntityId,
        position: Vec3,
    ) -> Self {
        assert_eq!(
            anchor_building_id.class(),
            Some(EntityClass::Unit),
            "base anchors must live in the unit pool"
        );
        Self {
            id,
            player_id,
            anchor_building_id,
            position,
            building_ids: BTreeSet::from([anchor_building_id]),
            plasma_shield: BasePlasmaShield::default(),
        }
    }

    /// Check whether this base owns a building.
    #[must_use]
    pub fn contains_building(&self, id: EntityId) -> bool {
        self.building_ids.contains(&id)
    }

    /// Get the number of buildings in this base.
    #[must_use]
    pub fn building_count(&self) -> usize {
        self.building_ids.len()
    }

    /// Iterate over building IDs in deterministic entity-ID order.
    pub fn buildings(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.building_ids.iter().copied()
    }

    /// Return the generator currently coordinating this base's plasma shield.
    #[must_use]
    pub const fn primary_plasma_shield_generator(&self) -> Option<EntityId> {
        self.plasma_shield.primary_generator_id
    }

    /// Return the live or pending plasma-shield squad owned by this base.
    #[must_use]
    pub const fn plasma_shield_squad(&self) -> Option<EntityId> {
        self.plasma_shield.shield_squad_id
    }

    /// Return the remaining authored rebuild delay in seconds.
    #[must_use]
    pub const fn plasma_shield_rebuild_remaining(&self) -> f32 {
        self.plasma_shield.rebuild_remaining
    }

    /// Return the plasma subshield currently protecting one base building.
    #[must_use]
    pub fn plasma_subshield_squad(&self, building_id: EntityId) -> Option<EntityId> {
        self.plasma_shield
            .subshield_squads
            .get(&building_id)
            .copied()
    }

    pub(crate) fn add_building(&mut self, id: EntityId) {
        self.building_ids.insert(id);
    }

    pub(crate) fn remove_building(&mut self, id: EntityId) {
        self.building_ids.remove(&id);
    }
}
