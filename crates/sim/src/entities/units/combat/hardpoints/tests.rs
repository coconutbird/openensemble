use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, FRAC_PI_4, PI};

use super::*;
use crate::gameplay::AttackAttachmentPose;

#[test]
fn yaw_attachment_turns_at_authored_rate_in_its_bone_frame() {
    let mut state = UnitHardpointState::default();
    let profile = profile(Some("turret"), None);
    let anchor = anchor("turret", Mat4::from_translation(Vec3::Y * 2.0));

    let aim = state.orient(
        &profile,
        Some(&anchor),
        Mat4::IDENTITY,
        Vec3::new(10.0, 2.0, 10.0),
        0.5,
    );

    let forward = state
        .attachment_transform("TURRET")
        .unwrap()
        .transform_vector3(Vec3::Z);
    assert!(aim.can_face);
    assert!(forward.abs_diff_eq(Vec3::new(FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2), 1.0e-5));
}

#[test]
fn capped_yaw_requests_unit_facing_but_keeps_the_turret_at_its_limit() {
    let mut state = UnitHardpointState::default();
    let mut profile = profile(Some("turret"), None);
    profile.yaw_left = -FRAC_PI_4;
    profile.yaw_right = FRAC_PI_4;
    profile.yaw_rate = PI;

    let aim = state.orient(
        &profile,
        Some(&anchor("turret", Mat4::IDENTITY)),
        Mat4::IDENTITY,
        Vec3::X * 10.0,
        1.0,
    );

    assert!(!aim.can_face);
    let forward = state
        .attachment_transform("turret")
        .unwrap()
        .transform_vector3(Vec3::Z);
    assert!(forward.abs_diff_eq(Vec3::new(FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2), 1.0e-5));
}

#[test]
fn released_hardpoint_waits_five_seconds_before_centering() {
    let mut state = UnitHardpointState::default();
    let mut profile = profile(Some("turret"), None);
    profile.yaw_rate = FRAC_PI_2;
    let anchor = anchor("turret", Mat4::IDENTITY);
    let _aim = state.orient(&profile, Some(&anchor), Mat4::IDENTITY, Vec3::X * 10.0, 1.0);
    let turned = state.attachment_transform("turret").unwrap();
    state.release_active();

    state.advance_auto_center(5.0);
    assert!(
        state
            .attachment_transform("turret")
            .unwrap()
            .abs_diff_eq(turned, 1.0e-6)
    );
    state.advance_auto_center(1.0);
    assert!(
        state
            .attachment_transform("turret")
            .unwrap()
            .abs_diff_eq(Mat4::IDENTITY, 1.0e-5)
    );
}

#[test]
fn tolerance_only_hardpoint_checks_arcs_without_rotating() {
    let mut state = UnitHardpointState::default();
    let mut profile = profile(Some("turret"), None);
    profile.set_uses_angles_as_tolerance(true);
    profile.yaw_left = -FRAC_PI_4;
    profile.yaw_right = FRAC_PI_4;
    let anchor = anchor("turret", Mat4::IDENTITY);

    let outside = state.orient(&profile, Some(&anchor), Mat4::IDENTITY, Vec3::X * 10.0, 1.0);
    let covered = state.orient(
        &profile,
        Some(&anchor),
        Mat4::from_rotation_y(FRAC_PI_4),
        Vec3::X * 10.0,
        1.0,
    );

    assert_eq!(
        outside,
        HardpointAim {
            can_face: false,
            oriented: false,
        }
    );
    assert!(covered.oriented);
    assert!(state.attachment_transform("turret").is_none());
}

#[test]
fn infinite_rate_preserves_world_yaw_when_the_unit_turns() {
    let mut state = UnitHardpointState::default();
    let mut profile = profile(Some("turret"), None);
    profile.set_preserves_yaw_on_unit_turn(true);
    profile.yaw_rate = PI;
    let anchor = anchor("turret", Mat4::IDENTITY);
    let _aim = state.orient(&profile, Some(&anchor), Mat4::IDENTITY, Vec3::X * 10.0, 1.0);
    let yaw_target = state.active_yaw_target_world(Mat4::IDENTITY).unwrap();
    let turned_unit = Mat4::from_rotation_y(FRAC_PI_4);

    state.restore_active_yaw_target(turned_unit, yaw_target);

    let world_forward = turned_unit.transform_vector3(
        state
            .attachment_transform("turret")
            .unwrap()
            .transform_vector3(Vec3::Z),
    );
    assert!(world_forward.abs_diff_eq(Vec3::X, 1.0e-5));
}

#[test]
fn single_bone_hardpoint_uses_the_proto_ik_node() {
    let mut state = UnitHardpointState::default();
    let mut profile = profile(Some("not-a-bone"), None);
    profile.set_uses_single_bone_ik(true);
    profile.single_bone = Some("Bip01 Spine".to_owned());

    let _aim = state.orient(&profile, None, Mat4::IDENTITY, Vec3::X * 10.0, 1.0);

    assert!(state.bone_transform("bip01 spine").is_some());
    assert!(state.bone_transform("not-a-bone").is_none());
}

#[test]
fn attack_tolerance_is_independent_from_the_hardpoint_stop_tolerance() {
    let mut state = UnitHardpointState::default();
    let profile = profile(Some("turret"), None);
    let anchor = anchor("turret", Mat4::IDENTITY);
    state.set_attachment_rotation("turret", Quat::from_rotation_y(15.0_f32.to_radians()));
    let target = Quat::from_rotation_y(20.0_f32.to_radians()) * Vec3::Z * 10.0;

    let aim = state.orient(&profile, Some(&anchor), Mat4::IDENTITY, target, 0.0);

    assert!(!aim.oriented);
    assert!(state.is_oriented(
        &profile,
        Some(&anchor),
        Mat4::IDENTITY,
        target,
        10.0_f32.to_radians().cos(),
    ));
}

#[test]
fn already_aimed_beyond_the_arc_uses_retails_early_success_path() {
    let mut state = UnitHardpointState::default();
    let mut profile = profile(Some("turret"), None);
    profile.yaw_left = -FRAC_PI_4;
    profile.yaw_right = FRAC_PI_4;
    state.set_attachment_rotation("turret", Quat::from_rotation_y(FRAC_PI_2));
    let anchor = anchor("turret", Mat4::IDENTITY);

    let aim = state.orient(&profile, Some(&anchor), Mat4::IDENTITY, Vec3::X * 10.0, 1.0);

    assert!(aim.can_face);
    assert!(
        state
            .attachment_transform("turret")
            .unwrap()
            .transform_vector3(Vec3::Z)
            .abs_diff_eq(Vec3::X, 1.0e-5)
    );
}

#[test]
fn pitch_only_hit_alignment_uses_retails_flattened_forward_test() {
    let mut state = UnitHardpointState::default();
    let profile = profile(None, Some("gun"));
    let anchor = anchor("gun", Mat4::IDENTITY);
    state.set_attachment_rotation("gun", Quat::from_rotation_x(-FRAC_PI_4));

    assert!(state.is_oriented(
        &profile,
        Some(&anchor),
        Mat4::IDENTITY,
        Vec3::new(0.0, 10.0, 10.0),
        DIRECTION_TOLERANCE,
    ));
}

fn profile(yaw: Option<&str>, pitch: Option<&str>) -> AttackHardpointProfile {
    AttackHardpointProfile::for_test(yaw, pitch)
}

fn anchor(component: &str, to_bone: Mat4) -> AttackAnimationAnchor {
    AttackAnimationAnchor {
        links: vec![AttackAttachmentPose {
            child_component: component.to_owned(),
            to_bone: Some(to_bone),
            from_bone: Some(Mat4::IDENTITY),
            disregard_orientation: false,
        }],
        bone_to_component: Some(Mat4::IDENTITY),
        single_bone_poses: Vec::new(),
    }
}
