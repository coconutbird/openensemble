//! Retail civilization and leader technology bootstrap for scenario players.

use crate::player::PlayerId;
use crate::world::World;
use pipeline::database::hw1::Database;

pub(super) fn activate_all_starting_technologies(world: &mut World, database: &Database) {
    let player_ids = world
        .active_players()
        .map(|player| player.id)
        .collect::<Vec<_>>();
    for player_id in player_ids {
        activate_player_starting_technologies(world, database, player_id);
    }
}

pub(super) fn activate_player_starting_technologies(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
) {
    world.initialize_shadow_technologies(player_id, database);
    let Some(player) = world.get_player(player_id) else {
        return;
    };
    let civilization_technology = usize::try_from(player.civ_id)
        .ok()
        .and_then(|index| database.civs.get(index))
        .and_then(|civilization| civilization.civ_tech.as_deref());
    let leader_technology = usize::try_from(player.leader_id)
        .ok()
        .and_then(|index| database.leaders.get(index))
        .and_then(|leader| leader.tech.as_deref());
    let names = [civilization_technology, leader_technology]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for name in names {
        let _activation = world.activate_technology(player_id, database, &name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::techs::{
        EffectTarget, EffectsWrapper, PrereqsWrapper, TechEffect, TechStatusEntry,
    };
    use pipeline::database::hw1::{Civ, Leader, Tech};

    #[test]
    fn civilization_then_leader_techs_are_authoritative_and_ordered() {
        let mut database = Database::new();
        database.civs.push(Civ {
            name: "TestCiv".to_owned(),
            civ_tech: Some("CivBootstrap".to_owned()),
            ..Civ::default()
        });
        database.leaders.push(Leader {
            name: "TestLeader".to_owned(),
            civ: Some("TestCiv".to_owned()),
            tech: Some("LeaderBootstrap".to_owned()),
            ..Leader::default()
        });
        database.techs.extend([
            shadow_technology("RootShadow", &[]),
            hitpoint_technology("CivBootstrap", 1.5),
            shadow_technology("CivShadow", &["CivBootstrap"]),
            shadow_technology("ChainedShadow", &["CivShadow"]),
            hitpoint_technology("LeaderBootstrap", 2.0),
            shadow_technology("LeaderShadow", &["CivBootstrap", "LeaderBootstrap"]),
            Tech {
                name: "ForbiddenShadow".to_owned(),
                flags: vec!["Shadow".to_owned(), "Forbid".to_owned()],
                ..Tech::default()
            },
            Tech {
                name: "UniqueShadow".to_owned(),
                flags: vec!["Shadow".to_owned(), "UniqueProtoUnitInstance".to_owned()],
                ..Tech::default()
            },
            Tech {
                name: "UnobtainableShadow".to_owned(),
                status: Some("Unobtainable".to_owned()),
                flags: vec!["Shadow".to_owned()],
                ..Tech::default()
            },
            Tech {
                name: "AlphaShadow".to_owned(),
                alpha: Some(1),
                flags: vec!["Shadow".to_owned()],
                ..Tech::default()
            },
        ]);
        let mut world = World::new();
        world.init_players(1);
        let player = world.get_player_mut(1).unwrap();
        player.civ_id = 0;
        player.leader_id = 0;
        let before = world.checksum();

        activate_all_starting_technologies(&mut world, &database);

        let player = world.get_player(1).unwrap();
        assert_eq!(
            player
                .technologies
                .active_technologies()
                .collect::<Vec<_>>(),
            [
                "RootShadow",
                "CivBootstrap",
                "CivShadow",
                "ChainedShadow",
                "LeaderBootstrap",
                "LeaderShadow"
            ]
        );
        assert!(!player.technologies.is_active("ForbiddenShadow"));
        assert!(!player.technologies.is_active("UniqueShadow"));
        assert!(!player.technologies.is_active("UnobtainableShadow"));
        assert!(!player.technologies.is_active("AlphaShadow"));
        assert!(player.technologies.hitpoints("test_unit", 100.0) > 100.0);
        assert_ne!(world.checksum(), before);
    }

    fn hitpoint_technology(name: &str, amount: f32) -> Tech {
        Tech {
            name: name.to_owned(),
            effects: Some(EffectsWrapper {
                entries: vec![TechEffect {
                    effect_type: "Data".to_owned(),
                    subtype: Some("Hitpoints".to_owned()),
                    amount: Some(amount),
                    relativity: Some("Percent".to_owned()),
                    target: Some(EffectTarget {
                        target_type: Some("ProtoUnit".to_owned()),
                        value: Some("test_unit".to_owned()),
                    }),
                    ..TechEffect::default()
                }],
            }),
            ..Tech::default()
        }
    }

    fn shadow_technology(name: &str, prerequisites: &[&str]) -> Tech {
        Tech {
            name: name.to_owned(),
            flags: vec!["Shadow".to_owned()],
            prereqs: (!prerequisites.is_empty()).then(|| PrereqsWrapper {
                entries: prerequisites
                    .iter()
                    .map(|prerequisite| TechStatusEntry {
                        tech: (*prerequisite).to_owned(),
                        status: "Active".to_owned(),
                        ..TechStatusEntry::default()
                    })
                    .collect(),
                ..PrereqsWrapper::default()
            }),
            ..Tech::default()
        }
    }
}
