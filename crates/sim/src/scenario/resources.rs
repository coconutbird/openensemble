//! Player starting resources resolved from layered leader and game data.

use crate::player::{Player, Resources};
use pipeline::database::hw1::Database;

pub(super) fn apply_leader_starting_resources(
    player: &mut Player,
    database: &Database,
    leader_id: i32,
) {
    player.resources = Resources::new();
    let Some(leader) = usize::try_from(leader_id)
        .ok()
        .and_then(|index| database.leaders.get(index))
    else {
        player.initialize_resource_totals();
        return;
    };
    let resource_names = database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .map_or(&[][..], |resources| resources.entries.as_slice());
    for entry in &leader.resources {
        let Some(resource_id) = resource_names.iter().position(|resource| {
            resource
                .name
                .trim()
                .eq_ignore_ascii_case(entry.resource_type.trim())
        }) else {
            continue;
        };
        player.resources.set(resource_id, entry.amount);
    }
    player.initialize_resource_totals();
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
    use pipeline::database::hw1::leaders::ResourceEntry;
    use pipeline::database::hw1::{GameData, Leader};

    #[test]
    fn layered_resource_names_seed_balance_and_lifetime_total() {
        let mut database = Database::new();
        database.game_data = Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![
                    ResourceDef {
                        name: "Supplies".to_owned(),
                        ..ResourceDef::default()
                    },
                    ResourceDef {
                        name: "Power".to_owned(),
                        ..ResourceDef::default()
                    },
                ],
            }),
            ..GameData::default()
        });
        database.leaders.push(Leader {
            resources: vec![ResourceEntry {
                resource_type: "supplies".to_owned(),
                amount: 800.0,
            }],
            ..Leader::default()
        });
        let mut player = Player::new(1);

        apply_leader_starting_resources(&mut player, &database, 0);

        assert!((player.get_resource(0) - 800.0).abs() < f32::EPSILON);
        assert!((player.get_total_resource(0) - 800.0).abs() < f32::EPSILON);
        assert!(player.get_resource(1).abs() < f32::EPSILON);
    }
}
