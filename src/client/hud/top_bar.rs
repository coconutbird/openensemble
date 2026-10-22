//! The top bar: the resources, the populations and the game time of the local player.
//!
//! Retail `BUIGameHud_updatePlayerStats` (`0x140221290`) writes the stat texts:
//!
//! - A resource is `BPlayer_getResource` as an integer (truncated, at most 99999).
//! - A population is `"%.0f/%.0f"` of count + future (`BPlayer_getPopCount` +
//!   `BPlayer_getPopFuture`) and the smaller of the cap and the maximum (`BPlayer_getPopCap`,
//!   `BPlayer_getPopMax`).
//!
//! Retail shows the per-civ `PlayerStats` list of `ui/game/uigame.xml` (one population and two
//! resources). The database does not read that file, so the bar shows each resource of the
//! `GameData` resource order and each population slot.

use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use sim::Player;

/// The largest resource value that the retail stat text shows.
const MAX_SHOWN_RESOURCE: i32 = 99_999;

/// One stat of the top bar.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Stat {
    /// The name of the resource or population type.
    pub(super) name: String,
    /// The retail stat text.
    pub(super) text: String,
}

/// Returns the resource stats, then the population stats of a player.
pub(super) fn player_stats(database: &Database, player: &Player) -> Vec<Stat> {
    let game_data = database.game_data.as_ref();
    let resources = game_data
        .and_then(|game_data| game_data.resources.as_ref())
        .map_or(&[][..], |resources| resources.entries.as_slice());
    let pops = game_data
        .and_then(|game_data| game_data.pops.as_ref())
        .map_or(&[][..], |pops| pops.entries.as_slice());
    let resource_stats = resources.iter().enumerate().map(|(index, resource)| Stat {
        name: resource.name.clone(),
        text: resource_text(player.resources.get(index)),
    });
    let population_stats = player
        .population
        .iter()
        .enumerate()
        .map(|(index, population)| Stat {
            name: pops
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("Pop {index}")),
            text: population_text(population),
        });
    resource_stats.chain(population_stats).collect()
}

/// Returns the retail text of a resource amount.
fn resource_text(amount: f32) -> String {
    // `cvttss2si`: the retail conversion truncates toward zero.
    let value = amount
        .to_i32()
        .unwrap_or(if amount > 0.0 { i32::MAX } else { 0 });
    value.min(MAX_SHOWN_RESOURCE).to_string()
}

/// Returns the retail text of a population slot.
fn population_text(population: &sim::Population) -> String {
    let used = population.count + population.future;
    let cap = population.cap.min(population.max);
    format!("{used:.0}/{cap:.0}")
}

/// Returns the game time as `m:ss`, or `h:mm:ss` after one hour.
pub(super) fn game_time_text(game_time_ms: u32) -> String {
    let seconds = game_time_ms / 1_000;
    let (hours, minutes, seconds) = (seconds / 3_600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Draws the stats and the game time in one row.
pub(super) fn draw(ui: &mut egui::Ui, stats: &[Stat], game_time_ms: u32) {
    ui.horizontal(|ui| {
        for stat in stats {
            ui.label(egui::RichText::new(format!("{}:", stat.name)).color(egui::Color32::GRAY));
            ui.label(
                egui::RichText::new(&stat.text)
                    .strong()
                    .color(egui::Color32::WHITE),
            );
            ui.add_space(12.0);
        }
        ui.label(egui::RichText::new("Time:").color(egui::Color32::GRAY));
        ui.label(
            egui::RichText::new(game_time_text(game_time_ms))
                .strong()
                .color(egui::Color32::WHITE),
        );
    });
}

#[cfg(test)]
mod tests {
    use pipeline::database::hw1::GameData;
    use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};

    use super::*;

    #[test]
    fn stats_use_the_retail_texts_in_game_data_order() {
        let database = Database {
            game_data: Some(GameData {
                resources: Some(ResourcesWrapper {
                    entries: ["Supplies", "Power"]
                        .map(|name| ResourceDef {
                            name: name.to_owned(),
                            deductable: Some(true),
                        })
                        .into(),
                }),
                pops: Some(PopsWrapper {
                    entries: vec!["Unit".to_owned()],
                }),
                ..GameData::default()
            }),
            ..Database::default()
        };
        let mut player = Player::new(1);
        player.resources.set(0, 812.9);
        player.resources.set(1, 250_000.0);
        player.configure_population_slots(1);
        let population = player.get_population_mut(0).unwrap();
        population.cap = 40.0;
        population.max = 30.0;
        population.count = 9.0;
        population.future = 2.0;

        let stats = player_stats(&database, &player);

        let texts = stats
            .iter()
            .map(|stat| (stat.name.as_str(), stat.text.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            [("Supplies", "812"), ("Power", "99999"), ("Unit", "11/30")]
        );
    }

    #[test]
    fn game_time_shows_minutes_and_hours() {
        assert_eq!(game_time_text(0), "0:00");
        assert_eq!(game_time_text(65_999), "1:05");
        assert_eq!(game_time_text(3_723_000), "1:02:03");
    }
}
