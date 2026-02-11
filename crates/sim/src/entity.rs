//! Entity system with deterministic iteration order.
//!
//! For network sync, entities must be iterated in the same order on all clients.
//! This module provides entity storage with guaranteed iteration order.

use crate::entity_id::{EntityClass, EntityId};
use std::collections::BTreeMap;

/// Entity trait for simulation objects.
pub trait Entity {
    /// Get the entity's unique ID.
    fn id(&self) -> EntityId;

    /// Get the entity's class.
    fn class(&self) -> EntityClass {
        self.id().class().unwrap_or(EntityClass::Object)
    }

    /// Update the entity for one simulation tick.
    fn update(&mut self, dt: f32);

    /// Check if the entity is alive/active.
    fn is_alive(&self) -> bool;
}

/// Entity manager with deterministic iteration order.
///
/// Uses BTreeMap to ensure entities are always iterated in ID order.
#[derive(Debug)]
pub struct EntityManager<T> {
    /// Entities stored by ID (BTreeMap for deterministic order).
    entities: BTreeMap<u32, T>,
    /// Next available index for each entity class.
    next_index: [u32; 7],
    /// Entity class for this manager.
    entity_class: EntityClass,
}

impl<T> Default for EntityManager<T> {
    fn default() -> Self {
        Self::new(EntityClass::Object)
    }
}

impl<T> EntityManager<T> {
    /// Create a new entity manager for the given class.
    pub fn new(entity_class: EntityClass) -> Self {
        Self {
            entities: BTreeMap::new(),
            next_index: [0; 7],
            entity_class,
        }
    }

    /// Allocate a new entity ID.
    pub fn allocate_id(&mut self) -> EntityId {
        let class_idx = self.entity_class as usize;
        let index = self.next_index[class_idx];
        self.next_index[class_idx] = index.wrapping_add(1);
        EntityId::new(self.entity_class, index)
    }

    /// Insert an entity with a specific ID.
    pub fn insert(&mut self, id: EntityId, entity: T) {
        self.entities.insert(id.as_u32(), entity);
    }

    /// Insert an entity and return its allocated ID.
    pub fn insert_new(&mut self, entity: T) -> EntityId {
        let id = self.allocate_id();
        self.insert(id, entity);
        id
    }

    /// Remove an entity by ID.
    pub fn remove(&mut self, id: EntityId) -> Option<T> {
        self.entities.remove(&id.as_u32())
    }

    /// Get an entity by ID.
    pub fn get(&self, id: EntityId) -> Option<&T> {
        self.entities.get(&id.as_u32())
    }

    /// Get a mutable entity by ID.
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut T> {
        self.entities.get_mut(&id.as_u32())
    }

    /// Check if an entity exists.
    pub fn contains(&self, id: EntityId) -> bool {
        self.entities.contains_key(&id.as_u32())
    }

    /// Get the number of entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Iterate over entities in deterministic order (by ID).
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.entities
            .iter()
            .map(|(&id, entity)| (EntityId::from_u32(id), entity))
    }

    /// Iterate mutably over entities in deterministic order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.entities
            .iter_mut()
            .map(|(&id, entity)| (EntityId::from_u32(id), entity))
    }

    /// Get all entity IDs in deterministic order.
    pub fn ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.keys().map(|&id| EntityId::from_u32(id))
    }

    /// Clear all entities.
    pub fn clear(&mut self) {
        self.entities.clear();
    }
}

/// Update all entities that implement the Entity trait.
impl<T: Entity> EntityManager<T> {
    /// Update all entities and remove dead ones.
    pub fn update_all(&mut self, dt: f32) {
        // Collect IDs of dead entities
        let dead: Vec<u32> = self
            .entities
            .iter()
            .filter(|(_, e)| !e.is_alive())
            .map(|(&id, _)| id)
            .collect();

        // Remove dead entities
        for id in dead {
            self.entities.remove(&id);
        }

        // Update remaining entities
        for entity in self.entities.values_mut() {
            entity.update(dt);
        }
    }
}
