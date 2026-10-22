//! The command menu of the building that the local player selected.
//!
//! The menu shows the train and research commands of the building proto (`<Command>`) with the
//! retail rules of `BUser_handleInputCommandMenu` (`0x140594EF0`):
//!
//! - Research (type 0): the technology status for the building
//!   (`BTechTree_getTechStatus`, `0x140416980`) must be `Available` (2), and the building must
//!   not have the technology in its queue (`BUnitActionBuilding_isResearchQueued`,
//!   `0x1404F7EF0`).
//! - Train unit (1) and train squad (3): `BUnit_getTrainLimitRemaining` (`0x1404D0590`) must not
//!   be 0.
//! - All items: the player must have the cost (`BPlayer_checkCost`, `0x1402F14F0`) and the
//!   population room (`BPlayer_checkPops`, `0x1402F3290`). Else the press reports the missing
//!   cost (`BUser_reportMissingCost`, `0x1403909D0`) and sends no command.
//! - A left click sends a `BBuildingCommand` with count +1. A right click sends count −1, but
//!   only when the building has the item in its queue (`BUnit_getTrainQueueCount`,
//!   `0x1404D1C90`) or the technology status is `Researching` (3).
//!
//! The sim does the command checks of the item (owner, operational building, authored command,
//! `CommandEnable` technology effects, forbids) in [`sim::World::training_quote`] and
//! [`sim::World::research_quote`]. An item that fails them is not in the menu. The retail menu
//! refresh (`BUser_refreshCommandMenu`, `0x140599E60`) was not decompiled completely, so this
//! hide rule is our choice.

#[cfg(test)]
mod tests;

use pipeline::database::hw1::{Database, ProtoObject};
use sim::{
    BuildingCommand, EntityId, PlayerId, PopulationCost, Resources, TechStatus, TrainingKind, World,
};

/// The retail command type of a menu item (`BProtoObjectCommand` type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuItemKind {
    /// Research a technology (type 0).
    Research,
    /// Train a standalone unit (type 1).
    TrainUnit,
    /// Train a squad (type 3).
    TrainSquad,
}

impl MenuItemKind {
    /// Returns the command type name that the `CommandSelectable` technology effects use.
    const fn command_name(self) -> &'static str {
        match self {
            Self::Research => "Research",
            Self::TrainUnit => TrainingKind::Unit.command_name(),
            Self::TrainSquad => TrainingKind::Squad.command_name(),
        }
    }

    /// Returns the sim training kind, or `None` for research.
    const fn training_kind(self) -> Option<TrainingKind> {
        match self {
            Self::Research => None,
            Self::TrainUnit => Some(TrainingKind::Unit),
            Self::TrainSquad => Some(TrainingKind::Squad),
        }
    }
}

/// Why the player cannot use a menu item now. A press plays the retail "cant do that" sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UnavailableReason {
    /// A `CommandSelectable` technology effect turned the command off.
    NotSelectable,
    /// The technology status is not `Available`.
    TechnologyStatus(TechStatus),
    /// The building already has the technology in its queue.
    AlreadyQueued,
    /// The building has no `<TrainLimit>` room for one more item.
    TrainLimitReached,
}

/// The state of a menu item for a left click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuItemState {
    /// A left click queues the item.
    Available,
    /// The player does not have the cost.
    MissingResources,
    /// The player does not have the population room.
    MissingPopulation,
    /// The player cannot use the item now.
    Unavailable(UnavailableReason),
}

/// One train or research item of the building menu.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MenuItem {
    /// The command type.
    pub(super) kind: MenuItemKind,
    /// The runtime proto object, proto squad or technology ID.
    pub(super) target_id: i32,
    /// The proto name of the target.
    pub(super) target_name: String,
    /// The string table ID of the target display name.
    pub(super) display_name_id: Option<i32>,
    /// The cost of one item.
    pub(super) cost: Resources,
    /// The build or research points of one item.
    pub(super) total_points: f32,
    /// The count of this item in the building queue, including the current item.
    pub(super) queued: u32,
    /// The state of the item for a left click.
    pub(super) state: MenuItemState,
    /// If true, a right click cancels one item.
    pub(super) can_cancel: bool,
}

impl MenuItem {
    /// Returns the command that a left click sends, or `None` if the state blocks it.
    pub(super) fn queue_command(
        &self,
        player_id: PlayerId,
        building_id: EntityId,
    ) -> Option<BuildingCommand> {
        (self.state == MenuItemState::Available)
            .then(|| self.building_command(player_id, building_id, 1))
    }

    /// Returns the command that a right click sends, or `None` if nothing can be canceled.
    pub(super) fn cancel_command(
        &self,
        player_id: PlayerId,
        building_id: EntityId,
    ) -> Option<BuildingCommand> {
        self.can_cancel
            .then(|| self.building_command(player_id, building_id, -1))
    }

    fn building_command(
        &self,
        player_id: PlayerId,
        building_id: EntityId,
        count: i32,
    ) -> BuildingCommand {
        let player_id = i32::from(player_id);
        let recipients = vec![building_id];
        match self.kind {
            MenuItemKind::Research => {
                BuildingCommand::research(player_id, recipients, self.target_id, count)
            }
            MenuItemKind::TrainUnit => {
                BuildingCommand::train_units(player_id, recipients, self.target_id, count)
            }
            MenuItemKind::TrainSquad => {
                BuildingCommand::train_squads(player_id, recipients, self.target_id, count)
            }
        }
    }
}

/// Returns the train and research items of a building of `player_id`, in button slot order.
///
/// The list is empty if the building does not exist or belongs to another player.
pub(super) fn menu_items(
    world: &World,
    database: &Database,
    player_id: PlayerId,
    building_id: EntityId,
) -> Vec<MenuItem> {
    let Some(building) = world.get_building(building_id) else {
        return Vec::new();
    };
    if building.base.player_id != player_id {
        return Vec::new();
    }
    let Some(prototype) = database.objects.iter().find(|prototype| {
        prototype
            .name
            .eq_ignore_ascii_case(building.proto_object_name.trim())
    }) else {
        return Vec::new();
    };
    let mut commands = prototype.commands.iter().collect::<Vec<_>>();
    // Retail places each command in its `Position` slot. The slots have no screen layout here, so
    // the menu lists the commands by slot, and the commands without a slot follow.
    commands.sort_by_key(|command| command.position.unwrap_or(i32::MAX));
    commands
        .into_iter()
        .filter_map(|command| {
            let kind = command_kind(database, command.command_type.as_deref(), &command.target)?;
            menu_item(
                world,
                database,
                player_id,
                building_id,
                prototype,
                kind,
                &command.target,
            )
        })
        .collect()
}

/// Returns the menu kind of an authored command, or `None` for the other command types.
///
/// A command without a type is a research when the target is a technology, else it is a train
/// command for the target table, like the sim command check.
fn command_kind(
    database: &Database,
    command_type: Option<&str>,
    target: &str,
) -> Option<MenuItemKind> {
    let target = target.trim();
    match command_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) if value.eq_ignore_ascii_case("Research") => Some(MenuItemKind::Research),
        Some(value) if value.eq_ignore_ascii_case("TrainUnit") => Some(MenuItemKind::TrainUnit),
        Some(value) if value.eq_ignore_ascii_case("TrainSquad") => Some(MenuItemKind::TrainSquad),
        Some(_) => None,
        None if sim::technology_prototype_id(database, target).is_some() => {
            Some(MenuItemKind::Research)
        }
        None if sim::squad_prototype_id(database, target).is_some() => {
            Some(MenuItemKind::TrainSquad)
        }
        None => sim::object_prototype_id(database, target).map(|_| MenuItemKind::TrainUnit),
    }
}

fn menu_item(
    world: &World,
    database: &Database,
    player_id: PlayerId,
    building_id: EntityId,
    prototype: &ProtoObject,
    kind: MenuItemKind,
    target: &str,
) -> Option<MenuItem> {
    let player = world.get_player(player_id)?;
    let production = &world.get_building(building_id)?.production;
    let target = target.trim();
    let selectable =
        player
            .technologies
            .command_selectable(&prototype.name, kind.command_name(), target, true);
    let (target_id, display_name_id, quote) = match kind.training_kind() {
        None => {
            let target_id = sim::technology_prototype_id(database, target)?;
            let quote = world
                .research_quote(player_id, building_id, database, target_id)
                .ok()?;
            let display_name_id = database
                .techs
                .get(usize::try_from(target_id).ok()?)?
                .display_name_id;
            let queued = production
                .current_research()
                .into_iter()
                .chain(production.queued_research())
                .filter(|task| task.player_id() == player_id && task.technology_id() == target_id)
                .count();
            let rule = ItemRule::Research {
                status: quote.status,
                queued: queued > 0,
            };
            (
                target_id,
                display_name_id,
                ItemQuote {
                    cost: quote.cost,
                    population_costs: Vec::new(),
                    total_points: quote.total_points,
                    queued,
                    rule,
                },
            )
        }
        Some(training_kind) => {
            let (target_id, display_name_id) = match training_kind {
                TrainingKind::Squad => {
                    let target_id = sim::squad_prototype_id(database, target)?;
                    let squad = database.squads.get(usize::try_from(target_id).ok()?);
                    (target_id, squad.and_then(|squad| squad.display_name_id))
                }
                TrainingKind::Unit => {
                    let target_id = sim::object_prototype_id(database, target)?;
                    let object = database.objects.get(usize::try_from(target_id).ok()?);
                    (target_id, object.and_then(|object| object.display_name_id))
                }
            };
            let quote = world
                .training_quote(player_id, building_id, database, training_kind, target_id)
                .ok()?;
            // Retail `BUnitActionBuilding_getTrainQueueCount` (`0x1404F7390`) counts the current
            // and queued items of type 1 or 3 with this ID. The sim keeps the unit and squad IDs
            // in two tables, so the count also compares the kind.
            let queued = production
                .current_training()
                .into_iter()
                .chain(production.queued_training())
                .filter(|task| {
                    task.player_id() == player_id
                        && task.kind() == training_kind
                        && task.prototype_id() == target_id
                })
                .count();
            (
                target_id,
                display_name_id,
                ItemQuote {
                    cost: quote.cost,
                    population_costs: quote.population_costs,
                    total_points: quote.total_points,
                    queued,
                    rule: ItemRule::Train {
                        train_limit_remaining: quote.train_limit_remaining,
                    },
                },
            )
        }
    };
    let state = item_state(
        selectable,
        quote.rule,
        &player.resources,
        &quote.cost,
        || player.can_reserve_population(&quote.population_costs),
    );
    let can_cancel = match quote.rule {
        ItemRule::Research { status, .. } => status == TechStatus::Researching,
        ItemRule::Train { .. } => quote.queued > 0,
    };
    Some(MenuItem {
        kind,
        target_id,
        target_name: target.to_owned(),
        display_name_id,
        cost: quote.cost,
        total_points: quote.total_points,
        queued: u32::try_from(quote.queued).unwrap_or(u32::MAX),
        state,
        can_cancel,
    })
}

/// The values of one item that the state rules use.
struct ItemQuote {
    cost: Resources,
    population_costs: Vec<PopulationCost>,
    total_points: f32,
    queued: usize,
    rule: ItemRule,
}

/// The type-specific input of the state rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ItemRule {
    /// A research item: the technology status for the building and the queue check.
    Research { status: TechStatus, queued: bool },
    /// A train item: the remaining `<TrainLimit>` count, or `None` without a limit.
    Train { train_limit_remaining: Option<u32> },
}

/// Returns the state of an item in the check order of `BUser_handleInputCommandMenu`.
///
/// The type checks come before the cost and population checks, so an item that the player
/// cannot use and cannot pay for shows the type reason. Retail checks the item flag `+200`
/// (`BUICommandMenu_addItem`, `0x1404B5D20`) first: the menu refresh sets it when a population
/// of the item is at its cap, and a press then reports the missing population. This function
/// does not model that flag, so such an item shows the type reason.
fn item_state(
    selectable: bool,
    rule: ItemRule,
    resources: &Resources,
    cost: &Resources,
    has_population_room: impl FnOnce() -> bool,
) -> MenuItemState {
    if !selectable {
        return MenuItemState::Unavailable(UnavailableReason::NotSelectable);
    }
    match rule {
        ItemRule::Research { status, .. } if status != TechStatus::Available => {
            return MenuItemState::Unavailable(UnavailableReason::TechnologyStatus(status));
        }
        ItemRule::Research { queued: true, .. } => {
            return MenuItemState::Unavailable(UnavailableReason::AlreadyQueued);
        }
        ItemRule::Train {
            train_limit_remaining: Some(0),
        } => return MenuItemState::Unavailable(UnavailableReason::TrainLimitReached),
        ItemRule::Research { .. } | ItemRule::Train { .. } => {}
    }
    if !resources.can_afford(cost) {
        MenuItemState::MissingResources
    } else if !has_population_room() {
        MenuItemState::MissingPopulation
    } else {
        MenuItemState::Available
    }
}

/// Returns the hover text of a state.
fn state_text(state: MenuItemState) -> Option<&'static str> {
    match state {
        MenuItemState::Available => None,
        MenuItemState::MissingResources => Some("Not enough resources"),
        MenuItemState::MissingPopulation => Some("Not enough population room"),
        MenuItemState::Unavailable(reason) => Some(match reason {
            UnavailableReason::NotSelectable => "Not available",
            UnavailableReason::AlreadyQueued => "Already in the queue",
            UnavailableReason::TrainLimitReached => "Train limit reached",
            UnavailableReason::TechnologyStatus(status) => match status {
                TechStatus::Researching | TechStatus::CoopResearching => "Research in progress",
                TechStatus::Active => "Researched",
                TechStatus::Obtainable => "Requirements not met",
                TechStatus::Unobtainable | TechStatus::Disabled | TechStatus::Available => {
                    "Not available"
                }
            },
        }),
    }
}

/// Returns the cost text of an item, for example `Supplies 100`.
fn cost_text(cost: &Resources, resource_names: &[String]) -> String {
    let parts = cost
        .amounts
        .iter()
        .enumerate()
        .filter(|(_, amount)| **amount > 0.0)
        .map(|(index, amount)| {
            let name = resource_names.get(index).map_or("?", String::as_str);
            format!("{name} {amount:.0}")
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "Free".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Draws the menu items as buttons and returns the command of a click.
///
/// A left click on an item queues it. A right click cancels one. `name` gives the display name
/// of an item.
pub(super) fn draw(
    ui: &mut egui::Ui,
    items: &[MenuItem],
    resource_names: &[String],
    name: impl Fn(&MenuItem) -> String,
    player_id: PlayerId,
    building_id: EntityId,
) -> Option<BuildingCommand> {
    if items.is_empty() {
        ui.label(egui::RichText::new("No commands").color(egui::Color32::GRAY));
        return None;
    }
    let mut command = None;
    for item in items {
        let mut label = name(item);
        if item.queued > 0 {
            label = format!("{label} ({})", item.queued);
        }
        let color = if item.state == MenuItemState::Available {
            egui::Color32::WHITE
        } else {
            egui::Color32::GRAY
        };
        let verb = if item.kind == MenuItemKind::Research {
            "Research"
        } else {
            "Train"
        };
        let mut hover = format!(
            "{verb}: {}\nTime: {:.0} s",
            cost_text(&item.cost, resource_names),
            item.total_points
        );
        if let Some(text) = state_text(item.state) {
            hover = format!("{hover}\n{text}");
        }
        let response = ui
            .add(
                egui::Button::new(egui::RichText::new(label).color(color))
                    .min_size(egui::vec2(220.0, 0.0)),
            )
            .on_hover_text(hover);
        if response.clicked() {
            command = item.queue_command(player_id, building_id);
        } else if response.secondary_clicked() {
            command = item.cancel_command(player_id, building_id);
        }
    }
    command
}
