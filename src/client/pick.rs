//! Screen picking of units, like `BSelectionManager_populateSelectionsRect` (`0x14038AFF0`).
//!
//! The pick projects the cursor ground point to the screen and keeps each unit whose pick
//! circle touches a square around that point. `BSelectionManager_sortPossibleSelections`
//! (`0x14038B8C0`) then orders the candidates with `comparePossibleSelections`
//! (`0x14038C5D0`).

use std::cmp::Ordering;

use glam::{Vec2, Vec3};
use num_traits::ToPrimitive;
use render::picking::project_point;
use render::world_view::WorldCamera;
use sim::{EntityId, PlayerId, Unit, World};

use super::catalog::{ClientCatalog, PickPriority, SelectType};

/// Shipped `circleSelectClickSize` of `game.cfg` (code default 20).
///
/// `BUser_updateHover` (`0x140581510`) and `BUser_selectAtCursor` (`0x140584F40`) pass half of
/// this value as the width and the height of the pick square, so the square is 20 pixels.
pub(crate) const CIRCLE_SELECT_CLICK_SIZE: f32 = 40.0;

/// The width and the height of the pick square, in pixels.
pub(crate) const PICK_SQUARE_SIZE: f32 = CIRCLE_SELECT_CLICK_SIZE * 0.5;

/// A projected pick circle with a center farther than this part of the projected radius from
/// the square center is an edge candidate.
const EDGE_RADIUS_FRACTION: f32 = 0.75;

/// One unit under the pick square.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PickCandidate {
    /// The unit.
    pub(crate) unit: EntityId,
    /// The proto `PickPriority`.
    pub(crate) priority: PickPriority,
    /// The squared distance from the camera to the pick center.
    pub(crate) distance_squared: f32,
    /// If true, the square center is near the edge of the projected pick circle.
    pub(crate) edge: bool,
}

/// The camera and the pick square of one pick.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PickView<'a> {
    /// The camera of the frame.
    pub(crate) camera: &'a WorldCamera,
    /// The team of the local player. Only units that this team sees are candidates.
    pub(crate) team: u8,
    /// The pick point, which is the cursor ground point.
    pub(crate) point: Vec3,
    /// The width and the height of the square, in pixels.
    pub(crate) size: f32,
}

/// Returns the sorted candidates under the pick square.
///
/// A candidate is a live unit that the team sees, with a select type that is not None, and not
/// Target unless `include_target`. The pick circle has the proto `PickRadius` (or the
/// obstruction radius) around the unit position raised by `PickOffset` (or by the obstruction
/// half height). The projected circle center must be on the screen, and the circle must touch
/// the square.
pub(crate) fn pick_candidates(
    world: &World,
    catalog: &ClientCatalog,
    view: &PickView<'_>,
    include_target: bool,
) -> Vec<PickCandidate> {
    let mut candidates = Vec::new();
    let Some(square_center) = project_point(view.camera, view.point) else {
        return candidates;
    };
    let viewport = Vec2::new(
        view.camera.viewport[0].to_f32().unwrap_or(0.0),
        view.camera.viewport[1].to_f32().unwrap_or(0.0),
    );
    let camera_right = view.camera.world_to_view.row(0).truncate();
    for (unit_id, unit) in world.units.iter() {
        if !unit.base.is_alive() || !unit.base.is_selectable() {
            continue;
        }
        let Some(info) = catalog.object(&unit.proto_object_name) else {
            continue;
        };
        if info.select_type == SelectType::None
            || (info.select_type == SelectType::Target && !include_target)
        {
            continue;
        }
        if !world.is_entity_visible_to_team(view.team, unit_id) {
            continue;
        }
        let (center, radius) = pick_circle(unit, info.pick_radius, info.pick_offset);
        let Some(screen) = project_point(view.camera, center) else {
            continue;
        };
        if !(0.0..=viewport.x).contains(&screen.x) || !(0.0..=viewport.y).contains(&screen.y) {
            continue;
        }
        let Some(rim) = project_point(view.camera, center + camera_right * radius) else {
            continue;
        };
        let Some(edge) = square_contact(square_center, view.size, screen, screen.distance(rim))
        else {
            continue;
        };
        candidates.push(PickCandidate {
            unit: unit_id,
            priority: info.pick_priority,
            distance_squared: view.camera.position.distance_squared(center),
            edge,
        });
    }
    sort_candidates(&mut candidates);
    candidates
}

/// Returns the pick center and radius of `unit`.
///
/// Retail uses the unit position plus `PickOffset` along the unit up direction, or the bounding
/// center (`unit+208`) when the offset is 0. The simulation has no bounding center, so the
/// client uses the position raised by the obstruction half height. The radius is `PickRadius`,
/// or the obstruction radius (`unit+568`) when it is 0; the client uses the larger horizontal
/// obstruction half extent.
fn pick_circle(unit: &Unit, pick_radius: f32, pick_offset: f32) -> (Vec3, f32) {
    let extents = unit.obstruction_half_extents;
    let height = if pick_offset == 0.0 {
        extents.y
    } else {
        pick_offset
    };
    let radius = if pick_radius == 0.0 {
        extents.x.max(extents.z)
    } else {
        pick_radius
    };
    (unit.base.position + Vec3::Y * height, radius)
}

/// Tests a projected pick circle against the pick square.
///
/// Returns `None` if the circle at `screen` with `radius` pixels does not touch the square of
/// `size` pixels around `square_center`. Otherwise returns the edge flag: true if the square
/// center is at least `0.75 * radius` from the circle center.
pub(crate) fn square_contact(
    square_center: Vec2,
    size: f32,
    screen: Vec2,
    radius: f32,
) -> Option<bool> {
    let half = size * 0.5;
    let min = square_center - Vec2::splat(half);
    let max = square_center + Vec2::splat(half);
    let outside = min.x > screen.x + radius
        || screen.x - radius > max.x
        || min.y > screen.y + radius
        || screen.y - radius > max.y;
    (!outside).then(|| radius * EDGE_RADIUS_FRACTION <= square_center.distance(screen))
}

/// Returns the retail weight of a candidate: Building 2, Resource 3, Unit 9, other 1, minus 1
/// for an edge candidate; Rally 20 without the edge rule.
fn candidate_weight(candidate: &PickCandidate) -> i32 {
    let edge = i32::from(candidate.edge);
    match candidate.priority {
        PickPriority::Building => 2 - edge,
        PickPriority::Resource => 3 - edge,
        PickPriority::Unit => 9 - edge,
        PickPriority::Rally => 20,
        PickPriority::None => 1,
    }
}

/// Sorts the candidates: higher weight first, then the nearer candidate first.
pub(crate) fn sort_candidates(candidates: &mut [PickCandidate]) {
    candidates.sort_by(|a, b| {
        candidate_weight(b).cmp(&candidate_weight(a)).then_with(|| {
            a.distance_squared
                .partial_cmp(&b.distance_squared)
                .unwrap_or(Ordering::Equal)
        })
    });
}

/// Returns `true` if `player` may pick `unit` as the first candidate of a click
/// (`BUser_selectAtCursor`): the select type is not None or Target, and a `MustOwnToSelect`
/// object belongs to the player.
pub(crate) fn clickable(
    world: &World,
    catalog: &ClientCatalog,
    player: PlayerId,
    unit: EntityId,
) -> bool {
    let Some(entity) = world.units.get(unit) else {
        return false;
    };
    catalog
        .object(&entity.proto_object_name)
        .is_some_and(|info| {
            !matches!(info.select_type, SelectType::None | SelectType::Target)
                && (!info.must_own_to_select || entity.base.player_id == player)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(index: u16, priority: PickPriority, distance: f32, edge: bool) -> PickCandidate {
        PickCandidate {
            unit: EntityId::new(sim::EntityClass::Object, u32::from(index)),
            priority,
            distance_squared: distance,
            edge,
        }
    }

    #[test]
    fn units_beat_buildings_and_edges_lose_one_step() {
        let mut candidates = vec![
            candidate(1, PickPriority::Building, 1.0, false),
            candidate(2, PickPriority::Unit, 50.0, true),
            candidate(3, PickPriority::Unit, 40.0, false),
            candidate(4, PickPriority::Resource, 2.0, false),
            candidate(5, PickPriority::Rally, 90.0, true),
            candidate(6, PickPriority::Unit, 10.0, false),
            candidate(7, PickPriority::None, 0.5, true),
        ];
        sort_candidates(&mut candidates);
        let order: Vec<u16> = candidates
            .iter()
            .map(|candidate| candidate.unit.pool_index())
            .collect();
        // Rally 20, Unit 9 (near, then far), Unit edge 8, Resource 3, Building 2, other 1.
        assert_eq!(order, [5, 6, 3, 2, 4, 1, 7]);
    }

    #[test]
    fn the_circle_must_touch_the_square_and_far_centers_are_edges() {
        let center = Vec2::new(100.0, 100.0);
        // The circle center is in the square center: not an edge.
        assert_eq!(square_contact(center, 20.0, center, 8.0), Some(false));
        // 15 pixels to the right with a radius of 6: touches the square side at 110.
        assert_eq!(
            square_contact(center, 20.0, Vec2::new(115.0, 100.0), 6.0),
            Some(true)
        );
        // 17 pixels to the right with a radius of 6: misses the square.
        assert_eq!(
            square_contact(center, 20.0, Vec2::new(117.0, 100.0), 6.0),
            None
        );
        // 5 pixels away with a radius of 8: 6 = 0.75 * 8 is more than 5, so not an edge.
        assert_eq!(
            square_contact(center, 20.0, Vec2::new(105.0, 100.0), 8.0),
            Some(false)
        );
    }
}
