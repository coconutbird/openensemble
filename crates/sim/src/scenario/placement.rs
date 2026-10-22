//! Start position assignment like `BScenario_assignPlayerPositions` @ `0x140383A90`.

use pipeline::hw1::scenario::{PlayerPlacement, ScenarioPosition};

use super::ScenarioData;
use crate::random::SimRandom;

/// Largest number of positions or players that the placement rules accept.
const MAX_PLACEMENT_ENTRIES: usize = 24;

/// One active scenario player as the placement sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlacementPlayer {
    /// Team after the team-0 balance.
    pub(super) team: i32,
    /// Position index that a `<Position Player="N">` element pins to this player.
    pub(super) pinned: Option<usize>,
}

/// The positions that `BScenario_loadPosition` @ `0x14037F430` keeps: it drops an element
/// without a valid `Position` vector.
pub(super) fn engine_positions(scenario: &ScenarioData) -> Vec<&ScenarioPosition> {
    scenario
        .positions()
        .iter()
        .filter(|position| position.position_vec3().is_some())
        .collect()
}

/// Assign a start position index to each active player, in the order of `players`.
///
/// 1. Each player takes its pinned position, else the next free position (`Player` of
///    `-1`) in document order.
/// 2. Unless the rule is fixed or the config `NoRandomPlayerPlacement` is defined, the rule
///    reassigns positions with the placement random stream (`qword_141625AC0 + 0x24`).
///    It needs at least as many positions as players, at most 24 of each, and one or two
///    teams among the unpinned players.
pub(super) fn assign_player_positions(
    players: &[PlacementPlayer],
    positions: &[&ScenarioPosition],
    placement: PlayerPlacement,
    no_random_placement: bool,
    rng: &mut SimRandom,
) -> Vec<Option<usize>> {
    let mut starts = vec![None; players.len()];
    let mut cursor = 0;
    for (start, player) in starts.iter_mut().zip(players) {
        if let Some(pinned) = player.pinned.filter(|index| *index < positions.len()) {
            *start = Some(pinned);
            continue;
        }
        while cursor < positions.len() && positions[cursor].player != -1 {
            cursor += 1;
        }
        if cursor < positions.len() {
            *start = Some(cursor);
            cursor += 1;
        }
    }
    if placement == PlayerPlacement::Fixed
        || no_random_placement
        || positions.len() < players.len()
        || positions.len() > MAX_PLACEMENT_ENTRIES
        || players.len() > MAX_PLACEMENT_ENTRIES
    {
        return starts;
    }
    let (order, team_count) = shuffle_placement_teams(players, rng);
    if !(1..=2).contains(&team_count) || order.is_empty() || order.len() > MAX_PLACEMENT_ENTRIES {
        return starts;
    }
    match placement {
        PlayerPlacement::Fixed => {}
        PlayerPlacement::Random => assign_random(&mut starts, positions.len(), rng),
        PlayerPlacement::Consecutive { spacing } => {
            assign_consecutive(&mut starts, players, positions, &order, spacing, rng);
        }
        PlayerPlacement::Grouped => assign_grouped(&mut starts, players, positions, &order),
    }
    starts
}

/// Order the unpinned players like `BScenario_shufflePlacementTeams` @ `0x140384430`.
///
/// The teams of the unpinned players (first appearance order) are shuffled. Then the players
/// of each team (document order) are appended and shuffled within the team. Returns the
/// player order and the team count.
fn shuffle_placement_teams(
    players: &[PlacementPlayer],
    rng: &mut SimRandom,
) -> (Vec<usize>, usize) {
    let mut teams = Vec::new();
    for player in players.iter().filter(|player| player.pinned.is_none()) {
        if !teams.contains(&player.team) {
            teams.push(player.team);
        }
    }
    let team_count = teams.len();
    let last_team = i32::try_from(team_count).unwrap_or(i32::MAX) - 1;
    for index in 0..team_count {
        let other = usize::try_from(rng.range(0, last_team)).unwrap_or(0);
        teams.swap(index, other);
    }
    let mut order = Vec::with_capacity(players.len());
    for team in teams {
        let first = order.len();
        order.extend(
            players
                .iter()
                .enumerate()
                .filter(|(_, player)| player.pinned.is_none() && player.team == team)
                .map(|(index, _)| index),
        );
        let last_offset = i32::try_from(order.len() - first).unwrap_or(i32::MAX) - 1;
        for index in first..order.len() {
            let other = first + usize::try_from(rng.range(0, last_offset)).unwrap_or(0);
            order.swap(index, other);
        }
    }
    (order, team_count)
}

/// `Random`: each active player in document order takes a random remaining position.
fn assign_random(starts: &mut [Option<usize>], position_count: usize, rng: &mut SimRandom) {
    let mut available = (0..position_count).collect::<Vec<_>>();
    for start in starts.iter_mut() {
        let last = i32::try_from(available.len()).unwrap_or(i32::MAX) - 1;
        let pick = usize::try_from(rng.range(0, last)).unwrap_or(0);
        *start = Some(available.remove(pick));
    }
}

/// The free positions grouped by `Number` (`1..=24`), each group in document order.
fn number_groups(positions: &[&ScenarioPosition]) -> Vec<Vec<usize>> {
    let mut groups = vec![Vec::new(); MAX_PLACEMENT_ENTRIES];
    for (index, position) in positions.iter().enumerate() {
        if position.player != -1 {
            continue;
        }
        if let Some(group) = usize::try_from(position.number)
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|group| groups.get_mut(group))
        {
            group.push(index);
        }
    }
    groups
}

/// `Grouped`: the team of the first ordered player takes the positions of number 1, the
/// other team the positions of number 2. The assignment stops when a group runs out.
fn assign_grouped(
    starts: &mut [Option<usize>],
    players: &[PlacementPlayer],
    positions: &[&ScenarioPosition],
    order: &[usize],
) {
    let groups = number_groups(positions);
    let first_team = players[order[0]].team;
    let mut next = [0, 0];
    for &player in order {
        let group = usize::from(players[player].team != first_team);
        let Some(&position) = groups[group].get(next[group]) else {
            return;
        };
        next[group] += 1;
        starts[player] = Some(position);
    }
}

/// `Consecutive`: the first position of each number forms a ring. The first team starts at
/// a random ring entry. The other team follows the first team after a random gap: with
/// `room` the ring length minus `spacing` minus the player count (at least 0), the gap is in
/// `min(spacing, room)..=room`.
fn assign_consecutive(
    starts: &mut [Option<usize>],
    players: &[PlacementPlayer],
    positions: &[&ScenarioPosition],
    order: &[usize],
    spacing: i32,
    rng: &mut SimRandom,
) {
    let cycle = number_groups(positions)
        .into_iter()
        .filter_map(|group| group.first().copied())
        .collect::<Vec<_>>();
    if cycle.len() < order.len() {
        return;
    }
    let ring_length = i32::try_from(cycle.len()).unwrap_or(i32::MAX);
    let first_team = players[order[0]].team;
    let first_team_size = players
        .iter()
        .filter(|player| player.team == first_team)
        .count();
    let first_start = rng.range(0, ring_length - 1);
    let order_length = i32::try_from(order.len()).unwrap_or(i32::MAX);
    let room = (ring_length - spacing - order_length).max(0);
    let gap = rng.range(spacing.min(room), room);
    let mut other_start = i32::try_from(first_team_size).unwrap_or(i32::MAX) + first_start + gap;
    if other_start >= ring_length {
        other_start -= ring_length;
    }
    let mut cursors = [
        usize::try_from(first_start).unwrap_or(0),
        usize::try_from(other_start).unwrap_or(0),
    ];
    for &player in order {
        let cursor = &mut cursors[usize::from(players[player].team != first_team)];
        starts[player] = cycle.get(*cursor).copied();
        *cursor += 1;
        if *cursor == cycle.len() {
            *cursor = 0;
        }
    }
}

#[cfg(test)]
#[path = "placement/tests.rs"]
mod tests;
