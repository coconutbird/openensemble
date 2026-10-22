use super::*;

const EPSILON: f32 = 1e-4;

fn assert_near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= EPSILON,
        "expected {expected}, got {actual}"
    );
}

fn assert_vec_near(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, EPSILON),
        "expected {expected}, got {actual}"
    );
}

#[test]
fn a_new_camera_uses_the_retail_start_values() {
    let camera = RtsCamera::new(Vec3::new(10.0, 2.0, 20.0));

    assert_near(camera.yaw_degrees, 317.2);
    assert_near(camera.zoom(), 85.0);
    // BUser_resetCameraDefaults: (85 - 60) / (115 - 60) * (54 - 34) + 34.
    assert_near(camera.pitch_degrees, 25.0 / 55.0 * 20.0 + 34.0);
    assert_near(camera.scroll_rate, SCROLL_RATE_BASE);
    assert_vec_near(camera.hover_point(), Vec3::new(10.0, 2.0, 20.0));
}

#[test]
fn the_render_camera_looks_at_the_hover_point_from_the_zoom_distance() {
    let rts = RtsCamera::new(Vec3::new(100.0, 5.0, 200.0));
    let mut camera = Camera::default();
    rts.apply_to(&mut camera);

    assert_near(camera.position.distance(rts.hover_point()), rts.zoom());
    assert_vec_near(
        camera.position + camera.forward() * rts.zoom(),
        rts.hover_point(),
    );
    // The pitch is the angle between the view direction and straight down.
    let angle_from_down = camera.forward().angle_between(Vec3::NEG_Y).to_degrees();
    assert_near(angle_from_down, rts.pitch_degrees);
    // The yaw direction is (sin yaw, 0, cos yaw).
    let yaw = 317.2_f32.to_radians();
    let horizontal = Vec3::new(camera.forward().x, 0.0, camera.forward().z).normalize();
    assert_vec_near(horizontal, Vec3::new(yaw.sin(), 0.0, yaw.cos()));
    assert_near(camera.fov, 44.9_f32.to_radians());
}

#[test]
fn scroll_speed_starts_at_the_base_rate_and_grows_to_the_maximum() {
    let mut rts = RtsCamera::new(Vec3::ZERO);
    let forward = rts.forward();

    rts.scroll(Vec2::Y, false, 0.1);
    // The rate grows before the move: 0.1 + 1.5 * 0.1 = 0.25.
    assert_near(rts.scroll_rate, 0.25);
    assert_vec_near(rts.hover_point(), forward * (0.25 * 100.0 * 0.1));

    for _ in 0..10 {
        rts.scroll(Vec2::Y, false, 0.1);
    }
    assert_near(rts.scroll_rate, 1.0);

    rts.scroll(Vec2::ZERO, false, 0.1);
    assert_near(rts.scroll_rate, SCROLL_RATE_BASE);
}

#[test]
fn fast_scroll_uses_the_fast_speed_and_diagonal_input_has_the_same_speed() {
    let mut rts = RtsCamera::new(Vec3::ZERO);
    rts.scroll_rate = 1.0;
    rts.scroll(Vec2::new(1.0, 1.0), true, 0.5);

    assert_near(rts.hover_point().length(), 400.0 * 0.5);
}

#[test]
fn scroll_to_the_right_follows_the_screen_right_of_the_render_camera() {
    let mut rts = RtsCamera::new(Vec3::ZERO);
    let mut camera = Camera::default();
    rts.apply_to(&mut camera);
    rts.scroll_rate = 1.0;
    rts.scroll(Vec2::X, false, 1.0);

    assert_vec_near(rts.hover_point(), camera.right() * 100.0);
}

#[test]
fn a_positive_rotation_turns_the_view_to_the_right() {
    let mut rts = RtsCamera::new(Vec3::ZERO);
    let right = rts.right();
    rts.rotate_degrees(90.0);

    assert_vec_near(rts.forward(), right);

    rts.rotate(1.0, false, 0.5);
    assert_near(rts.yaw_degrees, (317.2_f32 - 90.0 - 80.0).rem_euclid(360.0));
}

#[test]
fn zoom_input_clamps_the_zoom_and_sets_the_pitch_from_the_zoom() {
    let mut rts = RtsCamera::new(Vec3::ZERO);
    rts.zoom_by(1.0, false, 0.25);
    assert_near(rts.zoom(), 65.0);
    assert_near(rts.pitch_degrees, pitch_for_zoom(65.0));

    rts.zoom_by(1.0, true, 1.0);
    assert_near(rts.zoom(), ZOOM_MIN);
    assert_near(rts.pitch_degrees, PITCH_MAX_DEGREES);

    rts.zoom_by(-1.0, false, 10.0);
    assert_near(rts.zoom(), ZOOM_MAX);
    assert_near(rts.pitch_degrees, PITCH_MIN_DEGREES);
}

#[test]
fn a_directive_camera_gives_the_yaw_and_the_hover_point() {
    let mut rts = RtsCamera::new(Vec3::new(50.0, 3.0, 60.0));
    let mut camera = Camera::default();
    let adapter_zoom = rts.zoom();
    rts.rotate_degrees(30.0);
    rts.apply_to(&mut camera);
    let expected = rts.clone();

    let mut moved = RtsCamera::new(Vec3::ZERO);
    moved.take_directive(&camera, adapter_zoom);

    assert_vec_near(moved.hover_point(), expected.hover_point());
    assert_near(moved.yaw_degrees, expected.yaw_degrees);
}

#[test]
fn the_hover_point_stays_in_the_bounds_and_follows_the_terrain() {
    let mut rts = RtsCamera::new(Vec3::new(-10.0, 0.0, 500.0));
    let bounds = WorldBounds::from_corners(Vec3::ZERO, Vec3::new(100.0, 0.0, 200.0))
        .expect("finite corners");
    rts.clamp_to(bounds);
    assert_vec_near(rts.hover_point(), Vec3::new(0.0, 0.0, 200.0));

    rts.follow_terrain(Some(10.0), 0.1);
    assert_near(rts.hover_point().y, 5.0);
    rts.follow_terrain(None, 0.1);
    assert_near(rts.hover_point().y, 5.0);
    rts.snap_to_terrain(Some(12.0));
    assert_near(rts.hover_point().y, 12.0);
}
