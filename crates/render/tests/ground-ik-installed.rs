//! Installed-data audit for object IK profiles referenced by visual tags.

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn reports_installed_object_ik_profiles() {
    let loaded = test_support::installed_scenario("blood_gulch");

    let mut count = 0;
    for proto in &loaded.content.database.objects {
        if proto.ground_ik.is_empty()
            && proto.ground_ik_tilt.is_none()
            && proto.sweet_spot_ik.is_empty()
        {
            continue;
        }
        count += 1;
        println!(
            "{} ground={:?} tilt={:?} sweet={:?} flags={:?}",
            proto.name, proto.ground_ik, proto.ground_ik_tilt, proto.sweet_spot_ik, proto.flags,
        );
    }
    assert!(
        count > 0,
        "installed database contains no object IK profiles"
    );
}
