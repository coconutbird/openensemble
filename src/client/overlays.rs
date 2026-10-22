//! The selection decals and the HP bars of a frame.
//!
//! - Decals: `BUser_renderSelectionAndHover` (`0x1405872D0`) and `BSquad_drawSelectionDecal`
//!   (`0x1403ABEA0`). The intensity comes from the caller: `BaseDecalIntensity` or
//!   `SelectionDecalIntensity`. The decal list icon intensities do not change it: only
//!   `BWorld_drawPlacementDecal` (`0x1405CE810`) reads an icon intensity.
//! - Bars: `BSquad_renderUIOverlays` (`0x140398330`), `BHPBar_renderSquadHPBar`
//!   (`0x140044080`) and `BHPBar_renderUnitHPBar` (`0x140043530`).

use glam::Vec3;
use render::hp_bars::{HpBarInstance, HpBarStyleId};
use render::selection_decals::SelectionDecal;
use sim::{EntityId, PlayerId, Squad, World};

use super::bar_ratios::{SquadBarRatios, squad_bar_ratios, unit_bar_ratios};
use super::catalog::ClientCatalog;
use super::selection::Selection;

/// Shipped `BaseDecalIntensity`: the decal of a selected squad.
pub(crate) const BASE_DECAL_INTENSITY: f32 = 6.0;
/// Shipped `SelectionDecalIntensity`: the decal of the hovered squad.
pub(crate) const SELECTION_DECAL_INTENSITY: f32 = 15.0;
/// A squad shows its bar for this time after damage, in game milliseconds.
const RECENT_DAMAGE_MS: u32 = 6000;
/// A protosquad bar size of this value or less selects the style size.
const MINIMUM_BAR_SIZE: f32 = 0.000_001;

/// The local player, the selection and the hover of a frame.
#[derive(Clone, Copy)]
pub(crate) struct OverlayContext<'a> {
    pub(crate) world: &'a World,
    pub(crate) catalog: &'a ClientCatalog,
    pub(crate) selection: &'a Selection,
    /// The hovered unit.
    pub(crate) hovered: Option<EntityId>,
    /// The team of the local player. Only squads that this team sees get overlays.
    pub(crate) team: u8,
    /// The retail profile option `AlwaysShowHitpointBars` (`BUser_setAlwaysShowHitpointBars`,
    /// `0x1405AA1D0`). The PC game sets it in the options menu only; the client keeps it off.
    pub(crate) always_show_bars: bool,
}

/// Fills `decals` and `bars` with the overlays of the frame.
///
/// `style` returns the renderer style of an `hpbars.xml` style name.
pub(crate) fn build_overlays(
    context: &OverlayContext<'_>,
    style: impl Fn(&str) -> Option<HpBarStyleId>,
    decals: &mut Vec<SelectionDecal>,
    bars: &mut Vec<HpBarInstance>,
) {
    decals.clear();
    bars.clear();
    let world = context.world;
    let hovered_squad = context
        .hovered
        .and_then(|unit| world.units.get(unit))
        .and_then(|unit| unit.squad_id);
    for (squad_id, squad) in world.squads.iter() {
        if !squad.base.is_alive() || !world.is_entity_visible_to_team(context.team, squad_id) {
            continue;
        }
        let hovered = hovered_squad == Some(squad_id);
        let info = context.catalog.squad(&squad.proto_squad_name);
        // One decal slot per squad (`squad+604`): the hover decal replaces the selected one.
        let intensity = if hovered {
            Some(SELECTION_DECAL_INTENSITY)
        } else if context.selection.contains_squad(squad_id)
            || info.is_some_and(|info| info.always_render_selection_decal)
        {
            Some(BASE_DECAL_INTENSITY)
        } else {
            None
        };
        if let Some(intensity) = intensity {
            decals.extend(squad_decal(context, squad, intensity));
        }
        let recent_damage = squad.last_damaged_time != 0
            && world.game_time_ms.saturating_sub(squad.last_damaged_time) <= RECENT_DAMAGE_MS;
        let show = context.always_show_bars || recent_damage;
        if info.is_some() {
            let forced = hovered || info.is_some_and(|info| info.always_show_hp_bar);
            if show || forced {
                bars.extend(squad_bar(context, squad, &style));
            }
        } else if show || hovered {
            unit_bars(context, squad, &style, bars);
        }
    }
}

/// Returns the selection colour of the owner of `squad`, if the player has one.
fn owner_color(context: &OverlayContext<'_>, player: PlayerId) -> Option<[u8; 3]> {
    let color = context.world.get_player(player)?.color;
    context.catalog.selection_color(color)
}

/// Returns the position of the squad: the one member position, or the member average.
fn member_center(world: &World, squad: &Squad) -> Option<Vec3> {
    let mut sum = Vec3::ZERO;
    let mut count = 0_u16;
    for unit in squad
        .unit_ids
        .iter()
        .filter_map(|&unit_id| world.units.get(unit_id))
        .filter(|unit| unit.base.is_alive())
    {
        sum += unit.base.position;
        count = count.saturating_add(1);
    }
    (count > 0).then(|| sum / f32::from(count))
}

/// Makes the decal of `squad`, like `BSquad_drawSelectionDecal`: the catalog decal shape (the
/// decal list icon or the protosquad `<Selection>`), in the owner selection colour with full
/// opacity. The icon `Offset` moves the center along the squad right, up and forward axes. A
/// positive `ZOffset` then moves it along the forward direction. The size and `YOffset` apply
/// after the terrain conform.
///
/// Sub-select groups (opacity `SubSelectDecalAlpha` 0.4) are not modelled.
fn squad_decal(
    context: &OverlayContext<'_>,
    squad: &Squad,
    intensity: f32,
) -> Option<SelectionDecal> {
    let shape = context.catalog.squad(&squad.proto_squad_name)?.decal;
    let color = owner_color(context, squad.base.player_id)?;
    let center = member_center(context.world, squad)?;
    let forward = squad.base.forward;
    let horizontal = Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .unwrap_or(Vec3::Z);
    let right = Vec3::Y.cross(horizontal);
    let mut center =
        center + right * shape.offset.x + Vec3::Y * shape.offset.y + horizontal * shape.offset.z;
    if shape.z_offset > 0.0 {
        center += horizontal * shape.z_offset;
    }
    Some(SelectionDecal {
        center,
        forward,
        radius_x: shape.radius_x,
        radius_z: shape.radius_z,
        y_offset: shape.y_offset,
        color: [
            f32::from(color[0]) / 255.0,
            f32::from(color[1]) / 255.0,
            f32::from(color[2]) / 255.0,
            1.0,
        ],
        intensity,
    })
}

/// Makes the squad bar of `squad`, like `BHPBar_renderSquadHPBar`: the member average plus the
/// protosquad bar offset, the protosquad size or the style size, and the protosquad ratios.
fn squad_bar(
    context: &OverlayContext<'_>,
    squad: &Squad,
    style: &impl Fn(&str) -> Option<HpBarStyleId>,
) -> Option<HpBarInstance> {
    let bar = context
        .catalog
        .squad(&squad.proto_squad_name)?
        .hp_bar
        .as_ref()?;
    let size = if bar.size[0] > MINIMUM_BAR_SIZE && bar.size[1] > MINIMUM_BAR_SIZE {
        bar.size
    } else {
        context.catalog.hp_bar_size(&bar.name)?
    };
    let ratios = squad_bar_ratios(context.world, context.catalog, squad)?;
    Some(bar_instance(
        member_center(context.world, squad)? + bar.offset,
        size,
        style(&bar.name)?,
        ratios,
        owner_color(context, squad.base.player_id)?,
    ))
}

/// Adds the unit bars of a squad without a protosquad, like `BHPBar_renderUnitHPBar`.
///
/// The width is `clamp(maxHP * 0.002, 2, 20)` and the height `clamp(width * 0.3, 0.25,
/// 0.75)`. The bar is twice `ObstructionRadiusY` above the unit position. Retail can also put
/// the bar on an `hpbar` bone of the model; the client does not read bones.
fn unit_bars(
    context: &OverlayContext<'_>,
    squad: &Squad,
    style: &impl Fn(&str) -> Option<HpBarStyleId>,
    bars: &mut Vec<HpBarInstance>,
) {
    let Some(color) = owner_color(context, squad.base.player_id) else {
        return;
    };
    for unit in squad
        .unit_ids
        .iter()
        .filter_map(|&unit_id| context.world.units.get(unit_id))
        .filter(|unit| unit.base.is_alive())
    {
        let Some(info) = context.catalog.object(&unit.proto_object_name) else {
            continue;
        };
        let Some(style_id) = info.unit_hp_bar.as_deref().and_then(style) else {
            continue;
        };
        let (width, height) = unit_bar_size(unit.max_hitpoints);
        let position = unit.base.position + Vec3::Y * (info.obstruction_radius_y * 2.0);
        bars.push(bar_instance(
            position,
            [width, height],
            style_id,
            unit_bar_ratios(unit),
            color,
        ));
    }
}

/// Returns the unit bar size of `BHPBar_getUnitBarPosAndSize` (`0x1400427A0`).
pub(crate) fn unit_bar_size(max_hitpoints: f32) -> (f32, f32) {
    let width = (max_hitpoints * 0.002).clamp(2.0, 20.0);
    (width, (width * 0.3).clamp(0.25, 0.75))
}

fn bar_instance(
    position: Vec3,
    size: [f32; 2],
    style: HpBarStyleId,
    ratios: SquadBarRatios,
    player_color: [u8; 3],
) -> HpBarInstance {
    HpBarInstance {
        position,
        size,
        style,
        hitpoints: ratios.hitpoints,
        shield: Some(ratios.shield),
        ammo: Some(ratios.ammo),
        player_color,
    }
}

#[cfg(test)]
mod tests;
