//! Per-unit containment state and immutable container capabilities.

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// Retail-style unit containment state.
///
/// A container unit owns sorted references to every contained unit. A contained
/// unit retains the inverse reference so systems can exclude it from ordinary
/// movement, collision, targeting, and presentation without reconstructing
/// state from squad relationships.
#[derive(Debug, Clone, Default)]
pub struct UnitGarrison {
    container_id: Option<EntityId>,
    contained_unit_ids: Vec<EntityId>,
    accepted_object_types: Vec<String>,
    maximum_population: f32,
    can_contain: bool,
    one_squad_containment: bool,
    teleporter: bool,
}

impl UnitGarrison {
    /// Build a container capability for tests, tools, and custom content.
    #[must_use]
    pub fn container(
        maximum_population: f32,
        one_squad_containment: bool,
        teleporter: bool,
        accepted_object_types: Vec<String>,
    ) -> Self {
        let mut state = Self::default();
        state.configure_container(
            maximum_population,
            one_squad_containment,
            teleporter,
            accepted_object_types,
        );
        state
    }

    /// Unit currently containing this unit, if any.
    #[must_use]
    pub const fn container_id(&self) -> Option<EntityId> {
        self.container_id
    }

    /// Return whether this unit is currently contained.
    #[must_use]
    pub const fn is_contained(&self) -> bool {
        self.container_id.is_some()
    }

    /// Sorted unit IDs currently contained by this unit.
    #[must_use]
    pub fn contained_unit_ids(&self) -> &[EntityId] {
        &self.contained_unit_ids
    }

    /// Return whether authored data permits this unit to contain units.
    #[must_use]
    pub const fn can_contain(&self) -> bool {
        self.can_contain
    }

    /// Return whether only one logical squad may be inside at a time.
    #[must_use]
    pub const fn one_squad_containment(&self) -> bool {
        self.one_squad_containment
    }

    /// Return whether this is a retail teleporter pickup.
    #[must_use]
    pub const fn is_teleporter(&self) -> bool {
        self.teleporter
    }

    /// Maximum first-bucket population, or zero for unlimited capacity.
    #[must_use]
    pub const fn maximum_population(&self) -> f32 {
        self.maximum_population
    }

    /// Authored object-type filters accepted by this container.
    #[must_use]
    pub fn accepted_object_types(&self) -> &[String] {
        &self.accepted_object_types
    }

    pub(crate) fn configure_container(
        &mut self,
        maximum_population: f32,
        one_squad_containment: bool,
        teleporter: bool,
        mut accepted_object_types: Vec<String>,
    ) {
        accepted_object_types.sort_by_key(|value| value.to_ascii_lowercase());
        accepted_object_types.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        self.accepted_object_types = accepted_object_types;
        self.maximum_population = if maximum_population.is_finite() && maximum_population > 0.0 {
            maximum_population
        } else {
            0.0
        };
        self.can_contain = true;
        self.one_squad_containment = one_squad_containment;
        self.teleporter = teleporter;
    }

    pub(crate) fn set_container(&mut self, container_id: Option<EntityId>) {
        self.container_id = container_id.filter(|id| !id.is_invalid());
    }

    pub(crate) fn add_contained_unit(&mut self, unit_id: EntityId) -> bool {
        match self.contained_unit_ids.binary_search(&unit_id) {
            Ok(_) => false,
            Err(index) => {
                self.contained_unit_ids.insert(index, unit_id);
                true
            }
        }
    }

    pub(crate) fn remove_contained_unit(&mut self, unit_id: EntityId) -> bool {
        let Ok(index) = self.contained_unit_ids.binary_search(&unit_id) else {
            return false;
        };
        self.contained_unit_ids.remove(index);
        true
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            self.container_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_u32(u32::try_from(self.contained_unit_ids.len()).unwrap_or(u32::MAX));
        for id in &self.contained_unit_ids {
            checksum.hash_u32(id.as_u32());
        }
        checksum.hash_u32(u32::try_from(self.accepted_object_types.len()).unwrap_or(u32::MAX));
        for object_type in &self.accepted_object_types {
            checksum.hash_u32(u32::try_from(object_type.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(object_type.as_bytes());
        }
        checksum.hash_f32(self.maximum_population);
        checksum.hash_u32(u32::from(self.can_contain));
        checksum.hash_u32(u32::from(self.one_squad_containment));
        checksum.hash_u32(u32::from(self.teleporter));
    }
}
