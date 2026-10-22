//! The selection panel: one row for each selected squad, then one row for each selected unit
//! without a squad (a building).
//!
//! A squad row has the protosquad display name, the member count and the health ratio of the
//! retail squad HP bar (`BHPBar_getSquadBarRatios`, `0x140042C20`). A unit row has the proto
//! object display name and the unit HP ratio (`BHPBar_getUnitBarRatios`, `0x140043220`).

use pipeline::database::hw1::Database;
use sim::{EntityId, World};

use super::super::bar_ratios::squad_bar_ratios;
use super::super::catalog::ClientCatalog;

/// One row of the selection panel.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SelectionRow {
    /// The proto name of the squad or unit.
    pub(super) name: String,
    /// The string table ID of the display name.
    pub(super) display_name_id: Option<i32>,
    /// The count of live members. A unit row has 1.
    pub(super) members: usize,
    /// The health ratio in `0..=1`.
    pub(super) health: f32,
}

/// Returns the rows of the selected squads, then of the selected units without a squad.
pub(super) fn selection_rows(
    world: &World,
    database: &Database,
    catalog: &ClientCatalog,
    squads: &[EntityId],
    units: &[EntityId],
) -> Vec<SelectionRow> {
    let squad_rows = squads.iter().filter_map(|&squad_id| {
        let squad = world.get_squad(squad_id)?;
        let protosquad = database
            .squads
            .iter()
            .find(|protosquad| protosquad.name == squad.proto_squad_name);
        let first_unit = squad
            .unit_ids
            .first()
            .and_then(|&unit_id| world.get_unit(unit_id));
        // A squad without a protosquad record (an implicit squad of a proto object) has the
        // display name of its unit.
        let display_name_id = protosquad.map_or_else(
            || {
                first_unit
                    .and_then(|unit| object_display_name_id(database, &unit.proto_object_name))
            },
            |protosquad| protosquad.display_name_id,
        );
        let health = squad_bar_ratios(world, catalog, squad).map_or_else(
            || first_unit.map_or(0.0, unit_health),
            |ratios| ratios.hitpoints,
        );
        Some(SelectionRow {
            name: squad.proto_squad_name.clone(),
            display_name_id,
            members: squad.unit_ids.len(),
            health,
        })
    });
    let unit_rows = units.iter().filter_map(|&unit_id| {
        let unit = world.get_unit(unit_id)?;
        unit.squad_id.is_none().then(|| SelectionRow {
            name: unit.proto_object_name.clone(),
            display_name_id: object_display_name_id(database, &unit.proto_object_name),
            members: 1,
            health: unit_health(unit),
        })
    });
    squad_rows.chain(unit_rows).collect()
}

fn object_display_name_id(database: &Database, name: &str) -> Option<i32> {
    database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(name.trim()))?
        .display_name_id
}

fn unit_health(unit: &sim::Unit) -> f32 {
    if unit.max_hitpoints > 0.0 {
        (unit.hitpoints / unit.max_hitpoints).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Draws the selection rows. `name` gives the display name of a row.
pub(super) fn draw(
    ui: &mut egui::Ui,
    rows: &[SelectionRow],
    name: impl Fn(&SelectionRow) -> String,
) {
    for row in rows {
        ui.horizontal(|ui| {
            let label = if row.members > 1 {
                format!("{} ×{}", name(row), row.members)
            } else {
                name(row)
            };
            ui.add(
                egui::ProgressBar::new(row.health.clamp(0.0, 1.0))
                    .fill(health_color(row.health))
                    .desired_width(60.0),
            );
            ui.label(egui::RichText::new(label).color(egui::Color32::WHITE));
        });
    }
}

/// Returns the bar colour of a health ratio: green, then yellow below one half, then red below
/// one quarter. Retail draws the panel in Flash; these colours are our choice.
fn health_color(health: f32) -> egui::Color32 {
    if health < 0.25 {
        egui::Color32::from_rgb(200, 40, 30)
    } else if health < 0.5 {
        egui::Color32::from_rgb(220, 190, 40)
    } else {
        egui::Color32::from_rgb(60, 180, 60)
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{
        BARRACKS, MARINE, MARINE_NAME_ID, PLAYER, menu_database, menu_world,
    };
    use super::*;

    #[test]
    fn rows_list_squads_then_buildings_with_names_members_and_health() {
        let database = menu_database(false);
        let (mut world, barracks_id) = menu_world(&database, 10.0, 1_000.0);
        let marine_id = sim::squad_prototype_id(&database, MARINE).unwrap();
        let squad_id = sim::spawn_squad_at(
            &mut world,
            &database,
            PLAYER,
            marine_id,
            glam::Vec3::new(20.0, 0.0, 0.0),
            glam::Vec3::Z,
        )
        .unwrap();
        let members = world.get_squad(squad_id).unwrap().unit_ids.clone();
        let building = world.get_unit_mut(barracks_id).unwrap();
        building.max_hitpoints = 1_000.0;
        building.hitpoints = 500.0;

        let catalog = ClientCatalog::new(&database, None);
        let rows = selection_rows(
            &world,
            &database,
            &catalog,
            &[squad_id],
            &[members[0], barracks_id],
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, MARINE);
        assert_eq!(rows[0].display_name_id, Some(MARINE_NAME_ID));
        assert_eq!(rows[0].members, members.len());
        assert_eq!(rows[1].name, BARRACKS);
        assert_eq!(rows[1].members, 1);
        assert!(
            (rows[1].health - 0.5).abs() < 1e-5,
            "health {}",
            rows[1].health
        );
    }
}
