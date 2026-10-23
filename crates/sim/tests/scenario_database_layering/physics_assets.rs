use pipeline::database::hw1::physics::PhysicsVehicleType;

use super::assert_close;

pub(super) const BASE_VEHICLE_PHYSICS: &str = r"<physics>
    <blueprint>layered_vehicle</blueprint>
    <Vehicle>warthog</Vehicle>
    <CenterOffset>9,9,9</CenterOffset>
</physics>";

pub(super) const SCENARIO_VEHICLE_PHYSICS: &str = r"<physics>
    <blueprint>layered_vehicle</blueprint>
    <Vehicle>ghost</Vehicle>
    <CenterOffset>1,2,3</CenterOffset>
</physics>";

pub(super) const BASE_VEHICLE_BLUEPRINT: &str = r"<blueprint>
    <mass>10</mass>
    <friction>0.1</friction>
    <restitution>0.05</restitution>
    <linearDamping>0.2</linearDamping>
    <angularDamping>0.3</angularDamping>
    <shape>layered_vehicle</shape>
</blueprint>";

pub(super) const SCENARIO_VEHICLE_BLUEPRINT: &str = r"<blueprint>
    <mass>222</mass>
    <friction>1.25</friction>
    <restitution>0.4</restitution>
    <linearDamping>0.05</linearDamping>
    <angularDamping>0.15</angularDamping>
    <shape>layered_vehicle</shape>
</blueprint>";

pub(super) const BASE_VEHICLE_SHAPE: &str = r#"<hke version="V_20200_B_20031014">
    <hkobject name="body" type="hkBoxShape">
        <hkparam name="halfExtents" type="hkTypeVector4">(9 9 9)</hkparam>
    </hkobject>
</hke>"#;

pub(super) const SCENARIO_VEHICLE_SHAPE: &str = r#"<hke version="V_20200_B_20031014">
    <hkobject name="body" type="hkBoxShape">
        <hkparam name="halfExtents" type="hkTypeVector4">(4 5 6)</hkparam>
    </hkobject>
</hke>"#;

pub(super) const SCENARIO_CLAMSHELL_PHYSICS: &str = r#"<physics>
    <clamshell>
        <upper heightOffset="2">layered_upper</upper>
        <lower heightOffset="-2">layered_lower</lower>
        <pelvis>layered_pelvis</pelvis>
    </clamshell>
    <CenterOffset>1,10,3</CenterOffset>
</physics>"#;

pub(super) const SCENARIO_UPPER_BLUEPRINT: &str = r"<blueprint>
    <mass>222</mass>
    <shape>layered_upper</shape>
</blueprint>";

pub(super) const SCENARIO_LOWER_BLUEPRINT: &str = r"<blueprint>
    <mass>333</mass>
    <shape>layered_lower</shape>
</blueprint>";

pub(super) const SCENARIO_PELVIS_BLUEPRINT: &str = r"<blueprint>
    <mass>444</mass>
    <shape>layered_pelvis</shape>
</blueprint>";

pub(super) const SCENARIO_UPPER_SHAPE: &str = r#"<hke version="V_20200_B_20031014">
    <hkobject name="body" type="hkBoxShape">
        <hkparam name="halfExtents" type="hkTypeVector4">(1 2 1)</hkparam>
    </hkobject>
</hke>"#;

pub(super) const SCENARIO_LOWER_SHAPE: &str = r#"<hke version="V_20200_B_20031014">
    <hkobject name="body" type="hkBoxShape">
        <hkparam name="halfExtents" type="hkTypeVector4">(2 1 2)</hkparam>
    </hkobject>
</hke>"#;

pub(super) const SCENARIO_PELVIS_SHAPE: &str = r#"<hke version="V_20200_B_20031014">
    <hkobject name="body" type="hkSphereShape">
        <hkparam name="radius" type="hkTypeReal">0.5</hkparam>
    </hkobject>
</hke>"#;

/// The scenario vehicle info, the clamshell replacement and the Ghost body made from them.
pub(super) fn assert_layered_vehicle_physics(loaded: &mut sim::LoadedGameScenario) {
    let issues = loaded.simulation.gameplay.physics_issues();
    assert_eq!(issues.len(), 0, "{issues:?}");
    let profile = std::sync::Arc::clone(
        loaded
            .simulation
            .gameplay
            .object_physics("SCENARIO_VEHICLE")
            .and_then(|templates| templates.info.as_ref())
            .expect("scenario vehicle physics info"),
    );
    assert_eq!(profile.name(), "layered_vehicle");
    assert_eq!(profile.vehicle_type(), PhysicsVehicleType::Ghost);
    assert_eq!(profile.half_extents(), glam::Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(profile.center_offset(), glam::Vec3::new(1.0, 2.0, 3.0));
    let main = profile.main_body();
    assert_close(main.mass(), 222.0);
    assert_close(main.friction(), 1.25);
    assert_close(main.restitution(), 0.4);
    assert_close(main.linear_damping(), 0.05);
    assert_close(main.angular_damping(), 0.15);

    let replacement = loaded
        .simulation
        .gameplay
        .physics_replacement("SCENARIO_MINELAYER")
        .expect("scenario object should load replacement physics without a detonation flag");
    assert_eq!(replacement.name(), "layered_clamshell");
    assert!(replacement.is_clamshell());
    assert_eq!(replacement.center_offset(), glam::Vec3::new(1.0, 10.0, 3.0));
    let [upper, lower, pelvis] = replacement
        .clamshell_bodies()
        .expect("upper, lower and pelvis bodies");
    assert_close(upper.mass(), 222.0);
    assert_close(lower.mass(), 333.0);
    assert_close(pelvis.mass(), 444.0);
    assert_close(replacement.info().upper_height_offset, 2.0);
    assert_close(replacement.info().lower_height_offset, -2.0);

    let world = &mut loaded.simulation.world;
    let vehicle_id = sim::spawn_object_at(
        world,
        &loaded.content.database,
        1,
        sim::object_prototype_id(&loaded.content.database, "scenario_vehicle").unwrap(),
        glam::Vec3::new(10.0, 7.0, 20.0),
        glam::Vec3::Z,
    )
    .expect("scenario-layered vehicle prototype");
    world.create_pending_physics_objects();
    let vehicle = world.get_unit(vehicle_id).unwrap();
    assert_close(vehicle.speed, 20.0);
    assert_close(vehicle.acceleration, 50.0);
    assert_close(vehicle.turn_rate_degrees, 90.0);
    let state = vehicle.physics_object.as_ref().expect("the Ghost body");
    assert_eq!(state.info(), "layered_vehicle");
    assert!(state.has_vehicle_action());
    let body = state.body();
    let physics = world.physics();
    assert_eq!(
        physics.world().motion_type(body),
        Some(physics::MotionType::BoxInertia)
    );
    assert_eq!(physics.world().mass(body), Some(222.0));
    // `BEntity_setupVehiclePhysics`: the shared Vehicle filter and the ghost inertia
    // `diag(m, m, 0.4·m)`.
    assert_eq!(
        physics.world().filter_info(body),
        Some(physics.vehicle_filter())
    );
    assert_eq!(
        physics.world().inertia_local(body),
        Some(glam::Vec3::new(222.0, 222.0, 0.4 * 222.0))
    );
}
