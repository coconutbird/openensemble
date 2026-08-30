use super::*;

#[test]
fn quadratic_jump_passes_through_retail_midpoint() {
    let mut jump = UnitJump::default();
    assert!(jump.begin(
        JumpOrderType::Jump,
        "Jump",
        Vec3::ZERO,
        Vec3::X * 40.0,
        40.0,
    ));

    let pending = jump.advance(0.5);
    assert_eq!(jump.phase, UnitJumpPhase::Flying);
    assert_eq!(pending.position, None);
    assert!(!pending.complete);

    let midpoint = jump.advance(0.5);
    assert_vec3_close(midpoint.position.unwrap(), Vec3::new(20.0, 20.0, 0.0));
    assert!(!midpoint.complete);
    let landing = jump.advance(0.5);
    assert_vec3_close(landing.position.unwrap(), Vec3::X * 40.0);
    assert!(landing.complete);
}

#[test]
fn active_jump_is_the_renderer_move_projection_and_blocks_targeting() {
    let mut unit = Unit::default();
    assert!(unit.is_attackable());
    assert!(!unit.has_active_move_action());

    assert!(unit.begin_jump_action(JumpOrderType::Jump, "Jump", Vec3::X * 20.0, 20.0,));
    assert!(unit.is_jumping());
    assert!(unit.has_active_move_action());
    assert!(!unit.is_attackable());

    unit.cancel_jump_action();
    assert!(!unit.is_jumping());
    assert!(!unit.has_active_move_action());
    assert!(unit.is_attackable());
}

#[test]
fn jump_progress_changes_the_authoritative_checksum() {
    let mut jump = UnitJump::default();
    assert!(jump.begin(
        JumpOrderType::Attack,
        "JumpAttack",
        Vec3::ZERO,
        Vec3::X * 40.0,
        40.0,
    ));
    let before = hash(&jump);
    let _pending = jump.advance(0.25);
    let flying = hash(&jump);
    let _advanced = jump.advance(0.25);

    assert_ne!(before, flying);
    assert_ne!(flying, hash(&jump));
}

fn hash(jump: &UnitJump) -> u32 {
    let mut checksum = SyncChecksum::new();
    jump.hash_state(&mut checksum);
    checksum.value()
}

fn assert_vec3_close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, 0.000_1),
        "expected {expected:?}, got {actual:?}",
    );
}
