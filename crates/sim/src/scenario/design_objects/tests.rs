use super::*;
use pipeline::hw1::scenario::{DesignData, DesignLine, DesignSphere as ScenarioDesignSphere};

fn sphere(id: i32, type_name: &str, position: [f32; 3], radius: f32) -> ScenarioDesignSphere {
    ScenarioDesignSphere {
        id,
        position,
        radius,
        data: DesignData {
            type_name: type_name.to_owned(),
            ..DesignData::default()
        },
    }
}

#[test]
fn loads_points_in_the_authored_frame() {
    let objects = DesignObjects {
        spheres: Vec::new(),
        lines: vec![
            DesignLine {
                id: 7,
                points: vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
            },
            DesignLine {
                id: 8,
                points: Vec::new(),
            },
        ],
    };
    let mut world = World::new();
    let checksum_before = world.checksum();

    load_design_objects(&mut world, &objects);

    assert_eq!(world.design_line_count(), 2);
    assert_eq!(
        world.design_line_points(7),
        Some(
            [
                Vec3::new(1.0, 2.0, 3.0),
                Vec3::new(4.0, 5.0, 6.0),
                Vec3::new(7.0, 8.0, 9.0),
            ]
            .as_slice()
        )
    );
    assert_eq!(world.design_line_points(8), Some([].as_slice()));
    assert_ne!(world.checksum(), checksum_before);

    world.reset();
    assert_eq!(world.design_line_count(), 0);
}

#[test]
fn duplicate_ids_use_the_last_authored_line_like_retail_lookup() {
    let objects = DesignObjects {
        spheres: Vec::new(),
        lines: vec![
            DesignLine {
                id: 4,
                points: vec![[1.0, 2.0, 3.0]],
            },
            DesignLine {
                id: 4,
                points: vec![[10.0, 20.0, 30.0]],
            },
        ],
    };
    let mut world = World::new();

    load_design_objects(&mut world, &objects);

    assert_eq!(world.design_line_count(), 1);
    assert_eq!(
        world.design_line_points(4),
        Some([Vec3::new(10.0, 20.0, 30.0)].as_slice())
    );
}

/// `BDesignObjectManager_load` appends spheres in document order, also for duplicate IDs.
/// `BDesignSphere_load` reads `Position` as authored (`BXMLAttribute_getValueAsVector`).
#[test]
fn spheres_keep_document_order_and_authored_positions_and_change_the_checksum() {
    let objects = DesignObjects {
        spheres: vec![
            sphere(24, "Creeps", [447.0, 0.0, 268.0], 55.0),
            sphere(24, "creeps", [1.0, 2.0, 3.0], 20.0),
        ],
        lines: Vec::new(),
    };
    let mut world = World::new();
    let checksum_before = world.checksum();

    load_design_objects(&mut world, &objects);

    assert_eq!(
        world.design_spheres(),
        [
            DesignSphere {
                type_name: "Creeps".to_owned(),
                center: Vec3::new(447.0, 0.0, 268.0),
                radius: 55.0,
            },
            DesignSphere {
                type_name: "creeps".to_owned(),
                center: Vec3::new(1.0, 2.0, 3.0),
                radius: 20.0,
            },
        ]
    );
    let checksum_with_spheres = world.checksum();
    assert_ne!(checksum_with_spheres, checksum_before);

    let mut moved = objects.clone();
    moved.spheres[1].radius = 21.0;
    load_design_objects(&mut world, &moved);
    assert_ne!(world.checksum(), checksum_with_spheres);

    world.reset();
    assert_eq!(world.design_spheres(), []);
}
