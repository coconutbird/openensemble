use super::*;

#[test]
fn loads_points_in_canonical_world_axes() {
    let lines = [
        DesignLine {
            id: 7,
            points: vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
        },
        DesignLine {
            id: 8,
            points: Vec::new(),
        },
    ];
    let mut world = World::new();
    let checksum_before = world.checksum();

    load_design_lines(&mut world, &lines);

    assert_eq!(world.design_line_count(), 2);
    assert_eq!(
        world.design_line_points(7),
        Some(
            [
                Vec3::new(3.0, 2.0, 1.0),
                Vec3::new(6.0, 5.0, 4.0),
                Vec3::new(9.0, 8.0, 7.0),
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
    let lines = [
        DesignLine {
            id: 4,
            points: vec![[1.0, 2.0, 3.0]],
        },
        DesignLine {
            id: 4,
            points: vec![[10.0, 20.0, 30.0]],
        },
    ];
    let mut world = World::new();

    load_design_lines(&mut world, &lines);

    assert_eq!(world.design_line_count(), 1);
    assert_eq!(
        world.design_line_points(4),
        Some([Vec3::new(30.0, 20.0, 10.0)].as_slice())
    );
}
