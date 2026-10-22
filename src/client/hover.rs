//! The hover of the cursor, like `BUser_updateHover` (`0x140581510`).
//!
//! Each frame the client finds the ground point under the cursor, the units under the pick
//! square, and the hover type that a right click would use.

use glam::{Vec2, Vec3};
use render::picking::{intersect_terrain, screen_ray};
use render::world_view::WorldCamera;
use sim::{DefaultWorkAction, EntityId, GameplayCatalog, PlayerId, World};

use super::catalog::ClientCatalog;
use super::pick::{PICK_SQUARE_SIZE, PickCandidate, PickView, clickable, pick_candidates};

/// The longest cursor ray to the terrain, in world units.
///
/// This value is not a retail value. It covers the view of the largest zoom.
const MAX_GROUND_DISTANCE: f32 = 2000.0;

/// The hover type of `BUser_classifyHoverTarget` (`0x1405820C0`), stored at `user+484`.
///
/// Retail also has 9 (an ability applies) and 10 (another tactic action applies). The client
/// does not model abilities, so these types do not occur.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HoverType {
    /// 0: the cursor is over the ground.
    #[default]
    Ground = 0,
    /// 1: the cursor is over a selectable object and no action applies.
    Selectable = 1,
    /// 2: a selected squad gathers from the object.
    Gather = 2,
    /// 4: a selected squad captures the object.
    Capture = 4,
    /// 5: a selected squad repairs the object.
    RepairOther = 5,
    /// 6: a selected squad garrisons in or joins the object.
    Garrison = 6,
    /// 7: a selected squad hitches the object.
    Hitch = 7,
    /// 8: a selected squad attacks the object. This type stops the search.
    Attack = 8,
}

impl HoverType {
    /// Returns the hover type of a tactic action.
    pub(crate) fn from_action(action: DefaultWorkAction) -> Self {
        match action {
            DefaultWorkAction::Attack => Self::Attack,
            DefaultWorkAction::Garrison | DefaultWorkAction::Join => Self::Garrison,
            DefaultWorkAction::Capture => Self::Capture,
            DefaultWorkAction::Gather => Self::Gather,
            DefaultWorkAction::Hitch => Self::Hitch,
            DefaultWorkAction::RepairOther => Self::RepairOther,
        }
    }

    /// Returns `true` if a right click sends the hover entity as the target (type above 1).
    pub(crate) fn has_target(self) -> bool {
        self > Self::Selectable
    }
}

/// The hover state of one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Hover {
    /// The cursor point on the terrain (`user+448`), if the cursor ray hits the terrain.
    pub(crate) point: Option<Vec3>,
    /// The hovered unit (`user+480`).
    pub(crate) entity: Option<EntityId>,
    /// The hover type (`user+484`).
    pub(crate) kind: HoverType,
    /// The sorted pick candidates of the frame.
    pub(crate) candidates: Vec<PickCandidate>,
}

/// The world and the player of a hover update.
#[derive(Clone, Copy)]
pub(crate) struct HoverContext<'a> {
    pub(crate) world: &'a World,
    pub(crate) gameplay: &'a GameplayCatalog,
    pub(crate) catalog: &'a ClientCatalog,
    pub(crate) camera: &'a WorldCamera,
    /// The local player.
    pub(crate) player: PlayerId,
    /// The team of the local player.
    pub(crate) team: u8,
}

/// Returns the terrain point under the screen position `cursor`.
pub(crate) fn ground_point(world: &World, camera: &WorldCamera, cursor: Vec2) -> Option<Vec3> {
    let ray = screen_ray(camera, cursor)?;
    intersect_terrain(&ray, MAX_GROUND_DISTANCE, |point| {
        world.terrain_height(point, true)
    })
}

/// Updates the hover for the cursor at `cursor` and the selected squads `squads`.
///
/// Target objects are candidates only when a squad is selected. The hover entity is the first
/// candidate that a tactic action applies to, or else the first candidate as type 1 if a
/// click can select it.
pub(crate) fn update_hover(
    context: &HoverContext<'_>,
    cursor: Vec2,
    squads: &[EntityId],
    hover: &mut Hover,
) {
    hover.point = ground_point(context.world, context.camera, cursor);
    hover.entity = None;
    hover.kind = HoverType::Ground;
    hover.candidates.clear();
    let Some(point) = hover.point else {
        return;
    };
    let view = PickView {
        camera: context.camera,
        team: context.team,
        point,
        size: PICK_SQUARE_SIZE,
    };
    hover.candidates = pick_candidates(context.world, context.catalog, &view, !squads.is_empty());
    let (entity, kind) = resolve_hover(
        squads,
        &hover.candidates,
        |squad, unit| {
            sim::default_work_action(context.world, context.gameplay, squad, unit)
                .map(HoverType::from_action)
        },
        |unit| clickable(context.world, context.catalog, context.player, unit),
    );
    hover.entity = entity;
    hover.kind = kind;
}

/// Picks the hover entity and type from the sorted `candidates`.
///
/// The loop follows `BUser_updateHover`: for each selected squad, each candidate gets the type
/// of `classify`. An Attack type ends the search. Otherwise the last classified candidate of
/// the last squad stays, or the first classified candidate if the last squad has none. With no
/// classified candidate, the first candidate is the hover entity with type 1 if `clickable`
/// accepts it.
pub(crate) fn resolve_hover(
    squads: &[EntityId],
    candidates: &[PickCandidate],
    mut classify: impl FnMut(EntityId, EntityId) -> Option<HoverType>,
    clickable: impl Fn(EntityId) -> bool,
) -> (Option<EntityId>, HoverType) {
    let mut first = None;
    let mut current = None;
    'squads: for &squad in squads {
        current = None;
        for candidate in candidates {
            let Some(kind) = classify(squad, candidate.unit) else {
                continue;
            };
            current = Some((candidate.unit, kind));
            first.get_or_insert((candidate.unit, kind));
            if kind == HoverType::Attack {
                break 'squads;
            }
        }
    }
    if let Some((entity, kind)) = current.or(first) {
        return (Some(entity), kind);
    }
    match candidates.first() {
        Some(candidate) if clickable(candidate.unit) => {
            (Some(candidate.unit), HoverType::Selectable)
        }
        _ => (None, HoverType::Ground),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::catalog::PickPriority;

    fn id(index: u32) -> EntityId {
        EntityId::new(sim::EntityClass::Object, index)
    }

    fn candidates(units: &[u32]) -> Vec<PickCandidate> {
        units
            .iter()
            .map(|&unit| PickCandidate {
                unit: id(unit),
                priority: PickPriority::Unit,
                distance_squared: 1.0,
                edge: false,
            })
            .collect()
    }

    #[test]
    fn tactic_actions_map_to_the_retail_hover_types() {
        assert_eq!(
            HoverType::from_action(DefaultWorkAction::Attack),
            HoverType::Attack
        );
        assert_eq!(
            HoverType::from_action(DefaultWorkAction::Join),
            HoverType::Garrison
        );
        assert_eq!(
            HoverType::from_action(DefaultWorkAction::Gather),
            HoverType::Gather
        );
        assert!(HoverType::Gather.has_target());
        assert!(!HoverType::Selectable.has_target());
        assert!(!HoverType::Ground.has_target());
    }

    #[test]
    fn attack_stops_the_search_and_wins() {
        let list = candidates(&[10, 11]);
        let classify = |_squad: EntityId, unit: EntityId| {
            if unit == id(11) {
                Some(HoverType::Attack)
            } else {
                Some(HoverType::Gather)
            }
        };
        assert_eq!(
            resolve_hover(&[id(1), id(2)], &list, classify, |_| true),
            (Some(id(11)), HoverType::Attack)
        );
    }

    #[test]
    fn without_an_action_the_first_clickable_candidate_is_type_one() {
        let list = candidates(&[10, 11]);
        let none = |_: EntityId, _: EntityId| None;
        assert_eq!(
            resolve_hover(&[id(1)], &list, none, |_| true),
            (Some(id(10)), HoverType::Selectable)
        );
        assert_eq!(
            resolve_hover(&[], &list, none, |unit| unit != id(10)),
            (None, HoverType::Ground)
        );
        assert_eq!(
            resolve_hover(&[id(1)], &[], none, |_| true),
            (None, HoverType::Ground)
        );
    }

    #[test]
    fn the_last_squad_keeps_its_last_classified_candidate() {
        let list = candidates(&[10, 11, 12]);
        // Squad 1 classifies 10; squad 2 classifies 11 and 12 (the last one stays).
        let classify = |squad: EntityId, unit: EntityId| match (squad == id(1), unit) {
            (true, unit) if unit == id(10) => Some(HoverType::Capture),
            (false, unit) if unit != id(10) => Some(HoverType::Garrison),
            _ => None,
        };
        assert_eq!(
            resolve_hover(&[id(1), id(2)], &list, classify, |_| true),
            (Some(id(12)), HoverType::Garrison)
        );
        // When the last squad classifies nothing, the first classified candidate stays.
        let classify_first =
            |squad: EntityId, _unit: EntityId| (squad == id(1)).then_some(HoverType::Capture);
        assert_eq!(
            resolve_hover(&[id(1), id(2)], &list, classify_first, |_| true),
            (Some(id(10)), HoverType::Capture)
        );
    }
}
