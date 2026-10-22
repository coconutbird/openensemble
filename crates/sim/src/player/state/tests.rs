use super::*;

/// `BPlayer_setPlayerState`: `Disconnected` while playing becomes `Resigned`, sets the
/// disconnected flag, stores `(time + 500) / 1000` and posts world event 53.
#[test]
fn disconnect_while_playing_resigns_and_stores_the_rounded_time() {
    let mut player = Player::new(1);
    let change = player.set_player_state(PlayerState::Disconnected, 1_500);
    assert_eq!(
        change,
        Some(PlayerStateChange {
            player_id: 1,
            previous: PlayerState::Playing,
            state: PlayerState::Resigned,
            event: Some(PlayerStateEvent::Resigned),
        })
    );
    assert!(player.disconnected());
    assert_eq!(player.end_time_seconds(), Some(2));
    assert!(player.units_cleanup_pending());
}

/// Only the first change away from `Playing` stores the time. A later `Disconnected` keeps the
/// state 3, posts no event and does not mark the units.
#[test]
fn later_changes_keep_the_first_end_time() {
    let mut player = Player::new(2);
    let won = player.set_player_state(PlayerState::Won, 1_499);
    assert_eq!(
        won.map(|change| change.event),
        Some(Some(PlayerStateEvent::Won))
    );
    assert_eq!(player.end_time_seconds(), Some(1));
    assert!(!player.units_cleanup_pending());

    let disconnected = player.set_player_state(PlayerState::Disconnected, 9_000);
    assert_eq!(
        disconnected,
        Some(PlayerStateChange {
            player_id: 2,
            previous: PlayerState::Won,
            state: PlayerState::Disconnected,
            event: None,
        })
    );
    assert_eq!(player.end_time_seconds(), Some(1));
    assert!(!player.units_cleanup_pending());
}

/// A request for the current state changes nothing, but `Disconnected` still sets the flag.
#[test]
fn same_state_is_not_a_change() {
    let mut player = Player::new(3);
    assert!(player.set_player_state(PlayerState::Defeated, 0).is_some());
    assert!(player.units_cleanup_pending());
    assert_eq!(player.set_player_state(PlayerState::Defeated, 10), None);

    let mut disconnected = Player::new(4);
    disconnected.state = PlayerState::Disconnected;
    assert_eq!(
        disconnected.set_player_state(PlayerState::Disconnected, 0),
        None
    );
    assert!(disconnected.disconnected());
}
