//! Team diplomacy stored by the authoritative world.

use crate::player::{MAX_TEAMS, PlayerId, TeamId, TeamRelation};
use crate::sync::SyncChecksum;

use super::World;

impl World {
    /// Return the directed diplomacy relation between two teams.
    ///
    /// Invalid team IDs are neutral, matching the original engine.
    #[must_use]
    pub fn team_relation(&self, from: TeamId, to: TeamId) -> TeamRelation {
        self.team_relations
            .get(usize::from(from))
            .and_then(|relations| relations.get(usize::from(to)))
            .copied()
            .unwrap_or(TeamRelation::Neutral)
    }

    /// Set one directed team diplomacy relation.
    ///
    /// Returns `false` if either team ID is outside the vanilla team range.
    pub fn set_team_relation(&mut self, from: TeamId, to: TeamId, relation: TeamRelation) -> bool {
        let Some(relations) = self.team_relations.get_mut(usize::from(from)) else {
            return false;
        };
        let Some(current) = relations.get_mut(usize::from(to)) else {
            return false;
        };
        *current = relation;
        true
    }

    /// Set the same relation in both directions.
    pub fn set_mutual_team_relation(
        &mut self,
        first: TeamId,
        second: TeamId,
        relation: TeamRelation,
    ) -> bool {
        if usize::from(first) >= MAX_TEAMS || usize::from(second) >= MAX_TEAMS {
            return false;
        }
        self.team_relations[usize::from(first)][usize::from(second)] = relation;
        self.team_relations[usize::from(second)][usize::from(first)] = relation;
        true
    }

    /// Configure standard skirmish diplomacy from assigned team IDs.
    ///
    /// A team is allied with itself, Gaia/team zero is neutral, and distinct
    /// nonzero teams are enemies. Scenario scripts may override individual
    /// directed entries afterward with [`Self::set_team_relation`].
    pub fn configure_standard_team_relations(&mut self) {
        for from in 0..MAX_TEAMS {
            for to in 0..MAX_TEAMS {
                self.team_relations[from][to] = if from == to {
                    TeamRelation::Ally
                } else if from == 0 || to == 0 {
                    TeamRelation::Neutral
                } else {
                    TeamRelation::Enemy
                };
            }
        }
    }

    /// Return the diplomacy relation between two players' current teams.
    #[must_use]
    pub fn player_relation(&self, from: PlayerId, to: PlayerId) -> Option<TeamRelation> {
        let from_team = self.get_player(from)?.team_id;
        let to_team = self.get_player(to)?.team_id;
        Some(self.team_relation(from_team, to_team))
    }

    /// Return whether two valid players are allied.
    #[must_use]
    pub fn players_are_allied(&self, first: PlayerId, second: PlayerId) -> bool {
        self.player_relation(first, second) == Some(TeamRelation::Ally)
    }

    /// Return whether two valid players are enemies.
    #[must_use]
    pub fn players_are_enemies(&self, first: PlayerId, second: PlayerId) -> bool {
        self.player_relation(first, second) == Some(TeamRelation::Enemy)
    }
}

pub(super) fn neutral_team_relations() -> [[TeamRelation; MAX_TEAMS]; MAX_TEAMS] {
    let mut relations = [[TeamRelation::Neutral; MAX_TEAMS]; MAX_TEAMS];
    for (team, row) in relations.iter_mut().enumerate() {
        row[team] = TeamRelation::Ally;
    }
    relations
}

pub(super) fn hash_team_relations(
    checksum: &mut SyncChecksum,
    relations: &[[TeamRelation; MAX_TEAMS]; MAX_TEAMS],
) {
    for row in relations {
        for &relation in row {
            checksum.hash_u32(relation as u32);
        }
    }
}
