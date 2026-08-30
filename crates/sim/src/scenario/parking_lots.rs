//! Retail auto-parking-lot creation for socket-built structures.

use super::{create_unit_squad_from_prototype, find_proto_object};
use crate::entity_id::EntityId;
use crate::world::World;
use crate::world::sockets::authored_socket_transform;
use pipeline::database::hw1::{Database, ProtoObject};

pub(crate) fn materialize_auto_parking_lot(
    world: &mut World,
    builder_id: EntityId,
    building_id: EntityId,
    database: &Database,
) -> Option<EntityId> {
    if world
        .get_building(building_id)?
        .associated_parking_lot()
        .is_some()
    {
        return None;
    }
    let builder = world.get_building(builder_id)?;
    let builder_proto = find_proto_object(database, &builder.proto_object_name)?.1;
    if !has_flag(builder_proto, "UseAutoParkingLot") {
        return None;
    }
    let building = world.get_building(building_id)?;
    let building_proto = find_proto_object(database, &building.proto_object_name)?.1;
    let parking = building_proto.auto_parking_lot.as_ref()?;
    let parking_name = parking.proto_object.trim();
    if parking_name.is_empty() {
        return None;
    }
    let player_id = building.base.player_id;
    let (mut position, forward) = authored_socket_transform(
        building.base.position,
        building.base.forward,
        parking.offset,
        parking.rotation,
    );
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    let (squad_id, parking_lot_id) = create_unit_squad_from_prototype(
        world,
        player_id,
        position,
        forward,
        parking_name,
        database,
    )?;
    if let Some(parking_lot) = world.get_building_mut(parking_lot_id) {
        parking_lot.built_by = Some(builder_id);
    }
    if world.associate_parking_lot(building_id, parking_lot_id) {
        return Some(parking_lot_id);
    }
    let _removed = world.remove_unit(parking_lot_id);
    if world.get_squad(squad_id).is_some() {
        let _removed = world.remove_squad(squad_id);
    }
    None
}

fn has_flag(prototype: &ProtoObject, expected: &str) -> bool {
    prototype
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use pipeline::database::hw1::Vector3;
    use pipeline::database::hw1::objects::AutoParkingLot;

    #[test]
    fn auto_parking_lot_uses_target_local_transform_and_association() {
        let database = Database {
            objects: vec![
                ProtoObject {
                    name: "builder".to_owned(),
                    object_class: Some("Building".to_owned()),
                    flags: vec!["UseAutoParkingLot".to_owned()],
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "barracks".to_owned(),
                    object_class: Some("Building".to_owned()),
                    auto_parking_lot: Some(AutoParkingLot {
                        proto_object: "parking".to_owned(),
                        rotation: Some(90.0),
                        offset: Some(Vector3 {
                            x: 2.0,
                            y: 0.0,
                            z: 3.0,
                        }),
                    }),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "parking".to_owned(),
                    object_class: Some("Building".to_owned()),
                    ..ProtoObject::default()
                },
            ],
            ..Database::default()
        };
        let mut world = World::new();
        let builder = world.create_building(1);
        world.get_building_mut(builder).unwrap().proto_object_name = "builder".to_owned();
        let building = world.create_building_at(1, Vec3::new(10.0, 0.0, 20.0));
        let target = world.get_building_mut(building).unwrap();
        target.proto_object_name = "barracks".to_owned();
        target.base.set_forward(Vec3::X);

        let parking = materialize_auto_parking_lot(&mut world, builder, building, &database)
            .expect("auto parking lot");

        let parking_unit = world.get_building(parking).unwrap();
        assert!(
            parking_unit
                .base
                .position
                .abs_diff_eq(Vec3::new(13.0, 0.0, 18.0), 1.0e-6)
        );
        assert!(parking_unit.base.forward.abs_diff_eq(-Vec3::Z, 1.0e-6));
        assert_eq!(
            world
                .get_building(building)
                .unwrap()
                .associated_parking_lot(),
            Some(parking)
        );
    }
}
