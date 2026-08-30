use std::collections::HashMap;

use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};
use pipeline::source::{AssetSource, StdFileProvider};

use super::{simulation_entity_transform, simulation_proto_names};

#[test]
fn attached_class_zero_visual_is_projected_only_from_live_sim_state() {
    let mut source = AssetSource::with_provider(StdFileProvider);
    let mut world = sim::World::new();
    world.init_players(1);
    let unit_id = world.create_unit_at(1, Vec3::new(2.0, 3.0, 4.0));
    world.get_unit_mut(unit_id).unwrap().proto_object_name = "vehicle".to_owned();
    let database = Database {
        objects: vec![ProtoObject {
            name: "fx_hijacked".to_owned(),
            dbid: Some(3883),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let visuals = HashMap::new();
    let mut scene = super::UnitScene::load_world(&mut source, &world, &visuals, &database.objects);
    let attachment_id = world
        .add_prototype_attachment_to_unit(&database, unit_id, 3883)
        .expect("retail's omitted ObjectClass default creates a class-zero visual");

    assert!(!scene.roster_matches(&world));
    assert!(scene.sync_world(&mut source, &world, &visuals, &database.objects));
    assert!(scene.roster_matches(&world));
    assert_eq!(scene.simulation_entity_count(), 2);
    assert!(simulation_proto_names(&world).any(|name| name == "fx_hijacked"));
    let transform = simulation_entity_transform(&world, attachment_id).unwrap();
    assert!(
        transform
            .transform_point3(Vec3::ZERO)
            .abs_diff_eq(Vec3::new(2.0, 3.0, 4.0), 1.0e-6)
    );

    assert!(world.teleport_object(unit_id, Vec3::new(9.0, 8.0, 7.0)));
    world.update_entities(0.05);
    let transform = simulation_entity_transform(&world, attachment_id).unwrap();
    assert!(
        transform
            .transform_point3(Vec3::ZERO)
            .abs_diff_eq(Vec3::new(9.0, 8.0, 7.0), 1.0e-6)
    );
}
