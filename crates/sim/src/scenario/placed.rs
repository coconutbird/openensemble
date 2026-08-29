//! Retail scenario promotion of placed unit/building proto-objects into squads.

use super::{
    PlacedUnitKind, ScenarioObject, classify_proto_object, configure_unit_from_proto,
    create_scenario_squad, database_id, find_proto_object, find_proto_squad, is_class_zero_object,
    refresh_squad_member_settings, scenario_forward, scenario_position, valid_nonnegative,
};
use crate::entities::objects::is_icon_prototype;
use crate::entities::squads::marine::is_marine_squad;
use crate::entities::squads::warthog::{WarthogSquadSpec, is_warthog_squad};
use crate::entities::{IconObject, Object, SquadArchetype, SquadFormation};
use crate::{EntityId, World};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[derive(Debug, Clone, Copy)]
struct ProtoObjectSquadPlacement {
    player_id: u8,
    position: glam::Vec3,
    forward: glam::Vec3,
    start_built: bool,
}

pub(super) fn create_scenario_object(
    world: &mut World,
    object: &ScenarioObject,
    database: &Database,
) -> Option<EntityId> {
    if object.is_squad {
        return Some(create_scenario_squad(world, object, database));
    }
    let (prototype_index, prototype) = find_proto_object(database, object.proto_name.trim())?;
    if is_class_zero_object(prototype) {
        return Some(create_class_zero_object(
            world,
            object,
            prototype_index,
            prototype,
        ));
    }
    classify_proto_object(prototype)?;
    create_proto_object_squad(
        world,
        database,
        prototype_index,
        prototype,
        ProtoObjectSquadPlacement {
            player_id: u8::try_from(object.player).unwrap_or_default(),
            position: scenario_position(object),
            forward: scenario_forward(object),
            start_built: true,
        },
    )
    .map(|(squad_id, _)| squad_id)
}

fn create_class_zero_object(
    world: &mut World,
    placement: &ScenarioObject,
    prototype_index: usize,
    prototype: &ProtoObject,
) -> EntityId {
    let id = world.objects.allocate_id();
    let owner = u8::try_from(placement.player).unwrap_or_default();
    let position = scenario_position(placement);
    let forward = scenario_forward(placement);
    let prototype_id = database_id(prototype.dbid, prototype_index);
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
    let (prototype_index, prototype) = database
        .objects
        .iter()
        .enumerate()
        .find(|(index, prototype)| database_id(prototype.dbid, *index) == prototype_id)?;
    create_proto_object_squad(
        world,
        database,
        prototype_index,
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
    let squad_id = world.create_squad_at(placement.player_id, placement.position);
    configure_synthetic_squad(world, squad_id, prototype, database, placement.forward);

    let unit_id = match kind {
        PlacedUnitKind::Mobile => world.create_unit_at(placement.player_id, placement.position),
        PlacedUnitKind::Building => {
            world.create_building_at(placement.player_id, placement.position)
        }
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
    super::sockets::materialize_authored_sockets(world, unit_id, prototype, database);
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

fn configure_synthetic_squad(
    world: &mut World,
    squad_id: EntityId,
    prototype: &ProtoObject,
    database: &Database,
    forward: glam::Vec3,
) {
    let proto_squad_id = synthetic_proto_squad_id(database, prototype);
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
    }
    if let Some((_, proto)) = matching_proto_squad(database, &prototype.name) {
        if let Some(aggro_distance) = valid_nonnegative(proto.aggro_distance) {
            squad.aggro_distance = aggro_distance;
        }
        if let Some(leash_distance) = valid_nonnegative(proto.leash_distance) {
            squad.leash_distance = leash_distance;
        }
    }
}

pub(crate) fn transform_synthetic_squad(
    world: &mut World,
    squad_id: EntityId,
    source_proto_object: &str,
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
    configure_synthetic_squad(world, squad_id, target, database, forward);
    refresh_squad_member_settings(world, squad_id);
}

fn synthetic_proto_squad_id(database: &Database, prototype: &ProtoObject) -> i32 {
    if let Some((index, squad)) = matching_proto_squad(database, &prototype.name) {
        return database_id(squad.dbid, index);
    }
    let mut next_id = database.squads.len();
    for candidate in &database.objects {
        if classify_proto_object(candidate).is_none()
            || matching_proto_squad(database, &candidate.name).is_some()
        {
            continue;
        }
        if candidate.name.eq_ignore_ascii_case(&prototype.name) {
            return i32::try_from(next_id).unwrap_or(-1);
        }
        next_id += 1;
    }
    -1
}

fn matching_proto_squad<'a>(
    database: &'a Database,
    proto_object_name: &str,
) -> Option<(usize, &'a ProtoSquad)> {
    let (index, squad) = find_proto_squad(database, proto_object_name)?;
    let units = squad.units.as_ref()?;
    let [unit] = units.entries.as_slice() else {
        return None;
    };
    (unit.count == 1 && unit.proto_object.eq_ignore_ascii_case(proto_object_name))
        .then_some((index, squad))
}
