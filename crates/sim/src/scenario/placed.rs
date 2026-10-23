//! Retail scenario promotion of placed unit/building proto-objects into squads.

use super::prototypes::prototype_has_flag;
use super::{
    PlacedUnitKind, classify_proto_object, configure_unit_from_proto, create_squad_from_prototype,
    is_class_zero_object, refresh_squad_member_settings, valid_nonnegative,
};
use crate::entities::objects::is_icon_prototype;
use crate::entities::squads::marine::is_marine_squad;
use crate::entities::squads::warthog::{WarthogSquadSpec, is_warthog_squad};
use crate::entities::{IconObject, Object, SquadArchetype, SquadFormation};
use crate::player::GAIA_PLAYER;
use crate::prototypes::{
    ProtoSquadEntry, ScenarioProto, object_by_id, object_squad_id, proto_id, representing_squad,
};
use crate::{EntityId, World};
use pipeline::database::hw1::{Database, ProtoObject};

/// Owner and transform of a new entity.
#[derive(Debug, Clone, Copy)]
pub(super) struct Placement {
    /// Requested owner. A `ForceToGaiaPlayer` proto object belongs to Gaia instead.
    pub(super) player_id: u8,
    /// World position.
    pub(super) position: glam::Vec3,
    /// World forward direction.
    pub(super) forward: glam::Vec3,
}

#[derive(Debug, Clone, Copy)]
struct ProtoObjectSquadPlacement {
    player_id: u8,
    position: glam::Vec3,
    forward: glam::Vec3,
    start_built: bool,
}

/// Create the entities of one resolved scenario proto and return the created entity.
///
/// The proto reference resolves as in `BScenario_loadObject`; see
/// [`crate::prototypes::scenario_proto`]. An implicit proto squad places its object. The
/// sim does not place merged squads.
pub(super) fn create_scenario_object(
    world: &mut World,
    database: &Database,
    proto: ScenarioProto<'_>,
    proto_name: &str,
    placement: Placement,
) -> Option<EntityId> {
    let (prototype_index, prototype) = match proto {
        ScenarioProto::Squad(ProtoSquadEntry::Squad(_)) => {
            return Some(create_squad_from_prototype(
                world,
                placement.player_id,
                placement.position,
                placement.forward,
                proto_name.trim(),
                database,
            ));
        }
        ScenarioProto::Squad(ProtoSquadEntry::Merged { .. }) => return None,
        ScenarioProto::Squad(ProtoSquadEntry::Implicit {
            object_index,
            object: proto,
        }) => (object_index, proto),
        ScenarioProto::Object(index, proto) => (index, proto),
    };
    create_object_entity(world, database, prototype_index, prototype, placement)
        .map(|(entity_id, _)| entity_id)
}

/// Create one proto object like `BWorld_createEntity` with the object flag.
///
/// A class-zero object becomes a world object. A unit or building becomes a one-unit squad
/// with a completed unit. Returns the created entity (the object or the squad) and the unit.
pub(super) fn create_object_entity(
    world: &mut World,
    database: &Database,
    prototype_index: usize,
    prototype: &ProtoObject,
    placement: Placement,
) -> Option<(EntityId, Option<EntityId>)> {
    if is_class_zero_object(prototype) {
        let object_id = create_class_zero_object_at(
            world,
            placement.player_id,
            placement.position,
            placement.forward,
            proto_id(prototype_index),
            prototype,
        );
        return Some((object_id, None));
    }
    classify_proto_object(prototype)?;
    create_proto_object_squad(
        world,
        database,
        prototype_index,
        prototype,
        ProtoObjectSquadPlacement {
            player_id: placement.player_id,
            position: placement.position,
            forward: placement.forward,
            start_built: true,
        },
    )
    .map(|(squad_id, unit_id)| (squad_id, Some(unit_id)))
}

/// Owner of a new object: `BObject` initialization (`sub_14029C300`) gives a proto object
/// with the `ForceToGaiaPlayer` flag (`BProtoObject` `+0xB5` bit 4) to Gaia.
fn object_owner(requested_owner: u8, prototype: &ProtoObject) -> u8 {
    if prototype_has_flag(prototype, "ForceToGaiaPlayer") {
        GAIA_PLAYER
    } else {
        requested_owner
    }
}

pub(crate) fn create_class_zero_object_at(
    world: &mut World,
    requested_owner: u8,
    position: glam::Vec3,
    forward: glam::Vec3,
    prototype_id: i32,
    prototype: &ProtoObject,
) -> EntityId {
    let owner = object_owner(requested_owner, prototype);
    let id = world.objects.allocate_id();
    let object = if is_icon_prototype(prototype) {
        Object::new_icon(
            id,
            owner,
            position,
            forward,
            prototype_id,
            prototype.name.clone(),
            IconObject::from_prototype(prototype, None, false),
        )
    } else {
        Object::new_visual(
            id,
            owner,
            position,
            forward,
            prototype_id,
            prototype.name.clone(),
        )
    };
    world.objects.insert(id, object);
    // `BObject_initFromProto`: a proto object with a `PhysicsInfo` gets its body.
    world.queue_proto_object_physics(id);
    id
}

pub(crate) fn create_trigger_unit_squad(
    world: &mut World,
    database: &Database,
    player_id: u8,
    prototype_id: i32,
    position: glam::Vec3,
    forward: glam::Vec3,
    start_built: bool,
) -> Option<(EntityId, EntityId)> {
    world.get_player(player_id)?;
    let prototype = object_by_id(database, prototype_id)?;
    create_proto_object_squad(
        world,
        database,
        usize::try_from(prototype_id).ok()?,
        prototype,
        ProtoObjectSquadPlacement {
            player_id,
            position,
            forward,
            start_built,
        },
    )
}

fn create_proto_object_squad(
    world: &mut World,
    database: &Database,
    prototype_index: usize,
    prototype: &ProtoObject,
    placement: ProtoObjectSquadPlacement,
) -> Option<(EntityId, EntityId)> {
    let kind = classify_proto_object(prototype)?;
    let owner = object_owner(placement.player_id, prototype);
    let squad_id = world.create_squad_at(owner, placement.position);
    configure_synthetic_squad(
        world,
        squad_id,
        prototype_index,
        prototype,
        database,
        placement.forward,
    );

    let unit_id = match kind {
        PlacedUnitKind::Mobile => world.create_unit_at(owner, placement.position),
        PlacedUnitKind::Building => world.create_building_at(owner, placement.position),
    };
    configure_unit_from_proto(
        world,
        unit_id,
        prototype.name.trim(),
        prototype_index,
        prototype,
    );
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.base.set_forward(placement.forward);
        if kind == PlacedUnitKind::Building {
            unit.built = placement.start_built;
        }
    }
    if !world.attach_unit_to_squad(unit_id, squad_id) {
        let _removed_unit = world.remove_unit(unit_id);
        let _removed_squad = world.remove_squad(squad_id);
        return None;
    }
    if placement.start_built || kind == PlacedUnitKind::Mobile {
        super::population::apply_object_population(world, unit_id, database, prototype);
    } else {
        super::population::initialize_object_population(world, unit_id, database, prototype, false);
    }
    if kind == PlacedUnitKind::Building && placement.start_built && super::creates_base(prototype) {
        let _base_id = world.register_base(unit_id);
    }
    refresh_squad_member_settings(world, squad_id);
    Some((squad_id, unit_id))
}

/// Configure a squad that holds one unit of a proto object.
///
/// The squad gets the proto squad ID that the engine stores on the proto
/// object: the same-named one-unit squad, or else the implicit squad
/// ([`object_squad_id`]).
pub(super) fn configure_synthetic_squad(
    world: &mut World,
    squad_id: EntityId,
    prototype_index: usize,
    prototype: &ProtoObject,
    database: &Database,
    forward: glam::Vec3,
) {
    let proto_squad_id = object_squad_id(database, prototype_index).unwrap_or(-1);
    let Some(squad) = world.get_squad_mut(squad_id) else {
        return;
    };
    squad.base.set_forward(forward);
    squad.proto_squad_id = proto_squad_id;
    prototype.name.clone_into(&mut squad.proto_squad_name);
    squad.archetype = SquadArchetype::Generic;
    squad.formation = SquadFormation::Generic;
    squad.turn_radius = 0.0;
    squad.min_turn_radius = 0.0;
    squad.max_turn_radius = 0.0;
    squad.aggro_distance = 0.0;
    squad.leash_distance = 0.0;
    squad.configure_leash_profile(0.0, 0);
    if is_warthog_squad(&prototype.name) {
        let spec = WarthogSquadSpec::default();
        squad.archetype = SquadArchetype::Warthog;
        squad.turn_radius = spec.turn_radius;
        squad.min_turn_radius = spec.min_turn_radius;
        squad.max_turn_radius = spec.max_turn_radius;
    } else if is_marine_squad(&prototype.name) {
        let spec = crate::entities::squads::marine::MarineSquadSpec::default();
        squad.archetype = SquadArchetype::Marine;
        squad.formation = SquadFormation::Flock;
        squad.aggro_distance = spec.aggro_distance;
        squad.leash_distance = spec.leash_distance;
        squad.configure_leash_profile(spec.leash_deadzone, spec.leash_recall_delay_ms);
    }
    if let Some(proto) =
        representing_squad(database, prototype_index).and_then(|index| database.squads.get(index))
    {
        if let Some(aggro_distance) = valid_nonnegative(proto.aggro_distance) {
            squad.aggro_distance = aggro_distance;
        }
        if let Some(leash_distance) = valid_nonnegative(proto.leash_distance) {
            squad.leash_distance = leash_distance;
        }
        let deadzone = valid_nonnegative(proto.leash_deadzone).unwrap_or(squad.leash_deadzone());
        let recall_delay = proto
            .leash_recall_delay
            .unwrap_or_else(|| squad.leash_recall_delay_ms());
        squad.configure_leash_profile(deadzone, recall_delay);
    }
}

pub(crate) fn transform_synthetic_squad(
    world: &mut World,
    squad_id: EntityId,
    source_proto_object: &str,
    target_index: usize,
    target: &ProtoObject,
    database: &Database,
) {
    let Some(forward) = world.get_squad(squad_id).and_then(|squad| {
        squad
            .proto_squad_name
            .eq_ignore_ascii_case(source_proto_object)
            .then_some(squad.base.forward)
    }) else {
        return;
    };
    configure_synthetic_squad(world, squad_id, target_index, target, database, forward);
    refresh_squad_member_settings(world, squad_id);
}
