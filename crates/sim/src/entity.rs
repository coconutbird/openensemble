//! Deterministic, generation-checked entity storage.
//!
//! Vanilla `BObjectManager` keeps a separate indexed pool for every entity
//! class. Destruction increments a 12-bit generation and makes the low-16-bit
//! slot available for reuse. This module mirrors those externally visible
//! semantics while using safe Rust collections.

use crate::entity_id::{EntityClass, EntityId};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum high-water mark accepted by vanilla world save loading.
pub const MAX_ENTITY_SLOTS: usize = 20_000;

const GENERATION_MASK: u16 = 0x0FFF;

/// Common behavior for simulation entities.
pub trait Entity {
    /// Get the entity's unique ID.
    fn id(&self) -> EntityId;

    /// Get the entity's class.
    fn class(&self) -> EntityClass {
        self.id().class().unwrap_or(EntityClass::Object)
    }

    /// Update the entity for one simulation tick.
    fn update(&mut self, dt: f32);

    /// Check whether the entity remains active.
    fn is_alive(&self) -> bool;
}

/// A deterministic entity pool for one vanilla entity class.
///
/// Entities are keyed by their 16-bit pool index. All public lookup operations
/// validate both the class and generation in the supplied [`EntityId`].
#[derive(Debug)]
pub struct EntityManager<T> {
    entities: BTreeMap<u16, T>,
    generations: Vec<u16>,
    free_indices: BTreeSet<u16>,
    next_index: u16,
    entity_class: EntityClass,
}

impl<T> Default for EntityManager<T> {
    fn default() -> Self {
        Self::new(EntityClass::Object)
    }
}

impl<T> EntityManager<T> {
    /// Create an empty pool for `entity_class`.
    #[must_use]
    pub fn new(entity_class: EntityClass) -> Self {
        Self {
            entities: BTreeMap::new(),
            generations: Vec::new(),
            free_indices: BTreeSet::new(),
            next_index: 0,
            entity_class,
        }
    }

    /// Get the class stored by this pool.
    #[must_use]
    pub fn entity_class(&self) -> EntityClass {
        self.entity_class
    }

    /// Allocate the lowest available pool slot.
    ///
    /// # Panics
    ///
    /// Panics after the vanilla 20,000-slot high-water limit is reached.
    pub fn allocate_id(&mut self) -> EntityId {
        let index = self.free_indices.pop_first().unwrap_or_else(|| {
            assert!(
                usize::from(self.next_index) < MAX_ENTITY_SLOTS,
                "vanilla entity pool capacity exceeded"
            );
            let index = self.next_index;
            self.next_index += 1;
            self.generations.push(0);
            index
        });
        EntityId::with_generation(
            self.entity_class,
            index,
            self.generations[usize::from(index)],
        )
    }

    /// Insert an entity with a newly allocated or explicitly restored ID.
    ///
    /// Explicit IDs are used by vanilla save loading. Any skipped slots become
    /// available to the normal lowest-free-slot allocator.
    ///
    /// # Panics
    ///
    /// Panics for a mismatched class, an out-of-range slot, or an occupied slot.
    pub fn insert(&mut self, id: EntityId, entity: T) {
        self.reserve_explicit_id(id);
        assert!(
            self.entities.insert(id.pool_index(), entity).is_none(),
            "entity pool slot is already occupied"
        );
    }

    /// Insert an entity and return its allocated ID.
    pub fn insert_new(&mut self, entity: T) -> EntityId {
        let id = self.allocate_id();
        self.insert(id, entity);
        id
    }

    /// Remove an entity after validating its class and generation.
    pub fn remove(&mut self, id: EntityId) -> Option<T> {
        if !self.matches_current_generation(id) {
            return None;
        }
        let removed = self.entities.remove(&id.pool_index())?;
        let generation = &mut self.generations[usize::from(id.pool_index())];
        *generation = generation.wrapping_add(1) & GENERATION_MASK;
        self.free_indices.insert(id.pool_index());
        Some(removed)
    }

    /// Get an entity after validating its class and generation.
    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<&T> {
        self.matches_current_generation(id)
            .then(|| self.entities.get(&id.pool_index()))
            .flatten()
    }

    /// Mutably get an entity after validating its class and generation.
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut T> {
        if !self.matches_current_generation(id) {
            return None;
        }
        self.entities.get_mut(&id.pool_index())
    }

    /// Check whether a current entity exists for `id`.
    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.get(id).is_some()
    }

    /// Get the number of occupied slots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Check whether the pool is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Iterate in deterministic pool-index order.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.entities.iter().map(|(&index, entity)| {
            let id = self.id_for_index(index);
            (id, entity)
        })
    }

    /// Mutably iterate in deterministic pool-index order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        let class = self.entity_class;
        let generations = &self.generations;
        self.entities.iter_mut().map(move |(&index, entity)| {
            let id = EntityId::with_generation(class, index, generations[usize::from(index)]);
            (id, entity)
        })
    }

    /// Iterate over current IDs in deterministic pool-index order.
    pub fn ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.keys().map(|&index| self.id_for_index(index))
    }

    /// Remove every entity, invalidating all previously issued IDs.
    pub fn clear(&mut self) {
        let ids: Vec<_> = self.ids().collect();
        for id in ids {
            let _removed = self.remove(id);
        }
    }

    fn id_for_index(&self, index: u16) -> EntityId {
        EntityId::with_generation(
            self.entity_class,
            index,
            self.generations[usize::from(index)],
        )
    }

    fn matches_current_generation(&self, id: EntityId) -> bool {
        id.class() == Some(self.entity_class)
            && self
                .generations
                .get(usize::from(id.pool_index()))
                .is_some_and(|&generation| generation == id.generation())
    }

    fn reserve_explicit_id(&mut self, id: EntityId) {
        assert_eq!(
            id.class(),
            Some(self.entity_class),
            "entity ID class does not match its pool"
        );
        let index = id.pool_index();
        assert!(
            usize::from(index) < MAX_ENTITY_SLOTS,
            "entity pool index exceeds vanilla capacity"
        );
        self.extend_to_include(index);
        assert!(
            !self.entities.contains_key(&index),
            "entity pool slot is already occupied"
        );
        self.generations[usize::from(index)] = id.generation();
        self.free_indices.remove(&index);
    }

    fn extend_to_include(&mut self, index: u16) {
        while self.next_index <= index {
            let new_index = self.next_index;
            self.next_index += 1;
            self.generations.push(0);
            self.free_indices.insert(new_index);
        }
    }
}

impl<T: Entity> EntityManager<T> {
    /// Update all entities, then remove and return the IDs of dead entities.
    #[must_use]
    pub fn update_all(&mut self, dt: f32) -> Vec<EntityId> {
        for entity in self.entities.values_mut() {
            entity.update(dt);
        }
        let dead: Vec<_> = self
            .iter()
            .filter_map(|(id, entity)| (!entity.is_alive()).then_some(id))
            .collect();
        for &id in &dead {
            let _removed = self.remove(id);
        }
        dead
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestEntity {
        id: EntityId,
        alive: bool,
    }

    impl Entity for TestEntity {
        fn id(&self) -> EntityId {
            self.id
        }

        fn update(&mut self, _dt: f32) {}

        fn is_alive(&self) -> bool {
            self.alive
        }
    }

    #[test]
    fn reuses_lowest_slot_with_new_generation() {
        let mut entities = EntityManager::new(EntityClass::Unit);
        let old_id = entities.allocate_id();
        entities.insert(
            old_id,
            TestEntity {
                id: old_id,
                alive: true,
            },
        );

        assert!(entities.remove(old_id).is_some());
        let new_id = entities.allocate_id();

        assert_eq!(new_id.pool_index(), old_id.pool_index());
        assert_eq!(new_id.generation(), old_id.generation() + 1);
        assert!(entities.get(old_id).is_none());
    }

    #[test]
    fn rejects_wrong_class_and_stale_generation() {
        let mut entities = EntityManager::new(EntityClass::Squad);
        let id = entities.allocate_id();
        entities.insert(id, TestEntity { id, alive: true });

        let wrong_class =
            EntityId::with_generation(EntityClass::Unit, id.pool_index(), id.generation());
        let stale =
            EntityId::with_generation(EntityClass::Squad, id.pool_index(), id.generation() + 1);

        assert!(entities.get(wrong_class).is_none());
        assert!(entities.get(stale).is_none());
    }

    #[test]
    fn explicit_restore_exposes_lower_gaps_to_allocator() {
        let mut entities = EntityManager::new(EntityClass::Unit);
        let restored = EntityId::with_generation(EntityClass::Unit, 4, 7);
        entities.insert(
            restored,
            TestEntity {
                id: restored,
                alive: true,
            },
        );

        assert_eq!(entities.allocate_id().pool_index(), 0);
        assert!(entities.contains(restored));
    }
}
