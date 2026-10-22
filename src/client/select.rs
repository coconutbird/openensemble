//! The selection actions of the local player, with the retail rules.
//!
//! - Click: `BUser_selectAtCursor` (`0x140584F40`).
//! - Shift click: `BUser_toggleSelectHovered` (`0x140584C50`).
//! - Drag box: `BUser_dragSelect` (`0x140585BC0`).
//! - Double click and Ctrl click: `BUser_doubleClickSelect` (`0x1405A2890`).
//! - Q and E: `BUser_selectAllOnScreenOrGlobal` (`0x1405B0800`).
//!
//! Retail also rejects units with the engine flags `unit+1106 & 4`, `unit+193 & 0x40`,
//! `unit+194 & 0x41` and `unit+761 & 2`. The simulation does not model these flags, so the
//! client does not test them. Co-op players are not modelled: the local player is the only
//! owner that counts as "own".

use glam::{Mat4, Vec2, Vec3, Vec4};
use render::picking::project_point;
use render::world_view::WorldCamera;
use sim::{EntityId, PlayerId, Unit, World};

use super::catalog::{ClientCatalog, DEFAULT_SUB_SELECT_SORT, ObjectInfo, SelectType};
use super::hover::{Hover, HoverType};
use super::pick::clickable;
use super::selection::Selection;

/// A drag box selects only when both sides are at least this long, in pixels.
pub(crate) const DRAG_SELECT_MIN_SIZE: f32 = 2.0;

/// The object types that Shift click does not toggle (`xmmword_141524B20`).
const TOGGLE_EXCLUDED_TYPES: [&str; 4] = [
    "Building",
    "BuildingSocket",
    "TurretSocket",
    "TurretBuilding",
];

/// The world, the player and the camera of a selection action.
#[derive(Clone, Copy)]
pub(crate) struct SelectContext<'a> {
    pub(crate) world: &'a World,
    pub(crate) catalog: &'a ClientCatalog,
    /// The local player.
    pub(crate) player: PlayerId,
    /// The camera of the frame, for the frustum and the drag box.
    pub(crate) camera: &'a WorldCamera,
}

impl SelectContext<'_> {
    fn unit(&self, unit: EntityId) -> Option<(&Unit, &ObjectInfo)> {
        let entity = self.world.units.get(unit)?;
        let info = self.catalog.object(&entity.proto_object_name)?;
        Some((entity, info))
    }
}

/// Returns the select type of `unit` (`BUnit_getSelectType`, `0x1404DE170`): None for a unit
/// that is not selectable. Retail also tests the team selection mask (`unit+756`), which the
/// simulation does not model.
fn select_type(unit: &Unit, info: &ObjectInfo) -> SelectType {
    if unit.base.is_selectable() {
        info.select_type
    } else {
        SelectType::None
    }
}

impl SelectContext<'_> {
    /// Returns the select type and the proto name of the first live selected unit.
    fn first_selected(&self, selection: &Selection) -> Option<(SelectType, &str, PlayerId)> {
        selection.units().iter().find_map(|&unit| {
            let (entity, info) = self.unit(unit)?;
            Some((
                select_type(entity, info),
                entity.proto_object_name.as_str(),
                entity.base.player_id,
            ))
        })
    }

    /// `BUser_canSelectUnit` (`0x1405A8C20`): a live unit of select type `Unit`, `SingleUnit` or
    /// `SingleType` (`BuildingMenuOnSelect` removes `Command`), with the owner of the first selected
    /// unit unless `replacing`.
    fn can_select_unit(&self, selection: &Selection, unit: EntityId, replacing: bool) -> bool {
        let Some((entity, info)) = self.unit(unit) else {
            return false;
        };
        if !entity.base.is_alive()
            || !matches!(
                select_type(entity, info),
                SelectType::Unit | SelectType::SingleUnit | SelectType::SingleType
            )
        {
            return false;
        }
        replacing
            || self
                .first_selected(selection)
                .is_none_or(|(_, _, owner)| owner == entity.base.player_id)
    }

    /// Returns `true` if the select type of `unit` fits the selection: the same select type as
    /// the first selected unit, and the same proto for `SingleType`.
    fn fits_selection(&self, selection: &Selection, unit: &Unit, info: &ObjectInfo) -> bool {
        let select_type = select_type(unit, info);
        self.first_selected(selection)
            .is_none_or(|(first_type, first_proto, _)| {
                first_type == select_type
                    && (select_type != SelectType::SingleType
                        || first_proto == unit.proto_object_name)
            })
    }

    /// Returns `true` for an own live unit of select type Unit with an army `GotoType`: the
    /// filter of the drag box and of the screen and global selects.
    fn is_own_army_unit(&self, unit: &Unit, info: &ObjectInfo) -> bool {
        unit.base.player_id == self.player
            && unit.base.is_alive()
            && select_type(unit, info) == SelectType::Unit
            && info.army_goto_type
    }

    /// Returns `true` if the bounding sphere of `unit` is in the camera frustum.
    ///
    /// Retail tests the sphere at `unit+208` with the radius `unit+284`. The client uses the
    /// position raised by the obstruction half height and the obstruction half-extent length.
    fn in_frustum(&self, unit: &Unit) -> bool {
        let extents = unit.obstruction_half_extents;
        let center = unit.base.position + Vec3::Y * extents.y;
        sphere_in_frustum(self.camera.view_projection, center, extents.length())
    }

    /// Sorts the selected squads by the protosquad `SubSelectSort`.
    fn rebuild(&self, selection: &mut Selection) {
        rebuild_selection(self.world, self.catalog, selection);
    }
}

/// Removes the dead units of `selection` and sorts its squads by the protosquad
/// `SubSelectSort` (`BSelectionManager_rebuildSelectedSquads`).
pub(crate) fn rebuild_selection(world: &World, catalog: &ClientCatalog, selection: &mut Selection) {
    selection.rebuild_squads(world, |squad| {
        world
            .squads
            .get(squad)
            .and_then(|squad| catalog.squad(&squad.proto_squad_name))
            .map_or(DEFAULT_SUB_SELECT_SORT, |info| info.sub_select_sort)
    });
}

/// The last unit that a click selected and the ground point of that click (`user+5728` and
/// `user+5712`). A second click on the same point selects the next overlapping squad.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ClickCycle {
    last: Option<(EntityId, Vec3)>,
}

/// Returns the unit that a click picks: the hover entity of type 1, or else the first
/// candidate under the pick square that is not a Target and that `clickable` accepts.
fn clicked_unit(context: &SelectContext<'_>, hover: &Hover) -> Option<EntityId> {
    if hover.kind == HoverType::Selectable {
        return hover.entity;
    }
    let first = hover.candidates.iter().find(|candidate| {
        context
            .unit(candidate.unit)
            .is_some_and(|(unit, info)| select_type(unit, info) != SelectType::Target)
    })?;
    clickable(context.world, context.catalog, context.player, first.unit).then_some(first.unit)
}

/// Selects the unit under the cursor, like `BUser_selectAtCursor`.
///
/// - A Command object (a building) replaces the selection; an own building also opens its
///   command menu (`BuildingMenuOnSelect`).
/// - A `SingleUnit` object replaces the selection.
/// - Another unit must pass `BUser_canSelectUnit` and fit the select type of the selection.
///   With `replace`, the selection is cleared first. A second click on the same ground point
///   selects the next candidate of another squad.
pub(crate) fn select_at_cursor(
    context: &SelectContext<'_>,
    selection: &mut Selection,
    hover: &Hover,
    replace: bool,
    cycle: &mut ClickCycle,
) {
    // A click on the ground does not change the selection.
    let Some(picked) = clicked_unit(context, hover) else {
        return;
    };
    let Some((unit, info)) = context.unit(picked) else {
        return;
    };
    let owned = unit.base.player_id == context.player;
    let allowed = !info.must_own_to_select || owned;
    match select_type(unit, info) {
        select_type @ (SelectType::Command | SelectType::SingleUnit) => {
            if replace || allowed {
                selection.clear();
            }
            if allowed {
                selection.select_unit(context.world, picked);
                if owned && select_type == SelectType::Command {
                    selection.open_menu(picked);
                }
            }
        }
        _ => select_generic(context, selection, hover, picked, replace, cycle),
    }
    context.rebuild(selection);
}

fn select_generic(
    context: &SelectContext<'_>,
    selection: &mut Selection,
    hover: &Hover,
    picked: EntityId,
    replace: bool,
    cycle: &mut ClickCycle,
) {
    if !context.can_select_unit(selection, picked, replace) {
        return;
    }
    let chosen = match (cycle.last, hover.point) {
        (Some((last, point)), Some(hover_point)) if point == hover_point => {
            next_overlapping(context, selection, hover, last, replace).unwrap_or(picked)
        }
        _ => picked,
    };
    if replace {
        selection.clear();
    }
    let Some((unit, info)) = context.unit(chosen) else {
        return;
    };
    if !context.fits_selection(selection, unit, info) || selection.contains_unit(chosen) {
        return;
    }
    if selection.select_unit(context.world, chosen) {
        cycle.last = hover.point.map(|point| (chosen, point));
    }
}

/// Returns the candidate after `last` (cyclic) that can be selected and that is in another
/// squad than `last`.
fn next_overlapping(
    context: &SelectContext<'_>,
    selection: &Selection,
    hover: &Hover,
    last: EntityId,
    replace: bool,
) -> Option<EntityId> {
    let candidates = &hover.candidates;
    let start = candidates
        .iter()
        .position(|candidate| candidate.unit == last)?;
    let last_squad = context.world.units.get(last)?.squad_id;
    (1..candidates.len())
        .map(|step| candidates[(start + step) % candidates.len()].unit)
        .find(|&unit| {
            context.can_select_unit(selection, unit, replace)
                && context
                    .world
                    .units
                    .get(unit)
                    .is_some_and(|entity| entity.squad_id != last_squad)
        })
}

/// Toggles the squad of the hover entity, like `BUser_toggleSelectHovered`.
///
/// The unit must be own, must not be a building or socket type, and must fit the select type
/// of the selection. A selected squad is removed; another squad is added.
pub(crate) fn toggle_select_hovered(
    context: &SelectContext<'_>,
    selection: &mut Selection,
    hover: &Hover,
) {
    let Some(picked) = hover.entity else {
        return;
    };
    let Some((unit, info)) = context.unit(picked) else {
        return;
    };
    let excluded = TOGGLE_EXCLUDED_TYPES.iter().any(|excluded| {
        info.object_types
            .iter()
            .any(|kind| kind.eq_ignore_ascii_case(excluded))
    });
    if unit.base.player_id != context.player
        || excluded
        || !context.fits_selection(selection, unit, info)
    {
        return;
    }
    let Some(squad) = unit.squad_id else {
        return;
    };
    if selection.contains_squad(squad) {
        selection.unselect_squad(context.world, squad);
    } else {
        selection.select_squad(context.world, squad);
    }
    context.rebuild(selection);
}

/// Selects the own army units in the screen box between `start` and `end`, like
/// `BUser_dragSelect`.
///
/// The box must be at least 2 pixels wide and high; a smaller box changes nothing and returns
/// `false`. The selection is cleared first; `saved` units (the selection at the start of a
/// Shift drag) are selected again after the box.
pub(crate) fn drag_select(
    context: &SelectContext<'_>,
    selection: &mut Selection,
    start: Vec2,
    end: Vec2,
    saved: &[EntityId],
) -> bool {
    let size = (end - start).abs();
    if size.x < DRAG_SELECT_MIN_SIZE || size.y < DRAG_SELECT_MIN_SIZE {
        return false;
    }
    let min = start.min(end);
    let max = start.max(end);
    selection.clear();
    for (unit_id, unit) in context.world.units.iter() {
        let Some(info) = context.catalog.object(&unit.proto_object_name) else {
            continue;
        };
        if !context.is_own_army_unit(unit, info) {
            continue;
        }
        let inside = project_point(context.camera, unit.base.position)
            .is_some_and(|screen| screen.cmpge(min).all() && screen.cmple(max).all());
        if inside {
            selection.select_unit(context.world, unit_id);
        }
    }
    for &unit in saved {
        selection.select_unit(context.world, unit);
    }
    context.rebuild(selection);
    true
}

/// Selects every own unit of the clicked kind in view, like `BUser_doubleClickSelect`.
///
/// The kind is the `SubSelectSort` of the clicked unit. Without `shift` the selection is
/// cleared first. A unit must fit the select type of the first selected unit, pass
/// `BUser_canSelectUnit`, be in the playable bounds and in the camera frustum. A `SingleUnit`
/// kind selects one unit. With `add`, the squads of the selection before the action are
/// selected again.
pub(crate) fn double_click_select(
    context: &SelectContext<'_>,
    selection: &mut Selection,
    hover: &Hover,
    shift: bool,
    add: bool,
) {
    let saved: Vec<EntityId> = if add {
        selection.squads().to_vec()
    } else {
        Vec::new()
    };
    let Some(picked) = clicked_unit(context, hover) else {
        return;
    };
    if !context.can_select_unit(selection, picked, !shift) {
        return;
    }
    let Some((unit, info)) = context.unit(picked) else {
        return;
    };
    if unit.base.player_id != context.player {
        return;
    }
    let key = info.sub_select_sort;
    let picked_type = select_type(unit, info);
    if !shift {
        selection.clear();
    }
    if picked_type == SelectType::SingleUnit && !selection.is_empty() {
        return;
    }
    if picked_type == SelectType::SingleType && !selection.is_empty() {
        let mixed = selection.units().iter().any(|&selected| {
            context
                .unit(selected)
                .is_some_and(|(_, selected_info)| selected_info.sub_select_sort != key)
        });
        if mixed {
            return;
        }
    }
    let mut first = context
        .first_selected(selection)
        .map(|(select_type, _, _)| {
            let key = selection
                .units()
                .iter()
                .find_map(|&unit| context.unit(unit))
                .map_or(DEFAULT_SUB_SELECT_SORT, |(_, info)| info.sub_select_sort);
            (select_type, key)
        });
    for (unit_id, unit) in context.world.units.iter() {
        let Some(info) = context.catalog.object(&unit.proto_object_name) else {
            continue;
        };
        if unit.base.player_id != context.player || info.sub_select_sort != key {
            continue;
        }
        let unit_type = select_type(unit, info);
        let fits = first.is_none_or(|(first_type, first_key)| {
            unit_type == first_type && (unit_type != SelectType::SingleType || key == first_key)
        });
        let in_bounds = !context
            .world
            .is_outside_playable_bounds(unit.base.position, false);
        if fits
            && context.can_select_unit(selection, unit_id, false)
            && in_bounds
            && unit.base.position.is_finite()
            && context.in_frustum(unit)
            && selection.select_unit(context.world, unit_id)
        {
            if unit_type == SelectType::SingleUnit {
                break;
            }
            first.get_or_insert((unit_type, key));
        }
    }
    for squad in saved {
        selection.select_squad(context.world, squad);
    }
    context.rebuild(selection);
}

/// Selects the own army units on the screen (`ScreenSelect`, Q) or on the map
/// (`GlobalSelect`, E), like `BUser_selectAllOnScreenOrGlobal`. The selection is cleared first.
pub(crate) fn select_all(context: &SelectContext<'_>, selection: &mut Selection, global: bool) {
    selection.clear();
    for (unit_id, unit) in context.world.units.iter() {
        let Some(info) = context.catalog.object(&unit.proto_object_name) else {
            continue;
        };
        if context.is_own_army_unit(unit, info)
            && (global || context.in_frustum(unit))
            && unit.base.position.is_finite()
        {
            selection.select_unit(context.world, unit_id);
        }
    }
    context.rebuild(selection);
}

/// Returns `true` if the sphere at `center` with `radius` touches the frustum of
/// `view_projection` (wgpu depth 0 to 1).
pub(crate) fn sphere_in_frustum(view_projection: Mat4, center: Vec3, radius: f32) -> bool {
    let rows = [
        view_projection.row(0),
        view_projection.row(1),
        view_projection.row(2),
        view_projection.row(3),
    ];
    let planes = [
        rows[3] + rows[0],
        rows[3] - rows[0],
        rows[3] + rows[1],
        rows[3] - rows[1],
        rows[2],
        rows[3] - rows[2],
    ];
    planes.iter().all(|plane: &Vec4| {
        let normal = plane.truncate();
        let length = normal.length();
        length > 0.0 && (normal.dot(center) + plane.w) / length >= -radius
    })
}

#[cfg(test)]
mod tests;
