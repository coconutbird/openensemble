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
