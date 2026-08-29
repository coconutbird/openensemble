//! World state container for the simulation, based on retail `BWorld`.

use crate::entities::squads::{formation_offset_to_local, formation_offset_to_world};
use crate::entities::{Base, BaseId, Object, Projectile, ShieldCoverage, Squad, Unit};
use crate::entity::EntityManager;
use crate::entity_id::{EntityClass, EntityId};
use crate::gameplay::GroundVehiclePhysicsProfile;
use crate::player::{GAIA_PLAYER, MAX_TEAMS, Player, PlayerId, TeamRelation};
use crate::random::{Random, SimRandom};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet};

mod ability;
mod ammunition;
mod attachments;
mod bounds;
mod checksum;
mod collision_attacks;
mod combat;
mod construction;
mod control;
mod cryo;
mod custom_commands;
mod death_replacements;
mod death_spawns;
mod design_lines;
mod detonate;
mod events;
mod game_settings;
mod garrison;
mod health;
mod hitch;
mod icons;
mod idle;
mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
mod mines;
mod object_state;
mod object_types;
mod objectives;
mod orders;
mod ownership;
mod powers;
mod presentation;
mod production;
mod projectiles;
mod protection;
mod proto_data;
mod query;
mod rally_points;
mod repair;
mod research;
mod resources;
mod revival;
mod roster;
mod scoring;
mod shields;
pub(crate) mod sockets;
mod spatial;
mod team;
mod technology;
mod terrain;
mod time;
mod timers;
mod tower_walls;
mod training;
mod transports;
mod triggers;
mod update;
mod veterancy;
mod visibility;

pub use bounds::WorldBounds;
pub use construction::{ConstructionError, ConstructionQueueResult};
pub use custom_commands::{CustomCommand, CustomCommandFlags};
pub use design_lines::DesignLineId;
pub(crate) use events::EventEntityParameter;
pub use events::{
    ChatRequest, CinematicRequest, GeneralEvent, GeneralEventType, PresentationRequest,
};
pub use garrison::GarrisonError;
pub use health::UnitHealth;
pub use hitch::HitchError;
pub use objectives::ObjectiveState;
pub use powers::{
    CryoPowerError, CryoPowerExecution, CryoPowerInvocation, DisruptionPowerError,
    DisruptionPowerExecution, DisruptionPowerInvocation, NativePowerError, NativePowerInput,
    NativePowerInvocation, OdstDrop, OdstPowerError, OdstPowerExecution, OdstPowerInvocation,
    PowerExecutionId, RagePowerError, RagePowerExecution, RagePowerInvocation, RagePowerPhase,
    RepairPowerError, RepairPowerExecution, RepairPowerInvocation, power_prototype_id,
    TransportPowerError, TransportPowerExecution, TransportPowerInvocation,
};
pub use presentation::{
    CameraControlPermissions, CameraDirective, CameraShake, HintCallout, HintCalloutAnchor,
    HudItem, ObjectivePointer, PlayerPresentationState, RumbleMotor, RumbleRequest,
    ScreenFadeOverlay, ScreenFadeSequence,
};
pub use production::ProductionUpdate;
pub use research::{ResearchError, ResearchQueueResult, technology_prototype_id};
pub use scoring::ScenarioScoreInfo;
pub use technology::TechnologyError;
pub use terrain::TerrainLoadError;
pub use timers::{GameTimer, GameTimerAudience};
pub(crate) use training::TriggerTrainingRequest;
pub use training::{
    MAX_TRAIN_BATCH, TrainingError, TrainingQueueResult, object_runtime_id, squad_runtime_id,
};

use team::neutral_team_relations;

/// Maximum supported players.
pub const MAX_PLAYERS: usize = 8;

/// World state container.
///
/// Contains all game state: players, entities, time, etc.
#[derive(Debug)]
pub struct World {
    /// All players (index = player ID).
    players: Vec<Player>,
    /// Directed team diplomacy matrix, matching vanilla `BWorld` state.
    team_relations: [[TeamRelation; MAX_TEAMS]; MAX_TEAMS],
    /// Whether the current game was configured as campaign co-op.
    coop: bool,
    /// Whether gameplay fog of war is enabled for every team.
    fog_of_war_enabled: bool,
    /// Whether every team's whole terrain map has been explored.
    black_map_cleared: bool,
    /// Scenario-authored campaign scoring configuration.
    scenario_score: scoring::ScenarioScoreState,
    /// Scenario-authored path geometry keyed by retail design-line ID.
    design_lines: design_lines::DesignLineState,
    /// Scenario-authored objectives in their retail manager order.
    objectives: Vec<ObjectiveState>,
    /// Full horizontal terrain extent loaded from the active scenario XTD.
    terrain_bounds: Option<WorldBounds>,
    /// Active scenario-playable subset; `None` uses the full terrain extent.
    playable_bounds: Option<WorldBounds>,
    /// Retail XSD simulation terrain used by gameplay height queries.
    terrain_simulation: Option<terrain::TerrainSimulation>,
    /// Deterministic configuration symbols visible to retail trigger scripts.
    config_symbols: BTreeSet<String>,
    veterancy: game_settings::VeterancySetting,
    /// Retail general-event subscriptions and completion state.
    general_events: events::GeneralEventState,
    /// Renderer-facing requests authored by the authoritative simulation.
    presentation: events::PresentationState,
    /// Durable trigger-authored controls projected by renderer/UI clients.
    presentation_control: presentation::PresentationControlState,
    /// Retail's fixed four-slot trigger timer manager.
    game_timers: timers::GameTimerState,
    /// Scenario-authored custom command buttons keyed by retail command ID.
    custom_commands: BTreeMap<i32, CustomCommand>,
    /// Next monotonically assigned retail custom command ID.
    next_custom_command_id: i32,
    /// Paid custom-command work waiting on authoritative completion timers.
    custom_command_executions: Vec<custom_commands::CustomCommandExecution>,
    /// Running native powers and their deterministic execution IDs.
    power_manager: powers::PowerManagerState,
    /// Current game time in milliseconds.
    pub game_time_ms: u32,
    /// Deterministic RNG for the world.
    pub rng: Random,
    /// Retail synchronized random-manager stream used by trigger operations.
    sim_rng: SimRandom,
    /// Damage scalar applied to targets that have not completed construction.
    construction_damage_multiplier: f32,
    /// Concrete and abstract object types keyed by live proto-object database ID.
    prototype_object_types: BTreeMap<i32, Vec<String>>,
    /// Proto-squad name and maximum child count keyed by live database ID.
    prototype_squads: BTreeMap<i32, (String, u32)>,
    /// Scenario-layered integral shield coverage keyed by proto-object name.
    prototype_shield_coverages: BTreeMap<String, ShieldCoverage>,
    /// Scenario-layered supported vehicle bodies keyed by proto-object name.
    prototype_ground_vehicle_physics: BTreeMap<String, GroundVehiclePhysicsProfile>,
    /// Class-0 invisible and world-control objects.
    pub objects: EntityManager<Object>,
    /// Unit pool. Mobile units and buildings both use vanilla class 1.
    pub units: EntityManager<Unit>,
    /// Squad entity manager.
    pub squads: EntityManager<Squad>,
    /// Projectile pool. Projectiles use vanilla entity class 4.
    pub projectiles: EntityManager<Projectile>,
    /// Base ownership records, which are not standalone vanilla entities.
    bases: BTreeMap<BaseId, Base>,
    /// Next candidate base number.
    next_base_id: u16,
    /// Authoritative scenario and gameplay trigger scripts.
    trigger_engine: crate::trigger::TriggerEngine,
    /// Trigger-state writes deferred while their script is being evaluated.
    pending_building_command_events: Vec<triggers::BuildingCommandEvent>,
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
            team_relations: neutral_team_relations(),
            coop: false,
            fog_of_war_enabled: true,
            black_map_cleared: false,
            scenario_score: scoring::ScenarioScoreState::default(),
            design_lines: design_lines::DesignLineState::default(),
            objectives: Vec::new(),
            terrain_bounds: None,
            playable_bounds: None,
            terrain_simulation: None,
            config_symbols: game_settings::default_config_symbols(),
            veterancy: game_settings::VeterancySetting::Enabled,
            general_events: events::GeneralEventState::default(),
            presentation: events::PresentationState::default(),
            presentation_control: presentation::PresentationControlState::default(),
            game_timers: timers::GameTimerState::default(),
            custom_commands: BTreeMap::new(),
            next_custom_command_id: 0,
            custom_command_executions: Vec::new(),
            power_manager: powers::PowerManagerState::default(),
            game_time_ms: 0,
            rng: Random::new(),
            sim_rng: SimRandom::new(),
            construction_damage_multiplier: 1.0,
            prototype_object_types: BTreeMap::new(),
            prototype_squads: BTreeMap::new(),
            prototype_shield_coverages: BTreeMap::new(),
            prototype_ground_vehicle_physics: BTreeMap::new(),
            objects: EntityManager::new(EntityClass::Object),
            units: EntityManager::new(EntityClass::Unit),
            squads: EntityManager::new(EntityClass::Squad),
            projectiles: EntityManager::new(EntityClass::Projectile),
            bases: BTreeMap::new(),
            next_base_id: 0,
            trigger_engine: crate::trigger::TriggerEngine::new(),
            pending_building_command_events: Vec::new(),
        }
    }

    /// Create a world with a specific random seed.
    #[must_use]
    pub fn with_seed(seed: u64) -> Self {
        let mut world = Self::new();
        world.rng.set_seed64(seed);
        let [byte_0, byte_1, byte_2, byte_3, _, _, _, _] = seed.to_le_bytes();
        world
            .sim_rng
            .set_seed(u32::from_le_bytes([byte_0, byte_1, byte_2, byte_3]));
        world
    }

    pub(crate) fn trigger_random_index(&mut self, maximum: u32) -> u32 {
        self.sim_rng.index(maximum)
    }

    pub(crate) fn trigger_random_float(&mut self, minimum: f32, maximum: f32) -> f32 {
        self.sim_rng.range_float(minimum, maximum)
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
        self.prepare_remove_squad_garrison(id);
        self.detach_squad_hitch(id);
        self.prepare_remove_squad_protection(id);
        let squad = self.squads.remove(id)?;
        self.remove_hint_callouts_for_entity(id);
        for (_, other_squad) in self.squads.iter_mut() {
            other_squad.clear_teleporter_destination(id);
            other_squad.clear_join_reference(id);
            other_squad.clear_damage_proxy_target(id);
            other_squad.remove_associated_wall_tower(id);
        }
        for (_, unit) in self.units.iter_mut() {
            if unit
                .tower_wall
                .is_some_and(|action| action.target_squad_id() == id)
            {
                unit.tower_wall = None;
            }
        }
        if let Some(player) = self.get_player_mut(squad.base.player_id) {
            player.revoke_first_power_from_squad(id);
            player.release_population(&squad.population_costs);
        }
        for unit_id in &squad.unit_ids {
            if let Some(unit) = self.units.get_mut(*unit_id)
                && unit.squad_id == Some(id)
            {
                unit.squad_id = None;
                unit.shields.request_recharge();
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
        let socket_children = self
            .units
            .get(id)?
            .associated_socket_ids
            .iter()
            .copied()
            .filter(|&socket_id| {
                self.units
                    .get(socket_id)
                    .is_some_and(|socket| socket.socket_parent_id == Some(id))
            })
            .collect::<Vec<_>>();
        self.remove_owned_attachments(id);
        self.detach_attachment_from_parent(id);
        for socket_id in socket_children {
            let _removed = self.remove_unit(socket_id);
        }
        self.prepare_remove_unit_garrison(id);
        self.detach_unit_socket_refs(id);
        let unit = self.units.remove(id)?;
        self.remove_hint_callouts_for_entity(id);
        self.refund_production_for_removed_unit(&unit);
        if let Some(player) = self.get_player_mut(unit.base.player_id) {
            player.release_population(&unit.population_costs);
            if unit.built {
                player.adjust_population_cap(&unit.population_cap_additions, false);
            }
        }
        let mut emptied_squad = None;
        if let Some(squad_id) = unit.squad_id
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.remove_unit(id);
            if squad.unit_ids.is_empty() {
                emptied_squad = Some(squad_id);
            }
        }
        if let Some(squad_id) = emptied_squad {
            let _removed = self.remove_squad(squad_id);
        }
        if let Some(base_id) = unit.base_id {
            self.detach_removed_building(base_id, id);
        }
        Some(unit)
    }

    /// Attach a unit or building to a same-player squad.
    pub fn attach_unit_to_squad(&mut self, unit_id: EntityId, squad_id: EntityId) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        if unit.base.player_id != squad.base.player_id {
            return false;
        }
        let shielded = unit.shields.is_enabled();
        let old_squad_id = unit.squad_id;
        let reverse_move = squad.is_reverse_moving();
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
        if shielded {
            squad.shields.request_recharge();
        }
        let Some(unit) = self.units.get_mut(unit_id) else {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.remove_unit(unit_id);
            }
            return false;
        };
        unit.squad_id = Some(squad_id);
        unit.set_reverse_move(reverse_move);
        unit.shields.clear_recharge_request();
        unit.formation_offset = formation_offset;
        unit.stop();
        self.apply_squad_cryo_to_unit(squad_id, unit_id);
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
        unit.shields.request_recharge();
        unit.clear_cryo_effect();
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
        self.destroy_removed_base_plasma_shield(&base);
        let building_ids: Vec<_> = base.buildings().collect();
        for building_id in building_ids {
            let _removed = self.remove_unit(building_id);
        }
        true
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
                self.destroy_removed_base_plasma_shield(&base);
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
}
