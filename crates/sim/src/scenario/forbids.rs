//! Initial per-player prototype forbids authored directly in an SCN file.

use crate::spawn::{object_prototype_id, squad_prototype_id};
use crate::world::{World, technology_prototype_id};
use pipeline::database::hw1::Database;
use pipeline::xmb::{Document, Node};

pub(super) fn apply_scenario_forbids(world: &mut World, database: &Database, document: &Document) {
    let Some(players) = document
        .root()
        .and_then(|root| root.children.iter().find(|node| node.name == "Players"))
    else {
        return;
    };

    for (index, player_node) in players
        .children
        .iter()
        .filter(|node| node.name == "Player")
        .take(crate::world::MAX_PLAYERS)
        .enumerate()
    {
        let player_id = u8::try_from(index + 1).unwrap_or(u8::MAX);
        let Some(player) = world.get_player_mut(player_id) else {
            continue;
        };
        for name in child_values(player_node, "ForbidObjects", "Object") {
            if let Some(id) = object_prototype_id(database, &name) {
                let _changed = player.set_object_forbidden(database, id, true);
            }
        }
        for name in child_values(player_node, "ForbidSquads", "Squad") {
            if let Some(id) = squad_prototype_id(database, &name) {
                let _changed = player.set_squad_forbidden(database, id, true);
            }
        }
        for name in child_values(player_node, "ForbidTechs", "Tech") {
            if let Some(id) = technology_prototype_id(database, &name) {
                let _changed = player.set_technology_forbidden(database, id, true);
            }
        }
    }
}

fn child_values(player: &Node, wrapper_name: &str, entry_name: &str) -> Vec<String> {
    player
        .children
        .iter()
        .find(|node| node.name == wrapper_name)
        .into_iter()
        .flat_map(|wrapper| &wrapper.children)
        .filter(|node| node.name == entry_name)
        .map(Node::text_string)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests;
