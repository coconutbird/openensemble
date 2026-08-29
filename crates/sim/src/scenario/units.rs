//! Player-prototype-aware unit creation and configuration.

use super::prototypes::{
    PlacedUnitKind, classify_proto_object, creates_base, database_id, find_proto_object,
    prototype_has_flag,
};
use super::{population, sockets, valid_nonnegative};
use crate::entities::squads::marine::MarineSquadSpec;
use crate::entities::units::configure_ground_vehicle_physics;
use crate::entities::units::marine::{MARINE_HITPOINTS, MarineUnitSpec, is_marine_unit};
use crate::entities::units::warthog::{WarthogUnitSpec, is_warthog_unit};
use crate::entities::{ShieldCoverage, SquadArchetype, UnitArchetype, UnitScalarModifiers};
use crate::physics::{BoxCollider, PhysicsBody};
use crate::player::{GAIA_PLAYER, PlayerId};
use crate::{EntityId, World};
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

pub(crate) fn add_squad_member_from_prototype(
    world: &mut World,
    squad_id: EntityId,
    proto_object_name: &str,
    database: &Database,
) -> Option<EntityId> {
    let (player_id, position, archetype, slot, veterancy_level) =
        world.get_squad(squad_id).map(|squad| {
            (
                squad.base.player_id,
                squad.base.position,
                squad.archetype,
                squad.unit_ids.len(),
                squad.veterancy_level(),
            )
        })?;
    let effective_name = resolved_unit_prototype_name(world, player_id, proto_object_name);
    let unit_id = world.create_unit_at(player_id, position);
    configure_unit(world, unit_id, proto_object_name, &effective_name, database);
    if !world.attach_unit_to_squad(unit_id, squad_id) {
        let _removed = world.remove_unit(unit_id);
        return None;
    }
    if archetype == SquadArchetype::Marine
        && let Some(offset) = MarineSquadSpec::default().initial_formation_offset(slot)
    {
        let assigned = world.set_squad_member_formation_offset(unit_id, offset);
        debug_assert!(assigned, "attached Marine should accept a formation offset");
    }
    if let Some((_, prototype)) = find_proto_object(database, &effective_name) {
        UnitScalarModifiers::from_veterancy_levels(&prototype.veterancy, 0, veterancy_level)
            .apply(world.get_unit_mut(unit_id)?);
        sockets::materialize_authored_sockets(world, unit_id, prototype, database);
    }
    Some(unit_id)
}

pub(crate) fn create_object_from_prototype(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_name: &str,
    database: &Database,
) -> Option<EntityId> {
    let (logical_index, logical_proto) = find_proto_object(database, proto_name)?;
    let effective_name = resolved_unit_prototype_name(world, player_id, proto_name);
    let (_, proto) = find_proto_object(database, &effective_name)?;
    let kind = classify_proto_object(proto)?;
    let unit_id = match kind {
        PlacedUnitKind::Mobile => world.create_unit_at(player_id, position),
        PlacedUnitKind::Building => world.create_building_at(player_id, position),
    };
    configure_unit_from_player_proto(
        world,
        unit_id,
        proto_name,
        database_id(logical_proto.dbid, logical_index),
        &effective_name,
        proto,
    );
    population::apply_object_population(world, unit_id, database, proto);
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.base.set_forward(forward);
    }
    sockets::materialize_authored_sockets(world, unit_id, proto, database);
    if kind == PlacedUnitKind::Building && creates_base(proto) {
        let _base_id = world.register_base(unit_id);
    }
    Some(unit_id)
}

pub(crate) fn create_unbuilt_building_from_prototype(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_name: &str,
    database: &Database,
) -> Option<EntityId> {
    let (logical_index, logical_proto) = find_proto_object(database, proto_name)?;
    let effective_name = resolved_unit_prototype_name(world, player_id, proto_name);
    let (_, proto) = find_proto_object(database, &effective_name)?;
    if classify_proto_object(proto) != Some(PlacedUnitKind::Building) {
        return None;
    }
    let unit_id = world.create_building_at(player_id, position);
    configure_unit_from_player_proto(
        world,
        unit_id,
        proto_name,
        database_id(logical_proto.dbid, logical_index),
        &effective_name,
        proto,
    );
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.built = false;
        unit.base.set_forward(forward);
    }
    sockets::materialize_authored_sockets(world, unit_id, proto, database);
    population::initialize_object_population(world, unit_id, database, proto, false);
    Some(unit_id)
}

fn configure_unit(
    world: &mut World,
    unit_id: EntityId,
    logical_name: &str,
    effective_name: &str,
    database: &Database,
) {
    let Some((logical_index, logical_proto)) = find_proto_object(database, logical_name) else {
        configure_unknown_unit(world, unit_id, logical_name);
        return;
    };
    let Some((_, effective_proto)) = find_proto_object(database, effective_name) else {
        return;
    };
    configure_unit_from_player_proto(
        world,
        unit_id,
        logical_name,
        database_id(logical_proto.dbid, logical_index),
        effective_name,
        effective_proto,
    );
}

fn configure_unknown_unit(world: &mut World, unit_id: EntityId, logical_name: &str) {
    let Some(unit) = world.get_unit_mut(unit_id) else {
        return;
    };
    logical_name.clone_into(&mut unit.proto_object_name);
    logical_name.clone_into(&mut unit.logical_proto_object_name);
    if is_warthog_unit(logical_name, None) {
        configure_warthog(unit, WarthogUnitSpec::default());
    } else if is_marine_unit(logical_name) {
        unit.set_max_hitpoints(MARINE_HITPOINTS);
        configure_marine(unit, MarineUnitSpec::default());
    }
}

pub(crate) fn configure_unit_from_proto(
    world: &mut World,
    unit_id: EntityId,
    proto_name: &str,
    proto_index: usize,
    proto: &ProtoObject,
) {
    configure_unit_from_player_proto(
        world,
        unit_id,
        proto_name,
        database_id(proto.dbid, proto_index),
        proto_name,
        proto,
    );
}

pub(crate) fn configure_unit_from_player_proto(
    world: &mut World,
    unit_id: EntityId,
    logical_name: &str,
    logical_id: i32,
    effective_name: &str,
    proto: &ProtoObject,
) {
    let shield_coverage = world
        .prototype_shield_coverage(effective_name)
        .unwrap_or_else(|| typed_shield_coverage(proto));
    let technologies = world
        .get_unit(unit_id)
        .and_then(|unit| world.get_player(unit.base.player_id))
        .map(|player| &player.technologies);
    let adjusted_hitpoints = proto
        .hitpoints
        .map(|base| technologies.map_or(base, |state| state.hitpoints(logical_name, base)));
    let base_shieldpoints = valid_nonnegative(proto.shieldpoints)
        .filter(|_| shield_coverage != ShieldCoverage::None)
        .unwrap_or_default();
    let shield_settings = technologies.map_or((base_shieldpoints, 1.0, 1.0), |state| {
        (
            state.shieldpoints(logical_name, base_shieldpoints),
            state.unit_shield_regen_rate(logical_name),
            state.unit_shield_regen_delay(logical_name),
        )
    });
    let adjusted_velocity = valid_nonnegative(proto.max_velocity.or(proto.velocity))
        .map(|base| technologies.map_or(base, |state| state.maximum_velocity(logical_name, base)));
    let ammunition_settings = ammunition_settings(technologies, logical_name, proto);
    let ground_vehicle_physics = world
        .prototype_ground_vehicle_physics(effective_name)
        .cloned();
    let Some(unit) = world.get_unit_mut(unit_id) else {
        return;
    };
    unit.proto_object_id = logical_id;
    effective_name.clone_into(&mut unit.proto_object_name);
    logical_name.clone_into(&mut unit.logical_proto_object_name);
    configure_unit_traits(
        unit,
        proto,
        adjusted_hitpoints,
        shield_coverage,
        shield_settings,
    );
    unit.ammunition.configure(
        ammunition_settings.0,
        ammunition_settings.1,
        prototype_has_flag(proto, "StartAtMaxAmmo"),
    );
    configure_unit_movement(
        unit,
        proto,
        effective_name,
        adjusted_velocity,
        ground_vehicle_physics.as_ref(),
    );
}

fn ammunition_settings(
    technologies: Option<&crate::player::PlayerTechState>,
    logical_name: &str,
    proto: &ProtoObject,
) -> (f32, f32) {
    let maximum = valid_nonnegative(proto.ammo_max).unwrap_or_default();
    let rate = valid_nonnegative(proto.ammo_regen_rate).unwrap_or_default();
    technologies.map_or((maximum, rate), |state| {
        (
            state.ammunition_maximum(logical_name, maximum),
            state.ammunition_regeneration_rate(logical_name, rate),
        )
    })
}

fn configure_unit_traits(
    unit: &mut crate::entities::Unit,
    proto: &ProtoObject,
    hitpoints: Option<f32>,
    shield_coverage: ShieldCoverage,
    shield_settings: (f32, f32, f32),
) {
    let prototype_non_mobile = unit.is_building() || prototype_has_flag(proto, "Immoveable");
    unit.base.configure_prototype_mobility(prototype_non_mobile);
    unit.set_auto_attackable(!prototype_has_flag(proto, "DontAutoAttackMe"));
    unit.set_invulnerable(
        prototype_has_flag(proto, "Invulnerable")
            || (unit.base.player_id == GAIA_PLAYER
                && prototype_has_flag(proto, "InvulnerableWhenGaia")),
    );
    unit.set_external_shield(prototype_has_flag(proto, "ExternalShield"));
    super::garrison::configure_unit(unit, proto);
    if let Some(hitpoints) = hitpoints {
        unit.set_max_hitpoints(hitpoints);
    }
    unit.shields.configure(shield_coverage, shield_settings.0);
    unit.shields
        .set_regen_scalars(shield_settings.1, shield_settings.2);
}

fn configure_unit_movement(
    unit: &mut crate::entities::Unit,
    proto: &ProtoObject,
    effective_name: &str,
    adjusted_velocity: Option<f32>,
    ground_vehicle_physics: Option<&crate::gameplay::GroundVehiclePhysicsProfile>,
) {
    if !unit.is_building()
        && let Some(speed) = adjusted_velocity
    {
        unit.speed = speed;
    }
    unit.acceleration = valid_nonnegative(proto.acceleration).unwrap_or_default();
    unit.turn_rate_degrees = valid_nonnegative(proto.turn_rate).unwrap_or_default();
    unit.obstruction_half_extents = obstruction_half_extents(proto).unwrap_or(Vec3::ZERO);
    if is_warthog_unit(effective_name, proto.physics_info.as_deref()) {
        configure_warthog(unit, warthog_spec_from_proto(proto, adjusted_velocity));
    } else if is_marine_unit(effective_name) {
        configure_marine(unit, marine_spec_from_proto(proto, adjusted_velocity));
    } else if unit.is_building()
        && let Some(collider) = obstruction_collider(proto)
    {
        unit.physics = Some(PhysicsBody::static_obstruction(collider));
    } else if let Some(profile) = ground_vehicle_physics {
        configure_ground_vehicle_physics(unit, profile);
    }
}

fn resolved_unit_prototype_name(world: &World, player_id: PlayerId, logical_name: &str) -> String {
    world.get_player(player_id).map_or_else(
        || logical_name.to_owned(),
        |player| {
            player
                .technologies
                .resolved_unit_prototype(logical_name)
                .to_owned()
        },
    )
}

fn typed_shield_coverage(proto: &ProtoObject) -> ShieldCoverage {
    if proto
        .damage_type
        .as_deref()
        .is_some_and(|damage_type| damage_type.eq_ignore_ascii_case("Shielded"))
    {
        ShieldCoverage::Full
    } else {
        ShieldCoverage::None
    }
}

fn configure_warthog(unit: &mut crate::entities::Unit, spec: WarthogUnitSpec) {
    unit.archetype = UnitArchetype::Warthog;
    unit.speed = spec.max_speed;
    unit.acceleration = spec.acceleration;
    unit.turn_rate_degrees = spec.turn_rate_degrees;
    unit.obstruction_half_extents = spec.half_extents;
    unit.physics = Some(spec.physics_body(unit.base.position.y));
}

fn configure_marine(unit: &mut crate::entities::Unit, spec: MarineUnitSpec) {
    unit.archetype = UnitArchetype::Marine;
    unit.speed = spec.max_speed;
    unit.acceleration = spec.acceleration;
    unit.turn_rate_degrees = spec.turn_rate_degrees;
    unit.obstruction_half_extents = spec.half_extents;
    unit.physics = None;
}

fn warthog_spec_from_proto(proto: &ProtoObject, max_speed: Option<f32>) -> WarthogUnitSpec {
    let mut spec = WarthogUnitSpec::default();
    spec.max_speed = max_speed.unwrap_or(spec.max_speed);
    spec.acceleration = valid_nonnegative(proto.acceleration).unwrap_or(spec.acceleration);
    spec.turn_rate_degrees = valid_nonnegative(proto.turn_rate).unwrap_or(spec.turn_rate_degrees);
    spec.half_extents.x = valid_positive(proto.obstruction_radius_x).unwrap_or(spec.half_extents.x);
    spec.half_extents.y = valid_positive(proto.obstruction_radius_y).unwrap_or(spec.half_extents.y);
    spec.half_extents.z = valid_positive(proto.obstruction_radius_z).unwrap_or(spec.half_extents.z);
    spec
}

fn marine_spec_from_proto(proto: &ProtoObject, max_speed: Option<f32>) -> MarineUnitSpec {
    let mut spec = MarineUnitSpec::default();
    spec.max_speed = max_speed.unwrap_or(spec.max_speed);
    spec.acceleration = valid_nonnegative(proto.acceleration).unwrap_or(spec.acceleration);
    spec.turn_rate_degrees = valid_nonnegative(proto.turn_rate).unwrap_or(spec.turn_rate_degrees);
    spec.half_extents = obstruction_half_extents(proto).unwrap_or(spec.half_extents);
    spec
}

fn obstruction_half_extents(proto: &ProtoObject) -> Option<Vec3> {
    let x = valid_positive(proto.obstruction_radius_x)?;
    let z = valid_positive(proto.obstruction_radius_z)?;
    let y = valid_positive(proto.obstruction_radius_y).unwrap_or(1.0);
    Some(Vec3::new(x, y, z))
}

fn obstruction_collider(proto: &ProtoObject) -> Option<BoxCollider> {
    Some(BoxCollider::new(
        obstruction_half_extents(proto)?,
        Vec3::ZERO,
    ))
}

fn valid_positive(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}
