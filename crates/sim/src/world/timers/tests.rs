use super::*;

#[test]
fn count_up_and_down_clamp_and_finish_on_world_time() {
    let mut world = World::new();
    let up = world.create_game_timer(true, 50, 100, None, GameTimerAudience::PrimaryUser);
    let down = world.create_game_timer(false, 200, 0, None, GameTimerAudience::PrimaryUser);

    world.advance_time(60);
    assert_eq!(world.game_timer(up).unwrap().current_time_ms(), 100);
    assert!(world.game_timer(up).unwrap().is_done());
    assert_eq!(world.game_timer(down).unwrap().current_time_ms(), 140);
    assert!(!world.game_timer(down).unwrap().is_done());

    world.advance_time(200);
    assert_eq!(world.game_timer(down).unwrap().current_time_ms(), 0);
    assert!(world.game_timer(down).unwrap().is_done());
}

#[test]
fn pause_discards_elapsed_time_and_four_slots_reuse_the_lowest_slot() {
    let mut world = World::new();
    let first = world.create_game_timer(true, 0, 1_000, None, GameTimerAudience::PrimaryUser);
    assert!(world.set_game_timer_paused(first, true));
    world.advance_time(500);
    assert_eq!(world.game_timer(first).unwrap().current_time_ms(), 0);
    assert!(world.set_game_timer_paused(first, false));
    world.advance_time(50);
    assert_eq!(world.game_timer(first).unwrap().current_time_ms(), 50);

    for expected in 1..4 {
        assert_eq!(
            world.create_game_timer(true, 0, 1, None, GameTimerAudience::PrimaryUser),
            expected
        );
    }
    assert_eq!(
        world.create_game_timer(true, 0, 1, None, GameTimerAudience::PrimaryUser),
        -1
    );
    assert!(world.destroy_game_timer(1));
    assert_eq!(
        world.create_game_timer(true, 0, 1, None, GameTimerAudience::PrimaryUser),
        4
    );
    assert_eq!(world.game_timer(4).unwrap().slot(), 1);
}

#[test]
fn audiences_and_timer_state_are_authoritative_and_checksummed() {
    let mut world = World::new();
    let initial = world.checksum();
    let id = world.create_game_timer(
        false,
        60_000,
        0,
        Some(42),
        GameTimerAudience::Players(vec![1, 3]),
    );
    let timer = world.game_timer(id).unwrap();
    assert!(timer.audience().includes(1, false));
    assert!(!timer.audience().includes(2, true));
    assert_eq!(timer.label_string_id(), Some(42));
    assert_ne!(world.checksum(), initial);
}

#[test]
fn reset_clears_timers_and_restarts_timer_ids() {
    let mut world = World::new();
    let timer_id = world.create_game_timer(true, 0, 1_000, None, GameTimerAudience::PrimaryUser);
    assert_eq!(timer_id, 0);
    world.advance_time(250);

    world.reset();

    assert!(world.game_timers().next().is_none());
    assert_eq!(world.game_time(), 0);
    assert_eq!(
        world.create_game_timer(true, 0, 1_000, None, GameTimerAudience::PrimaryUser),
        0
    );
}
