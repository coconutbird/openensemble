//! Authoritative state for retail hint callouts.

use std::collections::BTreeMap;

use glam::Vec3;

use super::super::World;
use crate::entity_id::{EntityClass, EntityId};
use crate::sync::SyncChecksum;

const MAX_HINT_CALLOUTS: usize = 5;

/// The world-space source followed by one retail hint callout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HintCalloutAnchor {
    /// A fixed authored world location.
    Location(Vec3),
    /// A live unit or squad whose current transform is resolved by the UI.
    Entity(EntityId),
}

/// One active retail `BUICallout` descriptor owned by the simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HintCallout {
    id: i32,
    widget_slot: u8,
    string_id: i32,
    anchor: HintCalloutAnchor,
}

impl HintCallout {
    /// Trigger-visible monotonically allocated identifier.
    #[must_use]
    pub const fn id(self) -> i32 {
        self.id
    }

    /// One of retail's five reusable Flash widget slots.
    #[must_use]
    pub const fn widget_slot(self) -> u8 {
        self.widget_slot
    }

    /// Localized string-table identifier displayed by the UI.
    #[must_use]
    pub const fn string_id(self) -> i32 {
        self.string_id
    }

    /// Fixed location or live entity followed by the renderer/UI adapter.
    #[must_use]
    pub const fn anchor(self) -> HintCalloutAnchor {
        self.anchor
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct HintCalloutState {
    next_id: i32,
    active: BTreeMap<i32, HintCallout>,
}

impl World {
    /// Iterate active hint callouts in stable trigger-ID order.
    pub fn hint_callouts(&self) -> impl Iterator<Item = &HintCallout> {
        self.presentation_control.callouts.active.values()
    }

    /// Look up one active hint callout by its trigger-visible ID.
    #[must_use]
    pub fn hint_callout(&self, id: i32) -> Option<&HintCallout> {
        self.presentation_control.callouts.active.get(&id)
    }

    pub(crate) fn create_location_hint_callout(&mut self, location: Vec3, string_id: i32) -> i32 {
        self.insert_hint_callout(HintCalloutAnchor::Location(location), string_id)
    }

    pub(crate) fn create_entity_hint_callout(
        &mut self,
        entity_id: EntityId,
        string_id: i32,
        clear_parent_squad: bool,
    ) -> i32 {
        self.remove_hint_callouts_for_anchor(HintCalloutAnchor::Entity(entity_id));
        if clear_parent_squad
            && entity_id.class() == Some(EntityClass::Unit)
            && let Some(parent_id) = self.get_unit(entity_id).and_then(|unit| unit.squad_id)
        {
            self.remove_hint_callouts_for_anchor(HintCalloutAnchor::Entity(parent_id));
        }
        self.insert_hint_callout(HintCalloutAnchor::Entity(entity_id), string_id)
    }

    pub(crate) fn remove_hint_callout(&mut self, id: i32) -> bool {
        self.presentation_control
            .callouts
            .active
            .remove(&id)
            .is_some()
    }

    pub(crate) fn remove_hint_callouts_for_entity(&mut self, entity_id: EntityId) {
        self.remove_hint_callouts_for_anchor(HintCalloutAnchor::Entity(entity_id));
    }

    fn insert_hint_callout(&mut self, anchor: HintCalloutAnchor, string_id: i32) -> i32 {
        let id = self.presentation_control.callouts.allocate_id();
        let Some(widget_slot) = self.presentation_control.callouts.available_widget() else {
            return -1;
        };
        if !self.valid_hint_callout_anchor(anchor) {
            return -1;
        }
        self.presentation_control.callouts.active.insert(
            id,
            HintCallout {
                id,
                widget_slot,
                string_id,
                anchor,
            },
        );
        id
    }

    fn valid_hint_callout_anchor(&self, anchor: HintCalloutAnchor) -> bool {
        match anchor {
            HintCalloutAnchor::Location(_) => true,
            HintCalloutAnchor::Entity(entity_id) => match entity_id.class() {
                Some(EntityClass::Unit) => self.get_unit(entity_id).is_some(),
                Some(EntityClass::Squad) => self.get_squad(entity_id).is_some(),
                _ => false,
            },
        }
    }

    fn remove_hint_callouts_for_anchor(&mut self, anchor: HintCalloutAnchor) {
        self.presentation_control
            .callouts
            .active
            .retain(|_, callout| callout.anchor != anchor);
    }
}

impl HintCalloutState {
    fn allocate_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    fn available_widget(&self) -> Option<u8> {
        (0..MAX_HINT_CALLOUTS)
            .find(|slot| {
                self.active
                    .values()
                    .all(|callout| usize::from(callout.widget_slot) != *slot)
            })
            .and_then(|slot| u8::try_from(slot).ok())
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.next_id);
        checksum.hash_u32(u32::try_from(self.active.len()).unwrap_or(u32::MAX));
        for callout in self.active.values().copied() {
            checksum.hash_i32(callout.id);
            checksum.hash_u32(u32::from(callout.widget_slot));
            checksum.hash_i32(callout.string_id);
            match callout.anchor {
                HintCalloutAnchor::Location(location) => {
                    checksum.hash_u32(0);
                    checksum.hash_vec3(location.x, location.y, location.z);
                }
                HintCalloutAnchor::Entity(entity_id) => {
                    checksum.hash_u32(1);
                    checksum.hash_u32(entity_id.as_u32());
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "callouts/tests.rs"]
mod tests;
