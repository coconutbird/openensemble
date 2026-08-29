//! Retail `BPowerTransport` two-point selection and carrier execution.

use super::common::{
    PowerPayment, consume_power, hash_string, optional_float, optional_int, validate_level,
    validate_power_type, validate_requirements,
};
use super::{
    NativePowerError, NativePowerInput, NativePowerInvocation, PowerExecutionId, power_by_id,
};
use crate::commands::PowerUserId;
use crate::entities::squads::{SquadContainmentState, SquadPowerTransportPlan};
use crate::entities::SquadMode;
use crate::entity::Entity;
use crate::player::{PlayerId, ProtoPowerId};
use crate::scenario::placed::create_trigger_unit_squad;
use crate::spawn::object_prototype_id;
use crate::sync::SyncChecksum;
use crate::world::{GeneralEvent, GeneralEventType, World};
use crate::EntityId;
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::powers::PowerAttributes;
use pipeline::database::hw1::{Database, Power, ProtoObject};

const TRANSPORT_POWER_TYPE: u32 = 8;

/// Inputs accepted by retail's direct `InvokePower2` Transport path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportPowerInvocation {
    pub player_id: PlayerId,
    pub proto_power_id: ProtoPowerId,
    pub power_level: u32,
    pub squad_id: EntityId,
    pub target_location: Vec3,
    pub ignore_requirements: bool,
    pub power_user_id: PowerUserId,
}

impl From<TransportPowerInvocation> for NativePowerInvocation {
    fn from(invocation: TransportPowerInvocation) -> Self {
        Self {
            player_id: invocation.player_id,
            proto_power_id: invocation.proto_power_id,
            power_level: invocation.power_level,
            squad_id: invocation.squad_id,
            target_location: invocation.target_location,
            ignore_requirements: invocation.ignore_requirements,
            power_user_id: invocation.power_user_id,
        }
    }
}

/// Error returned by Transport invocation.
pub type TransportPowerError = NativePowerError;

/// Authoritative state for one running retail `BPowerTransport` targeting session.
#[derive(Debug, Clone)]
pub struct TransportPowerExecution {
    id: PowerExecutionId,
    player_id: PlayerId,
    proto_power_id: ProtoPowerId,
    power_level: u32,
    owner_squad_id: EntityId,
    power_user_id: PowerUserId,
    target_location: Vec3,
    pickup_location: Option<Vec3>,
    selected_squad_ids: Vec<EntityId>,
    ui_radius: f32,
    min_transport_distance: f32,
    max_ground_vehicles: Option<u32>,
    max_infantry_units: Option<u32>,
    transport_prototype: String,
    transport_prototype_id: i32,
    transport_contains: Vec<String>,
    max_contained_population: f32,
    maximum_transports: u32,
    carrier_speed: f32,
    carrier_spacing: f32,
    incoming_height: f32,
    incoming_offset: f32,
    outgoing_height: f32,
    outgoing_offset: f32,
    pickup_height: f32,
    dropoff_height: f32,
    ignore_requirements: bool,
}

impl TransportPowerExecution {
    #[must_use]
    pub const fn id(&self) -> PowerExecutionId {
        self.id
    }

    #[must_use]
    pub const fn player_id(&self) -> PlayerId {
        self.player_id
    }

    #[must_use]
    pub const fn proto_power_id(&self) -> ProtoPowerId {
        self.proto_power_id
    }

    #[must_use]
    pub const fn power_user_id(&self) -> PowerUserId {
        self.power_user_id
    }

    #[must_use]
    pub const fn pickup_location(&self) -> Option<Vec3> {
        self.pickup_location
    }

    #[must_use]
    pub fn selected_squad_ids(&self) -> &[EntityId] {
        &self.selected_squad_ids
    }

    #[must_use]
    pub const fn ui_radius(&self) -> f32 {
        self.ui_radius
    }

    #[must_use]
    pub const fn min_transport_distance(&self) -> f32 {
        self.min_transport_distance
    }

    #[must_use]
    pub fn transport_prototype(&self) -> &str {
        &self.transport_prototype
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id.get());
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_u32(self.power_level);
        checksum.hash_u32(self.owner_squad_id.as_u32());
        checksum.hash_u32(self.power_user_id.raw());
        hash_vec3(checksum, self.target_location);
        hash_optional_vec3(checksum, self.pickup_location);
        checksum.hash_u32(u32::try_from(self.selected_squad_ids.len()).unwrap_or(u32::MAX));
        for squad_id in &self.selected_squad_ids {
            checksum.hash_u32(squad_id.as_u32());
        }
        for value in [
            self.ui_radius,
            self.min_transport_distance,
            self.max_contained_population,
            self.carrier_speed,
            self.carrier_spacing,
            self.incoming_height,
            self.incoming_offset,
            self.outgoing_height,
            self.outgoing_offset,
            self.pickup_height,
            self.dropoff_height,
        ] {
            checksum.hash_f32(value);
        }
        hash_optional_u32(checksum, self.max_ground_vehicles);
        hash_optional_u32(checksum, self.max_infantry_units);
        hash_string(checksum, &self.transport_prototype);
        checksum.hash_i32(self.transport_prototype_id);
        checksum.hash_u32(u32::try_from(self.transport_contains.len()).unwrap_or(u32::MAX));
        for object_type in &self.transport_contains {
            hash_string(checksum, object_type);
        }
        checksum.hash_u32(self.maximum_transports);
        checksum.hash_u32(u32::from(self.ignore_requirements));
    }
}

#[derive(Debug)]
struct TransportProfile {
    ui_radius: f32,
    min_transport_distance: f32,
    max_ground_vehicles: Option<u32>,
    max_infantry_units: Option<u32>,
    transport_prototype: String,
    transport_prototype_id: i32,
    transport_contains: Vec<String>,
    max_contained_population: f32,
    maximum_transports: u32,
    carrier_speed: f32,
    carrier_spacing: f32,
    incoming_height: f32,
    incoming_offset: f32,
    outgoing_height: f32,
    outgoing_offset: f32,
    pickup_height: f32,
    dropoff_height: f32,
}

#[derive(Debug)]
struct TransportGroup {
    passengers: Vec<EntityId>,
    pickup: Vec3,
    dropoff: Vec3,
}

impl World {
    /// Validate and begin one authoritative two-point Transport session.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed profile/civilization data, unavailable
    /// ownership, or a mismatched packed power-user ID.
    pub fn invoke_transport_power(
        &mut self,
        database: &Database,
        invocation: TransportPowerInvocation,
    ) -> Result<PowerExecutionId, TransportPowerError> {
        invoke(self, database, invocation.into())
    }

    /// Submit direct input using a stable simulation execution ID.
    pub fn submit_transport_power_input(
        &mut self,
        database: &Database,
        execution_id: PowerExecutionId,
        input: NativePowerInput,
        no_cost: bool,
    ) -> bool {
        submit_input_by_execution(self, database, execution_id, input, no_cost)
    }
}

pub(super) fn invoke(
    world: &mut World,
    database: &Database,
    invocation: NativePowerInvocation,
) -> Result<PowerExecutionId, NativePowerError> {
    if !invocation.target_location.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    validate_power_user(invocation)?;
    let power = power_by_id(database, invocation.proto_power_id)
        .ok_or(NativePowerError::PowerNotFound(invocation.proto_power_id))?;
    let attributes = validate_power_type(power, "Transport")?;
    let profile = TransportProfile::resolve(world, database, attributes, invocation)?;
    if !invocation.ignore_requirements {
        let _payment = validate_requirements(
            world,
            database,
            power,
            invocation.player_id,
            invocation.proto_power_id,
        )?;
    }
    let id = world.power_manager.allocate_id();
    world
        .power_manager
        .transport_executions
        .push(create_execution(id, invocation, profile));
    Ok(id)
}

fn validate_power_user(invocation: NativePowerInvocation) -> Result<(), NativePowerError> {
    let id = invocation.power_user_id;
    if id.is_valid()
        && (id.player_id() != i32::from(invocation.player_id)
            || id.power_type() != TRANSPORT_POWER_TYPE)
    {
        return Err(NativePowerError::InvalidData("PowerUserID"));
    }
    Ok(())
}

fn create_execution(
    id: PowerExecutionId,
    invocation: NativePowerInvocation,
    profile: TransportProfile,
) -> TransportPowerExecution {
    TransportPowerExecution {
        id,
        player_id: invocation.player_id,
        proto_power_id: invocation.proto_power_id,
        power_level: invocation.power_level,
        owner_squad_id: invocation.squad_id,
        power_user_id: invocation.power_user_id,
        target_location: invocation.target_location,
        pickup_location: None,
        selected_squad_ids: Vec::new(),
        ui_radius: profile.ui_radius,
        min_transport_distance: profile.min_transport_distance,
        max_ground_vehicles: profile.max_ground_vehicles,
        max_infantry_units: profile.max_infantry_units,
        transport_prototype: profile.transport_prototype,
        transport_prototype_id: profile.transport_prototype_id,
        transport_contains: profile.transport_contains,
        max_contained_population: profile.max_contained_population,
        maximum_transports: profile.maximum_transports,
        carrier_speed: profile.carrier_speed,
        carrier_spacing: profile.carrier_spacing,
        incoming_height: profile.incoming_height,
        incoming_offset: profile.incoming_offset,
        outgoing_height: profile.outgoing_height,
        outgoing_offset: profile.outgoing_offset,
        pickup_height: profile.pickup_height,
        dropoff_height: profile.dropoff_height,
        ignore_requirements: invocation.ignore_requirements,
    }
}

pub(super) fn submit_input_by_user(
    world: &mut World,
    database: &Database,
    power_user_id: PowerUserId,
    input: NativePowerInput,
    no_cost: bool,
) -> bool {
    if !power_user_id.is_valid() {
        return false;
    }
    submit_input(world, database, input, no_cost, |execution| {
        execution.power_user_id == power_user_id
    })
}

fn submit_input_by_execution(
    world: &mut World,
    database: &Database,
    execution_id: PowerExecutionId,
    input: NativePowerInput,
    no_cost: bool,
) -> bool {
    submit_input(world, database, input, no_cost, |execution| {
        execution.id == execution_id
    })
}

fn submit_input(
    world: &mut World,
    database: &Database,
    input: NativePowerInput,
    no_cost: bool,
    matches: impl Fn(&TransportPowerExecution) -> bool,
) -> bool {
    let Some(index) = world
        .power_manager
        .transport_executions
        .iter()
        .position(matches)
    else {
        return false;
    };
    let mut execution = world.power_manager.transport_executions.remove(index);
    let (accepted, keep) = handle_input(world, database, &mut execution, input, no_cost);
    if keep {
        world
            .power_manager
            .transport_executions
            .insert(index, execution);
    }
    accepted
}

fn handle_input(
    world: &mut World,
    database: &Database,
    execution: &mut TransportPowerExecution,
    input: NativePowerInput,
    no_cost: bool,
) -> (bool, bool) {
    match input {
        NativePowerInput::Confirm(location) if execution.pickup_location.is_none() => (
            confirm_pickup(world, database, execution, location, no_cost),
            true,
        ),
        NativePowerInput::Confirm(location) => {
            let accepted = confirm_dropoff(world, database, execution, location, no_cost);
            (accepted, !accepted)
        }
        NativePowerInput::Shutdown if execution.pickup_location.is_some() => {
            execution.pickup_location = None;
            execution.selected_squad_ids.clear();
            (true, true)
        }
        NativePowerInput::Shutdown => (true, false),
        NativePowerInput::Position(_) | NativePowerInput::Direction(_) => (false, true),
    }
}

fn confirm_pickup(
    world: &World,
    database: &Database,
    execution: &mut TransportPowerExecution,
    location: Vec3,
    no_cost: bool,
) -> bool {
    let Some((power, attributes)) = execution_power(database, execution) else {
        return false;
    };
    let Ok(location) = validate_location(world, attributes, location, execution.ui_radius) else {
        return false;
    };
    if !requirements_are_met(world, database, power, execution, no_cost) {
        return false;
    }
    let selected = select_transportable_squads(world, execution, location);
    execution.target_location = location;
    if !selected.is_empty() {
        execution.pickup_location = Some(location);
        execution.selected_squad_ids = selected;
    }
    true
}

fn confirm_dropoff(
    world: &mut World,
    database: &Database,
    execution: &mut TransportPowerExecution,
    location: Vec3,
    no_cost: bool,
) -> bool {
    let Some(pickup) = execution.pickup_location else {
        return false;
    };
    let Some((power, attributes)) = execution_power(database, execution) else {
        return false;
    };
    let Ok(location) = validate_location(world, attributes, location, execution.ui_radius) else {
        return false;
    };
    if pickup.distance(location) < execution.min_transport_distance {
        return false;
    }
    let ignore_requirements = execution.ignore_requirements || no_cost;
    let payment = if ignore_requirements {
        PowerPayment::default()
    } else {
        let Ok(payment) = validate_requirements(
            world,
            database,
            power,
            execution.player_id,
            execution.proto_power_id,
        ) else {
            return false;
        };
        payment
    };
    if !ignore_requirements
        && consume_power(
            world,
            power,
            execution.player_id,
            execution.proto_power_id,
            execution.owner_squad_id,
            &payment,
        )
        .is_err()
    {
        return false;
    }
    execution.target_location = location;
    let _carriers = launch_transports(world, database, execution, pickup, location);
    let _fired = world.fire_general_event(&GeneralEvent::new(
        GeneralEventType::UsedPower,
        i32::from(execution.player_id),
    ));
    true
}

fn execution_power<'database>(
    database: &'database Database,
    execution: &TransportPowerExecution,
) -> Option<(&'database Power, &'database PowerAttributes)> {
    let power = power_by_id(database, execution.proto_power_id)?;
    let attributes = power.attributes.as_ref()?;
    Some((power, attributes))
}

fn requirements_are_met(
    world: &World,
    database: &Database,
    power: &Power,
    execution: &TransportPowerExecution,
    no_cost: bool,
) -> bool {
    execution.ignore_requirements
        || no_cost
        || validate_requirements(
            world,
            database,
            power,
            execution.player_id,
            execution.proto_power_id,
        )
        .is_ok()
}

fn validate_location(
    world: &World,
    attributes: &PowerAttributes,
    mut location: Vec3,
    radius: f32,
) -> Result<Vec3, NativePowerError> {
    if !location.is_finite() {
        return Err(NativePowerError::InvalidTarget);
    }
    if world.is_outside_playable_bounds(location, true) {
        return Err(NativePowerError::InvalidPlacement);
    }
    if let Some(id) =
        super::disruption::disrupting_power_circle(world, attributes, location, radius)
    {
        return Err(NativePowerError::Disrupted(id));
    }
    if let Some(height) = world.terrain_height(location, true) {
        location.y = height;
    }
    Ok(location)
}

fn select_transportable_squads(
    world: &World,
    execution: &TransportPowerExecution,
    location: Vec3,
) -> Vec<EntityId> {
    let radius_squared = execution.ui_radius * execution.ui_radius;
    let mut candidates = world
        .squads
        .iter()
        .filter(|(squad_id, squad)| {
            squad.base.player_id == execution.player_id
                && planar_distance_squared(squad.base.position, location) <= radius_squared
                && squad_is_transportable(world, *squad_id, true)
        })
        .map(|(squad_id, squad)| {
            (
                squad_id,
                planar_distance_squared(squad.base.position, location),
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|(left_id, left_distance), (right_id, right_distance)| {
        left_distance
            .total_cmp(right_distance)
            .then_with(|| left_id.cmp(right_id))
    });
    apply_type_limits(world, execution, candidates)
}

fn apply_type_limits(
    world: &World,
    execution: &TransportPowerExecution,
    candidates: Vec<(EntityId, f32)>,
) -> Vec<EntityId> {
    let mut ground_vehicles = 0;
    let mut infantry = 0;
    let mut selected = Vec::new();
    for (squad_id, _) in candidates {
        if world.squad_object_type_match(squad_id, "GroundVehicle") == Some(true) {
            if execution
                .max_ground_vehicles
                .is_some_and(|limit| ground_vehicles < limit)
            {
                ground_vehicles += 1;
                selected.push(squad_id);
            }
        } else if world.squad_object_type_match(squad_id, "Infantry") == Some(true)
            && execution
                .max_infantry_units
                .is_some_and(|limit| infantry < limit)
        {
            infantry += 1;
            selected.push(squad_id);
        }
    }
    selected
}

fn squad_is_transportable(world: &World, squad_id: EntityId, check_reserved: bool) -> bool {
    let Some(squad) = world.get_squad(squad_id) else {
        return false;
    };
    let Some(leader) = squad
        .unit_ids
        .first()
        .and_then(|unit_id| world.get_unit(*unit_id))
    else {
        return false;
    };
    squad.is_alive()
        && leader.is_object_type("Transportable")
        && !leader.is_down()
        && !squad.is_cryo_frozen()
        && squad.mode != SquadMode::Lockdown
        && matches!(squad.garrison.state(), SquadContainmentState::Free)
        && !squad.is_hitched()
        && squad.hitched_squad().is_none()
        && squad.board_state().is_none()
        && squad
            .unit_ids
            .iter()
            .all(|unit_id| world.get_unit(*unit_id).is_some_and(|unit| !unit.is_being_boarded()))
        && (!check_reserved || !squad_is_reserved(world, squad_id))
}

fn squad_is_reserved(world: &World, squad_id: EntityId) -> bool {
    world
        .power_manager
        .transport_executions
        .iter()
        .any(|execution| execution.selected_squad_ids.contains(&squad_id))
        || world.squad_has_power_transport_reservation(squad_id)
}

fn launch_transports(
    world: &mut World,
    database: &Database,
    execution: &TransportPowerExecution,
    pickup: Vec3,
    dropoff: Vec3,
) -> Vec<EntityId> {
    let selected = execution
        .selected_squad_ids
        .iter()
        .copied()
        .filter(|squad_id| squad_is_transportable(world, *squad_id, false))
        .filter(|squad_id| transport_accepts_squad(world, execution, *squad_id))
        .collect::<Vec<_>>();
    let groups = transport_groups(world, execution, &selected, pickup, dropoff);
    let mut carriers = Vec::with_capacity(groups.len());
    for group in groups {
        if let Some(carrier_id) = launch_transport_group(world, database, execution, group) {
            carriers.push(carrier_id);
        }
    }
    carriers
}

fn transport_groups(
    world: &World,
    execution: &TransportPowerExecution,
    selected: &[EntityId],
    pickup: Vec3,
    dropoff: Vec3,
) -> Vec<TransportGroup> {
    let total_population = selected
        .iter()
        .map(|squad_id| rounded_transport_population(world, *squad_id))
        .sum::<f32>();
    let count = (total_population / execution.max_contained_population)
        .ceil()
        .to_u32()
        .unwrap_or(u32::MAX)
        .min(execution.maximum_transports);
    if count == 0 {
        return Vec::new();
    }
    let average = average_squad_position(world, selected).unwrap_or(pickup);
    let direction = horizontal_direction(dropoff - average).unwrap_or(Vec3::Z);
    let right = Vec3::Y.cross(direction).normalize_or(Vec3::X);
    let center = count.saturating_sub(1).to_f32().unwrap_or(f32::MAX) * 0.5;
    let mut remaining = selected.to_vec();
    let mut result = Vec::new();
    for index in 0..count {
        let offset = right
            * ((index.to_f32().unwrap_or(f32::MAX) - center) * execution.carrier_spacing);
        let carrier_pickup = ground_position(world, average + offset);
        let carrier_dropoff = ground_position(world, dropoff + offset);
        remaining.sort_by(|left, right| {
            squad_distance_squared(world, *left, carrier_pickup)
                .total_cmp(&squad_distance_squared(world, *right, carrier_pickup))
                .then_with(|| left.cmp(right))
        });
        let passengers = take_transport_load(
            world,
            &mut remaining,
            execution.max_contained_population,
        );
        if !passengers.is_empty() {
            result.push(TransportGroup {
                passengers,
                pickup: carrier_pickup,
                dropoff: carrier_dropoff,
            });
        }
    }
    result
}

fn take_transport_load(
    world: &World,
    remaining: &mut Vec<EntityId>,
    capacity: f32,
) -> Vec<EntityId> {
    let mut loaded = Vec::new();
    let mut population = 0.0;
    let mut index = 0;
    while index < remaining.len() {
        let squad_population = rounded_transport_population(world, remaining[index]);
        if population + squad_population <= capacity {
            population += squad_population;
            loaded.push(remaining.remove(index));
        } else {
            index += 1;
        }
    }
    loaded
}

fn launch_transport_group(
    world: &mut World,
    database: &Database,
    execution: &TransportPowerExecution,
    group: TransportGroup,
) -> Option<EntityId> {
    let direction = horizontal_direction(group.dropoff - group.pickup).unwrap_or(Vec3::Z);
    let start = group.pickup - direction * execution.incoming_offset
        + Vec3::Y * execution.incoming_height;
    let pickup_target = group.pickup + Vec3::Y * execution.pickup_height;
    let dropoff_target = group.dropoff + Vec3::Y * execution.dropoff_height;
    let outgoing_target = group.dropoff
        + direction * execution.outgoing_offset
        + Vec3::Y * execution.outgoing_height;
    let (carrier_id, carrier_unit_id) = create_trigger_unit_squad(
        world,
        database,
        execution.player_id,
        execution.transport_prototype_id,
        start,
        direction,
        true,
    )?;
    if let Some(carrier) = world.get_squad_mut(carrier_id) {
        carrier.speed = execution.carrier_speed;
    }
    if let Some(carrier) = world.get_unit_mut(carrier_unit_id) {
        carrier.physics = None;
    }
    let started = world.start_power_transport_flight(
        carrier_id,
        SquadPowerTransportPlan {
            passenger_squad_ids: group.passengers,
            start_position: start,
            pickup_position: group.pickup,
            pickup_target,
            dropoff_position: group.dropoff,
            dropoff_target,
            outgoing_target,
        },
    );
    if !started {
        let _destroyed = world.kill_squad(carrier_id, true);
        return None;
    }
    Some(carrier_id)
}

fn transport_accepts_squad(
    world: &World,
    execution: &TransportPowerExecution,
    squad_id: EntityId,
) -> bool {
    execution.transport_contains.iter().any(|object_type| {
        world.squad_object_type_match(squad_id, object_type) == Some(true)
    })
}

fn rounded_transport_population(world: &World, squad_id: EntityId) -> f32 {
    world
        .get_squad(squad_id)
        .and_then(|squad| squad.population_costs.first())
        .map_or(0.0, |population| population.amount.max(0.0).round())
}

fn average_squad_position(world: &World, squads: &[EntityId]) -> Option<Vec3> {
    let mut total = Vec3::ZERO;
    let mut count = 0_u32;
    for squad_id in squads {
        if let Some(squad) = world.get_squad(*squad_id) {
            total += squad.base.position;
            count += 1;
        }
    }
    (count > 0).then(|| {
        ground_position(world, total / count.to_f32().unwrap_or(f32::MAX))
    })
}

fn ground_position(world: &World, mut position: Vec3) -> Vec3 {
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    position
}

fn squad_distance_squared(world: &World, squad_id: EntityId, location: Vec3) -> f32 {
    world.get_squad(squad_id).map_or(f32::INFINITY, |squad| {
        planar_distance_squared(squad.base.position, location)
    })
}

fn planar_distance_squared(left: Vec3, right: Vec3) -> f32 {
    let delta = left - right;
    delta.x * delta.x + delta.z * delta.z
}

fn horizontal_direction(direction: Vec3) -> Option<Vec3> {
    Vec3::new(direction.x, 0.0, direction.z).try_normalize()
}

impl TransportProfile {
    fn resolve(
        world: &World,
        database: &Database,
        attributes: &PowerAttributes,
        invocation: NativePowerInvocation,
    ) -> Result<Self, NativePowerError> {
        validate_level(attributes, invocation.power_level)?;
        let ui_radius = finite_positive(attributes.ui_radius, "UIRadius")?;
        let min_transport_distance = optional_float(
            attributes,
            invocation.power_level,
            "MinTransportDistance",
        )?
        .unwrap_or(ui_radius);
        if min_transport_distance < 0.0 {
            return Err(NativePowerError::InvalidData("MinTransportDistance"));
        }
        let max_ground_vehicles = optional_limit(
            attributes,
            invocation.power_level,
            "MaxGroundVehicles",
        )?;
        let max_infantry_units = optional_limit(
            attributes,
            invocation.power_level,
            "MaxInfantryUnits",
        )?;
        let (transport_prototype, transport_prototype_id, prototype) =
            player_transport_prototype(world, database, invocation.player_id)?;
        let max_contained_population = prototype
            .max_contained
            .filter(|value| *value > 0)
            .and_then(|value| value.to_f32())
            .ok_or(NativePowerError::InvalidData("TransportMaxContained"))?;
        let maximum_transports = database
            .game_data
            .as_ref()
            .and_then(|game_data| game_data.transport_max)
            .filter(|value| *value > 0)
            .ok_or(NativePowerError::InvalidData("TransportMax"))?;
        let settings = database.game_data.as_ref();
        Ok(Self {
            ui_radius,
            min_transport_distance,
            max_ground_vehicles,
            max_infantry_units,
            transport_prototype,
            transport_prototype_id,
            transport_contains: prototype.contain.clone(),
            max_contained_population,
            maximum_transports,
            carrier_speed: finite_positive(prototype.velocity, "TransportVelocity")?,
            carrier_spacing: carrier_spacing(prototype),
            incoming_height: setting(settings.and_then(|data| data.transport_incoming_height), 40.0),
            incoming_offset: setting(settings.and_then(|data| data.transport_incoming_offset), 60.0),
            outgoing_height: setting(settings.and_then(|data| data.transport_outgoing_height), 120.0),
            outgoing_offset: setting(settings.and_then(|data| data.transport_outgoing_offset), 60.0),
            pickup_height: setting(settings.and_then(|data| data.transport_pickup_height), 8.0),
            dropoff_height: setting(settings.and_then(|data| data.transport_dropoff_height), 15.0),
        })
    }
}

fn player_transport_prototype<'database>(
    world: &World,
    database: &'database Database,
    player_id: PlayerId,
) -> Result<(String, i32, &'database ProtoObject), NativePowerError> {
    let player = world
        .get_player(player_id)
        .ok_or(NativePowerError::PlayerNotFound(player_id))?;
    let civilization = usize::try_from(player.civ_id)
        .ok()
        .and_then(|index| database.civs.get(index))
        .ok_or(NativePowerError::InvalidData("Civilization"))?;
    let requested = civilization
        .transport
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or(NativePowerError::MissingData("TransportPrototype"))?;
    let prototype = database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(requested))
        .ok_or_else(|| NativePowerError::UnknownPrototype(requested.to_owned()))?;
    let prototype_id = object_prototype_id(database, &prototype.name)
        .ok_or_else(|| NativePowerError::UnknownPrototype(requested.to_owned()))?;
    Ok((prototype.name.clone(), prototype_id, prototype))
}

fn optional_limit(
    attributes: &PowerAttributes,
    level: u32,
    name: &'static str,
) -> Result<Option<u32>, NativePowerError> {
    optional_int(attributes, level, name)?
        .map(|value| u32::try_from(value).map_err(|_| NativePowerError::InvalidData(name)))
        .transpose()
}

fn finite_positive(value: Option<f32>, name: &'static str) -> Result<f32, NativePowerError> {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or(NativePowerError::InvalidData(name))
}

fn carrier_spacing(prototype: &ProtoObject) -> f32 {
    let radius = prototype
        .obstruction_radius_x
        .unwrap_or_default()
        .abs()
        .max(prototype.obstruction_radius_z.unwrap_or_default().abs());
    (radius * 2.0 + 2.0).max(8.0)
}

fn setting(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    checksum.hash_u32(u32::from(value.is_some()));
    if let Some(value) = value {
        hash_vec3(checksum, value);
    }
}

fn hash_optional_u32(checksum: &mut SyncChecksum, value: Option<u32>) {
    checksum.hash_u32(value.unwrap_or(u32::MAX));
}

#[cfg(test)]
mod tests;
