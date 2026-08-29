use super::*;

#[test]
fn loads_position_then_points_in_canonical_world_axes() {
    let document = Document::from_xml(
        r#"<Scenario><DesignObjects><Lines>
            <Lines ID="7" Position="1,2,3">
                <Data />
                <Points>4,5,6|7,8,9|</Points>
            </Lines>
            <Lines ID="8"><Points /></Lines>
        </Lines></DesignObjects></Scenario>"#,
    )
    .unwrap();
    let mut world = World::new();
    let checksum_before = world.checksum();

    load_design_lines(&mut world, &document).unwrap();

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
    let document = Document::from_xml(
        r#"<Scenario><DesignObjects><Lines>
            <Lines ID="4" Position="1,2,3" />
            <Lines ID="4" Position="10,20,30" />
        </Lines></DesignObjects></Scenario>"#,
    )
    .unwrap();
    let mut world = World::new();

    load_design_lines(&mut world, &document).unwrap();

    assert_eq!(world.design_line_count(), 1);
    assert_eq!(
        world.design_line_points(4),
        Some([Vec3::new(30.0, 20.0, 10.0)].as_slice())
    );
}

#[test]
fn malformed_ids_and_points_are_reported() {
    for (xml, expected) in [
        (
            r"<Scenario><DesignObjects><Lines><Lines /></Lines></DesignObjects></Scenario>",
            DesignLineLoadError::MissingId,
        ),
        (
            r#"<Scenario><DesignObjects><Lines><Lines ID="bad" /></Lines></DesignObjects></Scenario>"#,
            DesignLineLoadError::InvalidId {
                value: "bad".to_owned(),
            },
        ),
        (
            r#"<Scenario><DesignObjects><Lines><Lines ID="9" Position="1,nope,3" /></Lines></DesignObjects></Scenario>"#,
            DesignLineLoadError::InvalidVector {
                line_id: 9,
                field: "Position",
                value: "1,nope,3".to_owned(),
            },
        ),
    ] {
        let document = Document::from_xml(xml).unwrap();
        let mut world = World::new();
        assert_eq!(load_design_lines(&mut world, &document), Err(expected));
    }
}
