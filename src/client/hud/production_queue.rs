//! The production queue of the menu building: the current item with its progress, then the
//! queued items.
//!
//! The sim gives the queued training and research items as two lists, so the queue shows the
//! queued training items before the queued research items. Retail keeps one mixed queue.

use pipeline::database::hw1::Database;
use sim::{BuildingProduction, ResearchTask, TrainingKind, TrainingTask};

/// One row of the production queue.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct QueueRow {
    /// The proto name of the item.
    pub(super) name: String,
    /// The string table ID of the item display name.
    pub(super) display_name_id: Option<i32>,
    /// The completed fraction of the current item, or `None` for a queued item.
    pub(super) fraction_complete: Option<f32>,
}

/// Returns the rows of the training and research items of a building queue.
pub(super) fn queue_rows(database: &Database, production: &BuildingProduction) -> Vec<QueueRow> {
    let training_row = |task: &TrainingTask, fraction_complete| QueueRow {
        name: task.prototype_name().to_owned(),
        display_name_id: training_display_name_id(database, task.kind(), task.prototype_id()),
        fraction_complete,
    };
    let research_row = |task: &ResearchTask, fraction_complete| QueueRow {
        name: task.technology_name().to_owned(),
        display_name_id: usize::try_from(task.technology_id())
            .ok()
            .and_then(|index| database.techs.get(index))
            .and_then(|technology| technology.display_name_id),
        fraction_complete,
    };
    production
        .current_training()
        .map(|task| training_row(task, Some(task.fraction_complete())))
        .into_iter()
        .chain(
            production
                .current_research()
                .map(|task| research_row(task, Some(task.fraction_complete()))),
        )
        .chain(
            production
                .queued_training()
                .map(|task| training_row(task, None)),
        )
        .chain(
            production
                .queued_research()
                .map(|task| research_row(task, None)),
        )
        .collect()
}

fn training_display_name_id(
    database: &Database,
    kind: TrainingKind,
    prototype_id: i32,
) -> Option<i32> {
    let index = usize::try_from(prototype_id).ok()?;
    match kind {
        TrainingKind::Squad => database.squads.get(index)?.display_name_id,
        TrainingKind::Unit => database.objects.get(index)?.display_name_id,
    }
}

/// Draws the queue rows. `name` gives the display name of a row.
pub(super) fn draw(ui: &mut egui::Ui, rows: &[QueueRow], name: impl Fn(&QueueRow) -> String) {
    if rows.is_empty() {
        ui.label(egui::RichText::new("Queue empty").color(egui::Color32::GRAY));
        return;
    }
    for row in rows {
        match row.fraction_complete {
            Some(fraction) => {
                ui.add(
                    egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                        .text(name(row))
                        .desired_width(220.0),
                );
            }
            None => {
                ui.label(name(row));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use sim::TrainingKind;

    use super::super::fixture::{
        MARINE, MARINE_NAME_ID, PLAYER, UPGRADE, UPGRADE_NAME_ID, menu_database, menu_world,
    };
    use super::*;

    #[test]
    fn rows_give_the_current_item_progress_then_the_queued_items() {
        let database = menu_database(false);
        let (mut world, barracks_id) = menu_world(&database, 10.0, 1_000.0);
        let marine_id = sim::squad_prototype_id(&database, MARINE).unwrap();
        let upgrade_id = sim::technology_prototype_id(&database, UPGRADE).unwrap();
        world
            .queue_training(
                PLAYER,
                barracks_id,
                &database,
                TrainingKind::Squad,
                marine_id,
                2,
            )
            .unwrap();
        world
            .queue_research(PLAYER, barracks_id, &database, upgrade_id)
            .unwrap();
        // The first update starts the current item. The second one adds its build points.
        let _update = world.update_production(0.05, &database);
        let _update = world.update_production(0.02, &database);

        let production = &world.get_building(barracks_id).unwrap().production;
        let rows = queue_rows(&database, production);

        let summary = rows
            .iter()
            .map(|row| {
                (
                    row.name.as_str(),
                    row.display_name_id,
                    row.fraction_complete.is_some(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                (MARINE, Some(MARINE_NAME_ID), true),
                (MARINE, Some(MARINE_NAME_ID), false),
                (UPGRADE, Some(UPGRADE_NAME_ID), false),
            ]
        );
        let fraction = rows[0].fraction_complete.unwrap();
        assert!(fraction > 0.0 && fraction < 1.0, "fraction {fraction}");
    }
}
