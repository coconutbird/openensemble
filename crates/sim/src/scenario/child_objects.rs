//! Source-backed activation of authored child objects when a unit becomes built.

use super::{
    PlacedUnitKind, classify_proto_object, configure_unit_from_proto, find_proto_object,
    find_proto_squad, population,
    prototypes::{database_id, is_class_zero_object, prototype_has_flag},
};
use crate::entities::objects::is_icon_prototype;
use crate::entities::units::AuthoredUnitChildKind;
use crate::entities::{BaseId, IconObject, Object, Unit};
use crate::entity_id::EntityId;
use crate::player::{Player, PlayerId};
use crate::world::sockets::authored_socket_transform;
use crate::world::{TriggerTrainingRequest, World};
use glam::Vec3;
use pipeline::database::hw1::objects::{ChildObject, ChildObjectType, ProtoObject};
use pipeline::database::hw1::{Database, Vector3};

const MAX_CHILD_DEPTH: usize = 16;

#[derive(Clone, Copy)]
struct ParentContext {
    id: EntityId,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    built: bool,
    base_id: Option<BaseId>,
}

/// Materialize every retail child-object category represented by the database.
///
/// Retail calls `BUnit::createChildObjects` from `BUnit::onBuilt`, including
/// after a built prototype transforms. Entity children retain their typed
/// relationships, rally markers become unit state, and one-time squads route
/// through normal no-cost training.
pub(crate) fn materialize_authored_child_objects(
    world: &mut World,
    parent_id: EntityId,
    parent_proto: &ProtoObject,
    database: &Database,
) {
    let mut ancestors = vec![parent_proto.name.to_ascii_lowercase()];
    materialize_recursive(world, parent_id, parent_proto, database, &mut ancestors);
}

fn materialize_recursive(
    world: &mut World,
    parent_id: EntityId,
    parent_proto: &ProtoObject,
    database: &Database,
    ancestors: &mut Vec<String>,
) {
    if ancestors.len() > MAX_CHILD_DEPTH {
        return;
    }
    let Some(parent) = world.get_unit(parent_id).map(|parent| ParentContext {
        id: parent_id,
        player_id: parent.base.player_id,
        position: parent.base.position,
        forward: parent.base.forward,
        built: parent.built,
        base_id: parent.base_id,
    }) else {
        return;
    };
    let children = parent_proto
        .child_objects
        .as_ref()
        .map_or(&[][..], |children| children.objects.as_slice());
    let mut rally_point = None;
    for child in children {
        if !matches_user_civilization(world, parent.player_id, database, child) {
            continue;
        }
        match child.child_type.unwrap_or(ChildObjectType::Object) {
            ChildObjectType::Rally => {
                rally_point =
                    Some(child_transform(world, parent.position, parent.forward, child).0);
            }
            ChildObjectType::Socket => {
                materialize_socket(world, parent, child, database, ancestors);
            }
            ChildObjectType::OneTimeSpawnSquad => {
                spawn_one_time_squad(world, parent.id, parent.player_id, child, database);
            }
            ChildObjectType::Unit => materialize_associated_unit(
                world,
                parent,
                child,
                database,
                ancestors,
                AuthoredUnitChildKind::Unit,
            ),
            ChildObjectType::Foundation => materialize_associated_unit(
                world,
                parent,
                child,
                database,
                ancestors,
                AuthoredUnitChildKind::Foundation,
            ),
            ChildObjectType::Object => {
                materialize_generic_object(world, parent, child, database, ancestors);
            }
            ChildObjectType::ParkingLot => {
                materialize_parking_lot(world, parent, child, database, ancestors);
            }
        }
    }
    if let Some(rally_point) = rally_point {
        apply_authored_rally_point(world, parent.id, parent.player_id, rally_point);
    }
}

fn spawn_one_time_squad(
    world: &mut World,
    parent_id: EntityId,
    player_id: PlayerId,
    child: &ChildObject,
    database: &Database,
) {
    let Some((prototype_index, _prototype)) = find_proto_squad(database, child.proto_object.trim())
    else {
        return;
    };
    let Ok(prototype_id) = i32::try_from(prototype_index) else {
        return;
    };
    if world
        .get_player(player_id)
        .is_none_or(|player| player.has_used_one_time_spawn(prototype_id))
    {
        return;
    }
    let Ok(result) = world.queue_trigger_training(TriggerTrainingRequest {
        player_id,
        building_id: parent_id,
        database,
        prototype_id,
        count: 1,
        no_cost: true,
        trigger_state: None,
    }) else {
        return;
    };
    if result.queue.accepted > 0
        && let Some(player) = world.get_player_mut(player_id)
    {
        let _marked = player.mark_one_time_spawn_used(prototype_id);
    }
}

fn materialize_socket(
    world: &mut World,
    parent: ParentContext,
    child: &ChildObject,
    database: &Database,
    ancestors: &mut Vec<String>,
) {
    let Some((prototype_index, prototype)) = find_proto_object(database, child.proto_object.trim())
    else {
        return;
    };
    let prototype_key = prototype.name.to_ascii_lowercase();
    if ancestors.contains(&prototype_key) {
        return;
    }
    let local_offset = child.offset.map_or(Vec3::ZERO, vector_to_vec3);
    let local_yaw = child
        .rotation
        .filter(|value| value.is_finite())
        .unwrap_or(0.0);
    let (position, forward) = child_transform(world, parent.position, parent.forward, child);
    if existing_socket(world, parent.id, position).is_some() {
        return;
    }
    let socket_id = match classify_proto_object(prototype) {
        Some(PlacedUnitKind::Building) => world.create_building_at(parent.player_id, position),
        _ => world.create_unit_at(parent.player_id, position),
    };
    configure_unit_from_proto(
        world,
        socket_id,
        prototype.name.trim(),
        prototype_index,
        prototype,
    );
    if let Some(socket) = world.get_unit_mut(socket_id) {
        socket.base.set_forward(forward);
        if prototype_has_flag(prototype, "NotSelectableWhenChildObject") {
            socket.base.set_selectable(false);
        }
        socket.built = parent.built;
        socket.built_by = Some(parent.id);
        socket.base_id = parent.base_id;
    }
    if !world.associate_socket_at(parent.id, socket_id, local_offset, local_yaw) {
        let _removed = world.remove_unit(socket_id);
        return;
    }
    population::initialize_object_population(world, socket_id, database, prototype, parent.built);
    if parent.built {
        let _activated = world.activate_unit_built_economy(socket_id, database, prototype);
    }
    ancestors.push(prototype_key);
    materialize_recursive(world, socket_id, prototype, database, ancestors);
    let _popped = ancestors.pop();
}

fn materialize_associated_unit(
    world: &mut World,
    parent: ParentContext,
    child: &ChildObject,
    database: &Database,
    ancestors: &mut Vec<String>,
    kind: AuthoredUnitChildKind,
) {
    let Some((prototype_index, prototype)) = find_proto_object(database, child.proto_object.trim())
    else {
        return;
    };
    let prototype_key = prototype.name.to_ascii_lowercase();
    if ancestors.contains(&prototype_key) {
        return;
    }
    let (position, forward) = child_transform(world, parent.position, parent.forward, child);
    if kind == AuthoredUnitChildKind::Foundation {
        remove_replaced_foundation(world, parent.id, position);
    } else if existing_authored_unit(world, parent.id, prototype, position, kind).is_some() {
        return;
    }
    let child_id = match classify_proto_object(prototype) {
        Some(PlacedUnitKind::Building) => world.create_building_at(parent.player_id, position),
        Some(PlacedUnitKind::Mobile) => world.create_unit_at(parent.player_id, position),
        None => return,
    };
    configure_unit_from_proto(
        world,
        child_id,
        prototype.name.trim(),
        prototype_index,
        prototype,
    );
    if let Some(unit) = world.get_unit_mut(child_id) {
        unit.base.set_forward(forward);
        if prototype_has_flag(prototype, "NotSelectableWhenChildObject") {
            unit.base.set_selectable(false);
        }
        unit.built = parent.built;
    }
    if !world.associate_authored_child(parent.id, child_id, kind) {
        let _removed = world.remove_unit(child_id);
        return;
    }
    population::initialize_object_population(world, child_id, database, prototype, parent.built);
    if parent.built {
        let _activated = world.activate_unit_built_economy(child_id, database, prototype);
    }
    ancestors.push(prototype_key);
    materialize_recursive(world, child_id, prototype, database, ancestors);
    let _popped = ancestors.pop();
}

fn materialize_generic_object(
    world: &mut World,
    parent: ParentContext,
    child: &ChildObject,
    database: &Database,
    ancestors: &mut Vec<String>,
) {
    let Some((_, prototype)) = find_proto_object(database, child.proto_object.trim()) else {
        return;
    };
    if is_class_zero_object(prototype) {
        materialize_associated_object(world, parent, child, database);
    } else {
        materialize_associated_unit(
            world,
            parent,
            child,
            database,
            ancestors,
            AuthoredUnitChildKind::Building,
        );
    }
}

fn materialize_associated_object(
    world: &mut World,
    parent: ParentContext,
    child: &ChildObject,
    database: &Database,
) {
    let Some((prototype_index, prototype)) = find_proto_object(database, child.proto_object.trim())
    else {
        return;
    };
    if !is_class_zero_object(prototype) {
        return;
    }
    let (position, forward) = child_transform(world, parent.position, parent.forward, child);
    if existing_authored_object(world, parent.id, prototype, position).is_some() {
        return;
    }
    let object_id = world.objects.allocate_id();
    let prototype_id = database_id(prototype.dbid, prototype_index);
    let object = if is_icon_prototype(prototype) {
        Object::new_icon(
            object_id,
            parent.player_id,
            position,
            forward,
            prototype_id,
            prototype.name.clone(),
            IconObject::from_prototype(prototype, None, false),
        )
    } else {
        Object::new_visual(
            object_id,
            parent.player_id,
            position,
            forward,
            prototype_id,
            prototype.name.clone(),
        )
    };
    world.objects.insert(object_id, object);
    if !world.associate_authored_child(parent.id, object_id, AuthoredUnitChildKind::Object) {
        let _removed = world.remove_object(object_id);
    }
}

fn materialize_parking_lot(
    world: &mut World,
    parent: ParentContext,
    child: &ChildObject,
    database: &Database,
    ancestors: &mut Vec<String>,
) {
    if world
        .get_unit(parent.id)
        .and_then(Unit::associated_parking_lot)
        .is_some_and(|parking_id| world.get_unit(parking_id).is_some())
    {
        return;
    }
    let Some((prototype_index, prototype)) = find_proto_object(database, child.proto_object.trim())
    else {
        return;
    };
    let prototype_key = prototype.name.to_ascii_lowercase();
    if ancestors.contains(&prototype_key) {
        return;
    }
    let (position, forward) = child_transform(world, parent.position, parent.forward, child);
    let parking_id = match classify_proto_object(prototype) {
        Some(PlacedUnitKind::Building) => world.create_building_at(parent.player_id, position),
        Some(PlacedUnitKind::Mobile) => world.create_unit_at(parent.player_id, position),
        None => return,
    };
    configure_unit_from_proto(
        world,
        parking_id,
        prototype.name.trim(),
        prototype_index,
        prototype,
    );
    if let Some(parking) = world.get_unit_mut(parking_id) {
        parking.base.set_forward(forward);
        parking.built = parent.built;
        parking.built_by = Some(parent.id);
        if prototype_has_flag(prototype, "NotSelectableWhenChildObject") {
            parking.base.set_selectable(false);
        }
    }
    if !world.associate_parking_lot(parent.id, parking_id) {
        let _removed = world.remove_unit(parking_id);
        return;
    }
    population::initialize_object_population(world, parking_id, database, prototype, parent.built);
    if parent.built {
        let _activated = world.activate_unit_built_economy(parking_id, database, prototype);
    }
    ancestors.push(prototype_key);
    materialize_recursive(world, parking_id, prototype, database, ancestors);
    let _popped = ancestors.pop();
}

fn remove_replaced_foundation(world: &mut World, parent_id: EntityId, position: Vec3) {
    let foundation_id = world.get_unit(parent_id).and_then(|parent| {
        parent
            .associated_foundations()
            .iter()
            .copied()
            .find(|&foundation_id| {
                world
                    .get_unit(foundation_id)
                    .is_some_and(|foundation| foundation.base.position.distance(position) < 1.0)
            })
    });
    if let Some(foundation_id) = foundation_id {
        let _killed = world.kill_entity(foundation_id, true);
    }
}

fn existing_authored_unit(
    world: &World,
    parent_id: EntityId,
    prototype: &ProtoObject,
    position: Vec3,
    kind: AuthoredUnitChildKind,
) -> Option<EntityId> {
    let parent = world.get_unit(parent_id)?;
    let child_ids = match kind {
        AuthoredUnitChildKind::Building => parent.associated_child_buildings(),
        AuthoredUnitChildKind::Unit => parent.associated_child_units(),
        AuthoredUnitChildKind::Foundation => parent.associated_foundations(),
        AuthoredUnitChildKind::Object => return None,
    };
    child_ids.iter().copied().find(|&child_id| {
        world.get_unit(child_id).is_some_and(|child| {
            child
                .proto_object_name
                .eq_ignore_ascii_case(&prototype.name)
                && child.base.position.distance(position) < 1.0
        })
    })
}

fn existing_authored_object(
    world: &World,
    parent_id: EntityId,
    prototype: &ProtoObject,
    position: Vec3,
) -> Option<EntityId> {
    world
        .get_unit(parent_id)?
        .associated_child_objects()
        .iter()
        .copied()
        .find(|&object_id| {
            world.get_object(object_id).is_some_and(|object| {
                object
                    .proto_object_name
                    .eq_ignore_ascii_case(&prototype.name)
                    && object.base.position.distance(position) < 1.0
            })
        })
}

fn existing_socket(world: &World, parent_id: EntityId, position: Vec3) -> Option<EntityId> {
    world
        .get_unit(parent_id)?
        .associated_sockets()
        .iter()
        .copied()
        .find(|&socket_id| {
            world
                .get_unit(socket_id)
                .is_some_and(|socket| socket.base.position.distance(position) < 1.0)
        })
}

fn apply_authored_rally_point(
    world: &mut World,
    parent_id: EntityId,
    player_id: PlayerId,
    rally_point: Vec3,
) {
    let target_id = world
        .get_unit(parent_id)
        .and_then(Unit::associated_parking_lot)
        .unwrap_or(parent_id);
    if world.player_rally_point(player_id).is_some()
        || world.unit_rally_point(target_id, player_id).is_some()
    {
        return;
    }
    let _set = world.set_unit_rally_point(target_id, player_id, rally_point, None);
    let coop_player_id = world
        .is_coop()
        .then(|| world.get_player(player_id).and_then(Player::coop_player_id))
        .flatten()
        .filter(|&coop_id| world.players_are_allied(player_id, coop_id));
    if let Some(coop_player_id) = coop_player_id {
        let _set = world.set_unit_rally_point(target_id, coop_player_id, rally_point, None);
    }
}

fn matches_user_civilization(
    world: &World,
    player_id: PlayerId,
    database: &Database,
    child: &ChildObject,
) -> bool {
    let Some(required) = child
        .user_civilization
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return true;
    };
    world
        .get_player(player_id)
        .and_then(|player| usize::try_from(player.civ_id).ok())
        .and_then(|civ_id| database.civs.get(civ_id))
        .is_some_and(|civilization| civilization.name.eq_ignore_ascii_case(required))
}

fn child_transform(
    world: &World,
    parent_position: Vec3,
    parent_forward: Vec3,
    child: &ChildObject,
) -> (Vec3, Vec3) {
    let (mut position, forward) = authored_socket_transform(
        parent_position,
        parent_forward,
        child.offset,
        child.rotation,
    );
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    (position, forward)
}

const fn vector_to_vec3(value: Vector3) -> Vec3 {
    Vec3::new(value.x, value.y, value.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::Squad as ProtoSquad;
    use pipeline::database::hw1::objects::{ChildObjects, ObjectCommand};
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

    #[test]
    fn authored_socket_entities_keep_order_transform_and_prototype_types() {
        let database = socket_database();
        let mut world = World::new();
        let parent_id = world.create_building_at(1, Vec3::new(10.0, 0.0, 20.0));
        world
            .get_unit_mut(parent_id)
            .unwrap()
            .base
            .set_forward(Vec3::X);

        materialize_authored_child_objects(&mut world, parent_id, &database.objects[0], &database);

        let socket_ids = world
            .get_unit(parent_id)
            .unwrap()
            .associated_sockets()
            .to_vec();
        assert_eq!(socket_ids.len(), 2);
        let building_socket = world.get_unit(socket_ids[0]).unwrap();
        assert!(building_socket.is_object_type("BuildingSocket"));
        assert!(
            building_socket
                .base
                .position
                .abs_diff_eq(Vec3::new(13.0, 0.0, 18.0), 1.0e-6)
        );
        let turret_socket = world.get_unit(socket_ids[1]).unwrap();
        assert!(turret_socket.is_object_type("TurretSocket"));
        assert!(
            turret_socket
                .base
                .position
                .abs_diff_eq(Vec3::new(11.0, 0.0, 24.0), 1.0e-6)
        );
    }

    #[test]
    fn authored_rally_point_targets_an_existing_parking_lot() {
        let database = Database {
            objects: vec![ProtoObject {
                name: "parent".to_owned(),
                child_objects: Some(ChildObjects {
                    objects: vec![ChildObject {
                        proto_object: "rally_marker".to_owned(),
                        child_type: Some(ChildObjectType::Rally),
                        offset: Some(Vector3 {
                            x: 2.0,
                            y: 0.0,
                            z: 3.0,
                        }),
                        ..ChildObject::default()
                    }],
                }),
                ..ProtoObject::default()
            }],
            ..Database::default()
        };
        let mut world = World::new();
        world.init_players(1);
        let parent_id = world.create_building_at(1, Vec3::new(10.0, 0.0, 20.0));
        let parking_id = world.create_building_at(1, Vec3::ZERO);
        world
            .get_unit_mut(parent_id)
            .unwrap()
            .base
            .set_forward(Vec3::X);
        assert!(world.associate_parking_lot(parent_id, parking_id));

        materialize_authored_child_objects(&mut world, parent_id, &database.objects[0], &database);

        assert_eq!(world.unit_rally_point(parent_id, 1), None);
        let rally = world.unit_rally_point(parking_id, 1).unwrap();
        assert!(
            rally
                .position()
                .abs_diff_eq(Vec3::new(13.0, 0.0, 18.0), 1.0e-6)
        );
    }

    #[test]
    fn one_time_spawn_squad_uses_no_cost_training_only_once_per_player() {
        let database = one_time_spawn_database();
        let mut world = World::new();
        world.init_players(1);
        for _ in 0..2 {
            let parent_id = world.create_building(1);
            configure_unit_from_proto(&mut world, parent_id, "temple", 0, &database.objects[0]);
            materialize_authored_child_objects(
                &mut world,
                parent_id,
                &database.objects[0],
                &database,
            );
        }

        let heroes = world
            .squads
            .iter()
            .filter(|(_, squad)| squad.proto_squad_name == "hero_squad")
            .count();
        assert_eq!(heroes, 1);
        assert!(world.get_player(1).unwrap().has_used_one_time_spawn(0));
    }

    #[test]
    fn authored_foundation_and_unit_reconcile_and_follow_parent_lifetime() {
        let database = associated_unit_database();
        let mut world = World::new();
        let parent_id = world.create_building_at(1, Vec3::new(10.0, 0.0, 20.0));
        configure_unit_from_proto(&mut world, parent_id, "base", 0, &database.objects[0]);
        world
            .get_unit_mut(parent_id)
            .unwrap()
            .base
            .set_forward(Vec3::X);

        materialize_authored_child_objects(&mut world, parent_id, &database.objects[0], &database);

        let first_foundation = world.get_unit(parent_id).unwrap().associated_foundations()[0];
        let first_turret = world.get_unit(parent_id).unwrap().associated_child_units()[0];
        let beacon_id = world
            .get_unit(parent_id)
            .unwrap()
            .associated_child_objects()[0];
        let parking_id = world
            .get_unit(parent_id)
            .unwrap()
            .associated_parking_lot()
            .unwrap();
        assert!(
            world
                .get_unit(first_foundation)
                .unwrap()
                .is_object_type("foundation")
        );
        assert!(
            world
                .get_unit(first_turret)
                .unwrap()
                .is_object_type("turret")
        );
        assert_eq!(
            world.get_object(beacon_id).unwrap().proto_object_name,
            "beacon"
        );
        assert!(
            world
                .get_unit(parking_id)
                .unwrap()
                .is_object_type("parking")
        );
        assert!(
            world
                .get_unit(first_turret)
                .unwrap()
                .base
                .position
                .abs_diff_eq(Vec3::new(14.0, 0.0, 18.0), 1.0e-6)
        );

        materialize_authored_child_objects(&mut world, parent_id, &database.objects[0], &database);

        let parent = world.get_unit(parent_id).unwrap();
        let replacement_foundation = parent.associated_foundations()[0];
        assert_ne!(replacement_foundation, first_foundation);
        assert!(world.get_unit(first_foundation).is_none());
        assert_eq!(parent.associated_child_units(), &[first_turret]);
        assert_eq!(parent.associated_child_objects(), &[beacon_id]);
        assert_eq!(parent.associated_parking_lot(), Some(parking_id));
        assert!(world.kill_unit(parent_id, false));
        assert!(
            world
                .get_unit(replacement_foundation)
                .is_some_and(|foundation| !foundation.base.is_alive())
        );
        assert!(
            world
                .get_unit(first_turret)
                .is_some_and(|turret| !turret.base.is_alive())
        );
        assert!(world.get_object(beacon_id).is_none());
        assert!(
            world
                .get_unit(parking_id)
                .is_some_and(|parking| !parking.base.is_alive())
        );
    }

    fn socket_database() -> Database {
        Database {
            objects: vec![
                ProtoObject {
                    name: "parent".to_owned(),
                    object_class: Some("Building".to_owned()),
                    child_objects: Some(ChildObjects {
                        objects: vec![
                            socket_child("building_socket", 2.0, 3.0, 90.0),
                            socket_child("turret_socket", -4.0, 1.0, -90.0),
                        ],
                    }),
                    ..ProtoObject::default()
                },
                socket_prototype("building_socket", "BuildingSocket"),
                socket_prototype("turret_socket", "TurretSocket"),
            ],
            ..Database::default()
        }
    }

    fn one_time_spawn_database() -> Database {
        Database {
            objects: vec![
                ProtoObject {
                    name: "temple".to_owned(),
                    object_class: Some("Building".to_owned()),
                    commands: vec![ObjectCommand {
                        target: "hero_squad".to_owned(),
                        command_type: Some("TrainSquad".to_owned()),
                        ..ObjectCommand::default()
                    }],
                    child_objects: Some(ChildObjects {
                        objects: vec![ChildObject {
                            proto_object: "hero_squad".to_owned(),
                            child_type: Some(ChildObjectType::OneTimeSpawnSquad),
                            ..ChildObject::default()
                        }],
                    }),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "hero".to_owned(),
                    object_class: Some("Unit".to_owned()),
                    ..ProtoObject::default()
                },
            ],
            squads: vec![ProtoSquad {
                name: "hero_squad".to_owned(),
                build_points: Some(1.0),
                units: Some(UnitsWrapper {
                    entries: vec![UnitEntry {
                        proto_object: "hero".to_owned(),
                        count: 1,
                        ..UnitEntry::default()
                    }],
                }),
                ..ProtoSquad::default()
            }],
            ..Database::default()
        }
    }

    fn associated_unit_database() -> Database {
        Database {
            objects: vec![
                ProtoObject {
                    name: "base".to_owned(),
                    object_class: Some("Building".to_owned()),
                    flags: vec!["KillChildObjectsOnDeath".to_owned()],
                    child_objects: Some(ChildObjects {
                        objects: vec![
                            associated_child("foundation", ChildObjectType::Foundation, 0.0, 0.0),
                            associated_child("turret", ChildObjectType::Unit, 2.0, 4.0),
                            associated_child("beacon", ChildObjectType::Object, -2.0, 1.0),
                            associated_child("parking", ChildObjectType::ParkingLot, 5.0, 0.0),
                        ],
                    }),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "foundation".to_owned(),
                    object_class: Some("Building".to_owned()),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "turret".to_owned(),
                    object_class: Some("Building".to_owned()),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "beacon".to_owned(),
                    object_class: Some("Object".to_owned()),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "parking".to_owned(),
                    object_class: Some("Building".to_owned()),
                    ..ProtoObject::default()
                },
            ],
            ..Database::default()
        }
    }

    fn associated_child(name: &str, child_type: ChildObjectType, x: f32, z: f32) -> ChildObject {
        ChildObject {
            proto_object: name.to_owned(),
            child_type: Some(child_type),
            offset: Some(Vector3 { x, y: 0.0, z }),
            ..ChildObject::default()
        }
    }

    fn socket_child(name: &str, x: f32, z: f32, rotation: f32) -> ChildObject {
        ChildObject {
            proto_object: name.to_owned(),
            child_type: Some(ChildObjectType::Socket),
            offset: Some(Vector3 { x, y: 0.0, z }),
            rotation: Some(rotation),
            ..ChildObject::default()
        }
    }

    fn socket_prototype(name: &str, object_type: &str) -> ProtoObject {
        ProtoObject {
            name: name.to_owned(),
            object_class: Some("Building".to_owned()),
            object_types: vec![object_type.to_owned()],
            ..ProtoObject::default()
        }
    }
}
