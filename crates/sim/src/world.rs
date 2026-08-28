//! World state container for the simulation.
//!
//! Based on `BWorld` from the original source.

use crate::entities::squads::{formation_offset_to_local, formation_offset_to_world};
use crate::entities::{Base, BaseEntity, BaseId, Squad, Unit};
use crate::entity::{Entity, EntityManager};
use crate::entity_id::{EntityClass, EntityId};
use crate::physics::{
    prepare_squad_movement, resolve_unit_collisions, substeps, sync_squad_members,
};
use crate::player::{GAIA_PLAYER, Player, PlayerId};
use crate::random::Random;
use crate::sync::SyncChecksum;
use glam::Vec3;
use std::collections::BTreeMap;

/// Maximum supported players.
pub const MAX_PLAYERS: usize = 8;

/// World state container.
///
/// Contains all game state: players, entities, time, etc.
#[derive(Debug)]
pub struct World {
    /// All players (index = player ID).
    players: Vec<Player>,
    /// Current game time in milliseconds.
    pub game_time_ms: u32,
    /// Deterministic RNG for the world.
    pub rng: Random,
    /// Unit pool. Mobile units and buildings both use vanilla class 1.
    pub units: EntityManager<Unit>,
    /// Squad entity manager.
    pub squads: EntityManager<Squad>,
    /// Base ownership records, which are not standalone vanilla entities.
    bases: BTreeMap<BaseId, Base>,
    /// Next candidate base number.
    next_base_id: u16,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Create a new empty world.
    #[must_use]
    pub fn new() -> Self {
        Self {
            players: Vec::new(),
            game_time_ms: 0,
            rng: Random::new(),
            units: EntityManager::new(EntityClass::Unit),
            squads: EntityManager::new(EntityClass::Squad),
            bases: BTreeMap::new(),
            next_base_id: 0,
        }
    }

    /// Create a world with a specific random seed.
    #[must_use]
    pub fn with_seed(seed: u64) -> Self {
        let mut world = Self::new();
        world.rng.set_seed64(seed);
        world
    }

    /// Initialize the world with the given number of players.
    ///
    /// Creates player 0 as Gaia (neutral) and players 1..=count as active players.
    pub fn init_players(&mut self, player_count: u8) {
        self.players.clear();

        // Player 0 is always Gaia (neutral/world player)
        let mut gaia = Player::new(GAIA_PLAYER);
        gaia.name = "Gaia".to_string();
        gaia.player_type = crate::player::PlayerType::Npc;
        self.players.push(gaia);

        // Create active players
        for i in 1..=player_count {
            let player = Player::new(i);
            self.players.push(player);
        }
    }

    /// Get the number of players (including Gaia).
    #[must_use]
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Get a player by ID.
    #[must_use]
    pub fn get_player(&self, id: PlayerId) -> Option<&Player> {
        self.players.get(usize::from(id))
    }

    /// Get a mutable player by ID.
    pub fn get_player_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.players.get_mut(usize::from(id))
    }

    /// Iterate over all players.
    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.players.iter()
    }

    /// Iterate over all players mutably.
    pub fn players_mut(&mut self) -> impl Iterator<Item = &mut Player> {
        self.players.iter_mut()
    }

    /// Iterate over active (non-Gaia) players.
    pub fn active_players(&self) -> impl Iterator<Item = &Player> {
        self.players.iter().skip(1)
    }

    /// Iterate over active (non-Gaia) players mutably.
    pub fn active_players_mut(&mut self) -> impl Iterator<Item = &mut Player> {
        self.players.iter_mut().skip(1)
    }

    /// Get current game time in milliseconds.
    #[must_use]
    pub fn game_time(&self) -> u32 {
        self.game_time_ms
    }

    /// Advance game time by the given milliseconds.
    pub fn advance_time(&mut self, ms: u32) {
        self.game_time_ms = self.game_time_ms.wrapping_add(ms);
    }

    /// Reset the world to initial state.
    pub fn reset(&mut self) {
        self.players.clear();
        self.game_time_ms = 0;
        self.units.clear();
        self.squads.clear();
        self.bases.clear();
        self.next_base_id = 0;
    }

    /// Create a new squad for the given player.
    pub fn create_squad(&mut self, player_id: PlayerId) -> EntityId {
        self.create_squad_at(player_id, Vec3::ZERO)
    }

    /// Create a new squad at a specific position.
    pub fn create_squad_at(&mut self, player_id: PlayerId, position: Vec3) -> EntityId {
        let id = self.squads.allocate_id();
        let mut squad = Squad::new(id, player_id);
        squad.set_position(position);
        self.squads.insert(id, squad);
        id
    }

    /// Get a squad by ID.
    #[must_use]
    pub fn get_squad(&self, id: EntityId) -> Option<&Squad> {
        self.squads.get(id)
    }

    /// Get a mutable squad by ID.
    pub fn get_squad_mut(&mut self, id: EntityId) -> Option<&mut Squad> {
        self.squads.get_mut(id)
    }

    /// Remove a squad and detach its surviving units.
    pub fn remove_squad(&mut self, id: EntityId) -> Option<Squad> {
        let squad = self.squads.remove(id)?;
        for unit_id in &squad.unit_ids {
            if let Some(unit) = self.units.get_mut(*unit_id)
                && unit.squad_id == Some(id)
            {
                unit.squad_id = None;
                unit.stop();
            }
        }
        Some(squad)
    }

    /// Create a standalone mobile unit at the origin.
    pub fn create_unit(&mut self, player_id: PlayerId) -> EntityId {
        self.create_unit_at(player_id, Vec3::ZERO)
    }

    /// Create a standalone mobile unit.
    pub fn create_unit_at(&mut self, player_id: PlayerId, position: Vec3) -> EntityId {
        let id = self.units.allocate_id();
        let mut unit = Unit::new(id, player_id);
        unit.base.set_position(position);
        self.units.insert(id, unit);
        id
    }

    /// Create a building at the origin. Buildings occupy the unit pool.
    pub fn create_building(&mut self, player_id: PlayerId) -> EntityId {
        self.create_building_at(player_id, Vec3::ZERO)
    }

    /// Create an immobile building in the unit pool.
    pub fn create_building_at(&mut self, player_id: PlayerId, position: Vec3) -> EntityId {
        let id = self.units.allocate_id();
        let mut building = Unit::new_building(id, player_id);
        building.base.set_position(position);
        self.units.insert(id, building);
        id
    }

    /// Get a mobile unit or building by its current generational ID.
    #[must_use]
    pub fn get_unit(&self, id: EntityId) -> Option<&Unit> {
        self.units.get(id)
    }

    /// Mutably get a mobile unit or building.
    pub fn get_unit_mut(&mut self, id: EntityId) -> Option<&mut Unit> {
        self.units.get_mut(id)
    }

    /// Get a unit only when it is a building.
    #[must_use]
    pub fn get_building(&self, id: EntityId) -> Option<&Unit> {
        self.units.get(id).filter(|unit| unit.is_building())
    }

    /// Mutably get a unit only when it is a building.
    pub fn get_building_mut(&mut self, id: EntityId) -> Option<&mut Unit> {
        self.units.get_mut(id).filter(|unit| unit.is_building())
    }

    /// Remove a unit or building and clean up squad/base membership.
    pub fn remove_unit(&mut self, id: EntityId) -> Option<Unit> {
        let unit = self.units.remove(id)?;
        if let Some(squad_id) = unit.squad_id
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.remove_unit(id);
        }
        if let Some(base_id) = unit.base_id {
            self.detach_removed_building(base_id, id);
        }
        Some(unit)
    }

    /// Attach a mobile unit to a same-player squad.
    pub fn attach_unit_to_squad(&mut self, unit_id: EntityId, squad_id: EntityId) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        if unit.is_building() || unit.base.player_id != squad.base.player_id {
            return false;
        }
        let old_squad_id = unit.squad_id;
        let formation_offset =
            formation_offset_to_local(squad.base.forward, unit.base.position - squad.base.position);
        if old_squad_id == Some(squad_id) {
            return true;
        }
        if let Some(old_id) = old_squad_id
            && let Some(old_squad) = self.squads.get_mut(old_id)
        {
            old_squad.remove_unit(unit_id);
        }
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        squad.add_unit(unit_id);
        let Some(unit) = self.units.get_mut(unit_id) else {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.remove_unit(unit_id);
            }
            return false;
        };
        unit.squad_id = Some(squad_id);
        unit.formation_offset = formation_offset;
        unit.stop();
        true
    }

    /// Assign a squad-local formation offset and immediately synchronize the member transform.
    pub(crate) fn set_squad_member_formation_offset(
        &mut self,
        unit_id: EntityId,
        offset: Vec3,
    ) -> bool {
        let Some(squad_id) = self.units.get(unit_id).and_then(|unit| unit.squad_id) else {
            return false;
        };
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        let position = squad.base.position + formation_offset_to_world(squad.base.forward, offset);
        let forward = squad.base.forward;
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        unit.formation_offset = offset;
        unit.base.position = position;
        unit.base.forward = forward;
        true
    }

    /// Detach a unit from its current squad.
    pub fn detach_unit_from_squad(&mut self, unit_id: EntityId) -> bool {
        let Some(squad_id) = self.units.get(unit_id).and_then(|unit| unit.squad_id) else {
            return false;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.remove_unit(unit_id);
        }
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        unit.squad_id = None;
        unit.stop();
        true
    }

    /// Create a generic anchor building and register it as a base.
    pub fn create_base(&mut self, player_id: PlayerId, position: Vec3) -> BaseId {
        let anchor_id = self.create_building_at(player_id, position);
        let base_id = self.allocate_base_id();
        self.bases
            .insert(base_id, Base::new(base_id, player_id, anchor_id, position));
        if let Some(anchor) = self.units.get_mut(anchor_id) {
            anchor.base_id = Some(base_id);
        }
        base_id
    }

    /// Register an existing unassigned building as a base anchor.
    pub fn register_base(&mut self, anchor_id: EntityId) -> Option<BaseId> {
        let anchor = self.get_building(anchor_id)?;
        if anchor.base_id.is_some() {
            return None;
        }
        let player_id = anchor.base.player_id;
        let position = anchor.base.position;
        let base_id = self.allocate_base_id();
        self.bases
            .insert(base_id, Base::new(base_id, player_id, anchor_id, position));
        let Some(anchor) = self.units.get_mut(anchor_id) else {
            self.bases.remove(&base_id);
            return None;
        };
        anchor.base_id = Some(base_id);
        Some(base_id)
    }

    /// Add an unassigned, same-player building to a base.
    pub fn add_building_to_base(&mut self, base_id: BaseId, building_id: EntityId) -> bool {
        let Some(base) = self.bases.get(&base_id) else {
            return false;
        };
        let Some(building) = self.get_building(building_id) else {
            return false;
        };
        if building.base_id.is_some() || building.base.player_id != base.player_id {
            return false;
        }
        let Some(base) = self.bases.get_mut(&base_id) else {
            return false;
        };
        base.add_building(building_id);
        let Some(building) = self.units.get_mut(building_id) else {
            if let Some(base) = self.bases.get_mut(&base_id) {
                base.remove_building(building_id);
            }
            return false;
        };
        building.base_id = Some(base_id);
        true
    }

    /// Get a base record.
    #[must_use]
    pub fn get_base(&self, id: BaseId) -> Option<&Base> {
        self.bases.get(&id)
    }

    /// Iterate over bases in deterministic base-number order.
    pub fn bases(&self) -> impl Iterator<Item = (&BaseId, &Base)> {
        self.bases.iter()
    }

    /// Destroy a base and all buildings currently assigned to it.
    pub fn destroy_base(&mut self, id: BaseId) -> bool {
        let Some(base) = self.bases.remove(&id) else {
            return false;
        };
        let building_ids: Vec<_> = base.buildings().collect();
        for building_id in building_ids {
            let _removed = self.remove_unit(building_id);
        }
        true
    }

    /// Update all entities for one tick.
    pub fn update_entities(&mut self, dt: f32) {
        let Some((step_count, step_duration)) = substeps(dt) else {
            return;
        };
        for _ in 0..step_count {
            self.update_entity_substep(step_duration);
        }
    }

    fn update_entity_substep(&mut self, dt: f32) {
        let physics_anchors = prepare_squad_movement(&self.squads, &mut self.units);
        for (_, squad) in self.squads.iter_mut() {
            if !physics_anchors.contains_key(&squad.base.id) {
                squad.update(dt);
            }
        }
        let dead_squads: Vec<_> = self
            .squads
            .iter()
            .filter_map(|(id, squad)| (!squad.is_alive()).then_some(id))
            .collect();
        for id in dead_squads {
            let _removed = self.remove_squad(id);
        }

        for (_, unit) in self.units.iter_mut() {
            unit.update(dt);
        }
        let dead_units: Vec<_> = self
            .units
            .iter()
            .filter_map(|(id, unit)| (!unit.is_alive()).then_some(id))
            .collect();
        for id in dead_units {
            let _removed = self.remove_unit(id);
        }
        resolve_unit_collisions(&mut self.units);
        sync_squad_members(&mut self.squads, &mut self.units, &physics_anchors);
    }

    fn allocate_base_id(&mut self) -> BaseId {
        for _ in 0..=u32::from(u16::MAX) {
            let id = BaseId::new(self.next_base_id);
            self.next_base_id = self.next_base_id.wrapping_add(1);
            if !self.bases.contains_key(&id) {
                return id;
            }
        }
        panic!("base ID space exhausted");
    }

    fn detach_removed_building(&mut self, base_id: BaseId, building_id: EntityId) {
        let is_anchor = self
            .bases
            .get(&base_id)
            .is_some_and(|base| base.anchor_building_id == building_id);
        if is_anchor {
            if let Some(base) = self.bases.remove(&base_id) {
                for other_id in base.buildings() {
                    if let Some(other) = self.units.get_mut(other_id) {
                        other.base_id = None;
                    }
                }
            }
        } else if let Some(base) = self.bases.get_mut(&base_id) {
            base.remove_building(building_id);
        }
    }

    /// Compute a checksum of the entire world state for sync verification.
    ///
    /// This hashes all deterministic state: game time, players, entities.
    /// Two simulations with the same inputs should produce identical checksums.
    #[must_use]
    pub fn checksum(&self) -> u32 {
        let mut cs = SyncChecksum::new();
        cs.hash_u32(self.game_time_ms);
        hash_players(&mut cs, &self.players);
        hash_units(&mut cs, &self.units);
        hash_squads(&mut cs, &self.squads);
        hash_bases(&mut cs, &self.bases);
        cs.value()
    }

    /// Compute a full checksum that also includes RNG state.
    ///
    /// This is useful for detecting divergence in the random number generator,
    /// which would cause future simulation divergence even if current state matches.
    #[must_use]
    pub fn checksum_with_rng(&self) -> u32 {
        let mut cs = SyncChecksum::new();

        // Start with regular world checksum
        cs.hash_u32(self.checksum());

        // Hash RNG state by sampling it (non-destructive check)
        // We can't read internal RNG state directly, so we hash a characteristic
        // We'll use a copy to sample without affecting the original
        let mut rng_copy = self.rng.clone();
        for _ in 0..8 {
            cs.hash_u32(rng_copy.u_rand());
        }

        cs.value()
    }
}

fn hash_players(cs: &mut SyncChecksum, players: &[Player]) {
    cs.hash_u32(u32::try_from(players.len()).unwrap_or(u32::MAX));
    for player in players {
        cs.hash_u32(u32::from(player.id));
        cs.hash_u32(u32::from(player.team_id));
        cs.hash_i32(player.civ_id);
        cs.hash_i32(player.leader_id);
        cs.hash_u32(player.state as u32);
        cs.hash_u32(player.player_type as u32);
        for &amount in &player.resources.amounts {
            cs.hash_f32(amount);
        }
        for population in &player.population {
            cs.hash_f32(population.count);
            cs.hash_f32(population.max);
            cs.hash_f32(population.cap);
            cs.hash_f32(population.future);
        }
    }
}

fn hash_units(cs: &mut SyncChecksum, units: &EntityManager<Unit>) {
    cs.hash_u32(u32::try_from(units.len()).unwrap_or(u32::MAX));
    for (_, unit) in units.iter() {
        hash_base_entity(cs, &unit.base);
        cs.hash_u32(unit.kind as u32);
        cs.hash_u32(unit.archetype as u32);
        cs.hash_u32(unit.state as u32);
        cs.hash_i32(unit.proto_object_id);
        cs.hash_u32(u32::try_from(unit.proto_object_name.len()).unwrap_or(u32::MAX));
        cs.hash_bytes(unit.proto_object_name.as_bytes());
        cs.hash_f32(unit.hitpoints);
        cs.hash_f32(unit.max_hitpoints);
        cs.hash_f32(unit.speed);
        cs.hash_f32(unit.acceleration);
        cs.hash_f32(unit.turn_rate_degrees);
        cs.hash_vec3(
            unit.obstruction_half_extents.x,
            unit.obstruction_half_extents.y,
            unit.obstruction_half_extents.z,
        );
        if let Some(body) = &unit.physics {
            cs.hash_u32(1);
            body.hash_state(cs);
        } else {
            cs.hash_u32(0);
        }
        hash_optional_vec3(cs, unit.move_target);
        hash_optional_entity_id(cs, unit.squad_id);
        hash_optional_base_id(cs, unit.base_id);
        cs.hash_vec3(
            unit.formation_offset.x,
            unit.formation_offset.y,
            unit.formation_offset.z,
        );
    }
}

fn hash_squads(cs: &mut SyncChecksum, squads: &EntityManager<Squad>) {
    cs.hash_u32(u32::try_from(squads.len()).unwrap_or(u32::MAX));
    for (_, squad) in squads.iter() {
        hash_base_entity(cs, &squad.base);
        cs.hash_u32(squad.state as u32);
        cs.hash_u32(squad.archetype as u32);
        cs.hash_u32(squad.formation as u32);
        cs.hash_f32(squad.speed);
        cs.hash_f32(squad.acceleration);
        cs.hash_f32(squad.turn_rate_degrees);
        cs.hash_i32(squad.proto_squad_id);
        cs.hash_u32(u32::try_from(squad.proto_squad_name.len()).unwrap_or(u32::MAX));
        cs.hash_bytes(squad.proto_squad_name.as_bytes());
        cs.hash_f32(squad.turn_radius);
        cs.hash_f32(squad.min_turn_radius);
        cs.hash_f32(squad.max_turn_radius);
        hash_optional_vec3(cs, squad.move_target);
        cs.hash_u32(u32::try_from(squad.unit_ids.len()).unwrap_or(u32::MAX));
        for &unit_id in &squad.unit_ids {
            cs.hash_u32(unit_id.as_u32());
        }
    }
}

fn hash_bases(cs: &mut SyncChecksum, bases: &BTreeMap<BaseId, Base>) {
    cs.hash_u32(u32::try_from(bases.len()).unwrap_or(u32::MAX));
    for (id, base) in bases {
        cs.hash_u32(u32::from(id.as_u16()));
        cs.hash_u32(u32::from(base.player_id));
        cs.hash_u32(base.anchor_building_id.as_u32());
        cs.hash_vec3(base.position.x, base.position.y, base.position.z);
        cs.hash_u32(u32::try_from(base.building_count()).unwrap_or(u32::MAX));
        for building_id in base.buildings() {
            cs.hash_u32(building_id.as_u32());
        }
    }
}

fn hash_base_entity(cs: &mut SyncChecksum, entity: &BaseEntity) {
    cs.hash_u32(entity.id.as_u32());
    cs.hash_u32(u32::from(entity.player_id));
    cs.hash_vec3(entity.position.x, entity.position.y, entity.position.z);
    cs.hash_vec3(entity.forward.x, entity.forward.y, entity.forward.z);
    cs.hash_vec3(entity.velocity.x, entity.velocity.y, entity.velocity.z);
    cs.hash_u32(u32::from(entity.alive));
}

fn hash_optional_vec3(cs: &mut SyncChecksum, value: Option<Vec3>) {
    if let Some(value) = value {
        cs.hash_u32(1);
        cs.hash_vec3(value.x, value.y, value.z);
    } else {
        cs.hash_u32(0);
    }
}

fn hash_optional_entity_id(cs: &mut SyncChecksum, value: Option<EntityId>) {
    cs.hash_u32(value.map_or(EntityId::INVALID.as_u32(), EntityId::as_u32));
}

fn hash_optional_base_id(cs: &mut SyncChecksum, value: Option<BaseId>) {
    cs.hash_u32(value.map_or(u32::MAX, |id| u32::from(id.as_u16())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{ScenarioData, load_scenario_into_world};
    use crate::simulation::Simulation;
    use pipeline::database::hw1::Database;

    /// Sample scenario for testing
    const TEST_SCENARIO: &str = r#"<?xml version="1.0"?>
<Scenario>
  <Players>
    <Player Name="TestPlayer1" Civ="UNSC" Leader1="Cutter" Team="1" />
    <Player Name="TestPlayer2" Civ="Covenant" Leader1="Arbiter" Team="2" />
  </Players>
  <Objects>
    <Object IsSquad="true" Player="1" ID="0" Position="10.0,0.0,20.0">unsc_inf_marine_01</Object>
    <Object IsSquad="true" Player="2" ID="1" Position="50.0,0.0,60.0">cov_inf_grunt_01</Object>
  </Objects>
</Scenario>"#;

    /// Run a simulation for a given number of ticks and return the final checksum.
    fn run_simulation(seed: u64, ticks: u32) -> (u32, u32) {
        let scenario = ScenarioData::from_xml_str(TEST_SCENARIO).unwrap();
        let db = Database::new();
        let mut loaded = load_scenario_into_world(&scenario, &db);
        loaded.world.rng.set_seed64(seed);

        let mut sim = Simulation::with_seed(seed);
        sim.start();

        for _ in 0..ticks {
            sim.tick_with_world(&mut loaded.world);
        }

        (loaded.world.checksum(), loaded.world.checksum_with_rng())
    }

    #[test]
    fn test_world_checksum_deterministic() {
        // Same world should always produce same checksum
        let mut world1 = World::with_seed(12_345);
        world1.init_players(2);
        world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        let mut world2 = World::with_seed(12_345);
        world2.init_players(2);
        world2.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        assert_eq!(world1.checksum(), world2.checksum());
    }

    #[test]
    fn test_world_checksum_different_state() {
        // Different state should produce different checksum
        let mut world1 = World::with_seed(12_345);
        world1.init_players(2);
        world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        let mut world2 = World::with_seed(12_345);
        world2.init_players(2);
        world2.create_squad_at(1, glam::Vec3::new(15.0, 0.0, 25.0)); // Different position

        assert_ne!(world1.checksum(), world2.checksum());
    }

    #[test]
    fn test_simulation_determinism_same_seed() {
        // Run simulation twice with same seed - must produce identical checksums
        let seed = 42;
        let ticks = 100;

        let (cs1, cs1_rng) = run_simulation(seed, ticks);
        let (cs2, cs2_rng) = run_simulation(seed, ticks);

        assert_eq!(cs1, cs2, "World checksum mismatch after {ticks} ticks");
        assert_eq!(
            cs1_rng, cs2_rng,
            "World+RNG checksum mismatch after {ticks} ticks"
        );
    }

    #[test]
    fn test_simulation_different_seeds_different_rng() {
        // Different seeds should produce different RNG states
        // Note: World state (without RNG) may be identical if no random operations occur
        let (_, cs1_rng) = run_simulation(42, 100);
        let (_, cs2_rng) = run_simulation(43, 100);

        assert_ne!(
            cs1_rng, cs2_rng,
            "Different seeds should produce different RNG states"
        );
    }

    #[test]
    fn test_simulation_determinism_long_run() {
        // Run for longer to catch subtle non-determinism
        let seed = 99_999;
        let ticks = 1_000;

        let (cs1, cs1_rng) = run_simulation(seed, ticks);
        let (cs2, cs2_rng) = run_simulation(seed, ticks);

        assert_eq!(cs1, cs2, "World checksum mismatch after {ticks} ticks");
        assert_eq!(
            cs1_rng, cs2_rng,
            "World+RNG checksum mismatch after {ticks} ticks"
        );
    }

    #[test]
    fn test_simulation_checksum_changes_over_time() {
        // Checksum should change as simulation progresses (state evolves)
        let seed = 12_345;

        let (cs_10, _) = run_simulation(seed, 10);
        let (cs_100, _) = run_simulation(seed, 100);

        // At minimum, game_time_ms changes, so checksums should differ
        assert_ne!(cs_10, cs_100, "Checksum should change over time");
    }

    /// Run simulation with movement commands for determinism testing.
    fn run_simulation_with_movement(seed: u64, ticks: u32) -> (u32, u32) {
        let scenario = ScenarioData::from_xml_str(TEST_SCENARIO).unwrap();
        let db = Database::new();
        let mut loaded = load_scenario_into_world(&scenario, &db);
        loaded.world.rng.set_seed64(seed);

        let mut sim = Simulation::with_seed(seed);
        sim.start();

        // Issue move commands to squads after a few ticks
        for tick in 0..ticks {
            if tick == 10 {
                // Issue move orders to all squads
                for (_id, squad) in loaded.world.squads.iter_mut() {
                    let target = glam::Vec3::new(100.0, 0.0, 100.0);
                    squad.move_to(target);
                }
            }
            sim.tick_with_world(&mut loaded.world);
        }

        (loaded.world.checksum(), loaded.world.checksum_with_rng())
    }

    #[test]
    fn test_simulation_determinism_with_movement() {
        // Run simulation with movement twice - must be identical
        let seed = 7_777;
        let ticks = 200;

        let (cs1, cs1_rng) = run_simulation_with_movement(seed, ticks);
        let (cs2, cs2_rng) = run_simulation_with_movement(seed, ticks);

        assert_eq!(
            cs1, cs2,
            "World checksum mismatch with movement after {ticks} ticks"
        );
        assert_eq!(
            cs1_rng, cs2_rng,
            "World+RNG checksum mismatch with movement after {ticks} ticks"
        );
    }

    #[test]
    fn test_movement_changes_checksum() {
        // Directly verify that issuing a move command changes the checksum
        let mut world1 = World::with_seed(12_345);
        world1.init_players(2);
        let squad_id = world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        let mut world2 = World::with_seed(12_345);
        world2.init_players(2);
        let squad_id2 = world2.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

        // Initially should be equal
        assert_eq!(world1.checksum(), world2.checksum());

        // Issue move command to world2's squad
        if let Some(squad) = world2.get_squad_mut(squad_id2) {
            squad.move_to(glam::Vec3::new(100.0, 0.0, 100.0));
        }

        // Now checksums should differ (squad has move_target set)
        assert_ne!(
            world1.checksum(),
            world2.checksum(),
            "Move command should change checksum"
        );

        // Run some ticks on world2 to actually move
        for _ in 0..20 {
            world2.update_entities(0.05); // 50ms
            world2.game_time_ms += 50;
        }

        // Position should have changed
        let squad1_pos = world1.get_squad(squad_id).unwrap().position();
        let squad2_pos = world2.get_squad(squad_id2).unwrap().position();
        assert_ne!(
            squad1_pos, squad2_pos,
            "Squad should have moved after update"
        );
    }

    #[test]
    fn buildings_share_the_unit_pool() {
        let mut world = World::new();
        let building_id = world.create_building_at(1, Vec3::new(4.0, 0.0, 8.0));

        assert_eq!(building_id.class(), Some(EntityClass::Unit));
        assert!(world.get_building(building_id).is_some());
        assert!(!world.get_unit_mut(building_id).unwrap().move_to(Vec3::X));
    }

    #[test]
    fn squad_members_follow_the_squad_transform() {
        let mut world = World::new();
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let unit_id = world.create_unit_at(1, Vec3::X);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));

        world
            .get_squad_mut(squad_id)
            .unwrap()
            .move_to(Vec3::new(10.0, 0.0, 0.0));
        world.update_entities(0.1);

        let squad = world.get_squad(squad_id).unwrap();
        let unit = world.get_unit(unit_id).unwrap();
        let world_offset = unit.base.position - squad.base.position;
        assert!((world_offset - Vec3::NEG_Z).length() < f32::EPSILON);
    }

    #[test]
    fn detached_units_stop_inheriting_squad_motion() {
        let mut world = World::new();
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let unit_id = world.create_unit_at(1, Vec3::X);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        world
            .get_squad_mut(squad_id)
            .unwrap()
            .move_to(Vec3::X * 10.0);
        world.update_entities(0.1);

        assert!(world.detach_unit_from_squad(unit_id));
        let detached_position = world.get_unit(unit_id).unwrap().base.position;
        assert_eq!(world.get_unit(unit_id).unwrap().base.velocity, Vec3::ZERO);
        assert!(world.get_unit_mut(unit_id).unwrap().move_to(Vec3::X * 20.0));
        world.update_entities(0.1);

        assert_ne!(
            world.get_unit(unit_id).unwrap().base.position,
            detached_position
        );
    }

    #[test]
    fn removing_an_anchor_dissolves_base_membership() {
        let mut world = World::new();
        let anchor_id = world.create_building(1);
        let base_id = world.register_base(anchor_id).unwrap();
        let second_id = world.create_building(1);
        assert!(world.add_building_to_base(base_id, second_id));

        let removed = world.remove_unit(anchor_id);

        assert!(removed.is_some());
        assert!(world.get_base(base_id).is_none());
        assert_eq!(world.get_building(second_id).unwrap().base_id, None);
    }

    #[test]
    fn dead_units_are_removed_and_stale_ids_fail() {
        let mut world = World::new();
        let old_id = world.create_unit(1);
        world.get_unit_mut(old_id).unwrap().kill();
        world.update_entities(0.05);
        let replacement_id = world.create_unit(1);

        assert!(world.get_unit(old_id).is_none());
        assert_eq!(old_id.pool_index(), replacement_id.pool_index());
        assert_ne!(old_id.generation(), replacement_id.generation());
    }
}
