//! Per-frame placement state: sim transforms, selection overlays and animation samples.

use glam::{Mat4, Vec3};
use num_traits::ToPrimitive;
use sim::{TargetingSelection, TeamId, World as SimWorld};

use super::super::{
    combat_animations::combat_animation_position, movement_animations::MovementAnimation,
    simulation_entity_animation, simulation_entity_transform, simulation_entity_visible_to_team,
    simulation_entity_visual_opacity,
};
use super::{RenderedPlacement, simulation_entity_secondary_transform};
use crate::terrain::LightingParams;
use crate::ugx::renderer::{SelectionOverlay, VisualState};
use crate::ugx::unit::{UnitAnimationEvent, UnitAnimationFrame};
use crate::wgpu;

pub(super) fn update_placement_states(
    placements: &mut [RenderedPlacement],
    queue: &wgpu::Queue,
    world: &SimWorld,
    viewer_team: Option<TeamId>,
    time_seconds: f32,
) -> Vec<UnitAnimationEvent> {
    let mut animation_events = Vec::new();
    for placement in placements {
        let Some(transform) = simulation_entity_transform(world, placement.entity_id) else {
            placement.visible = false;
            continue;
        };
        placement.transform = transform;
        placement.secondary_transform =
            simulation_entity_secondary_transform(world, placement.entity_id);
        placement.visual_opacity =
            simulation_entity_visual_opacity(world, placement.entity_id).unwrap_or(1.0);
        placement.selection = world
            .entity_targeting_selection(placement.entity_id)
            .map_or_else(SelectionOverlay::default, |selection| {
                project_selection(
                    world,
                    selection,
                    transform,
                    placement.visual_bounds_min,
                    placement.visual_bounds_max,
                )
            });
        placement.visible = viewer_team.is_none_or(|team_id| {
            simulation_entity_visible_to_team(world, team_id, placement.entity_id)
        });
        let animation_position = placement.combat_animation_duration.map_or_else(
            || {
                simulation_entity_animation(world, placement.entity_id)
                    .map(|animation| animation.normalized_position(world.game_time()))
            },
            |duration| combat_animation_position(world, placement.entity_id, duration),
        );
        let action_duration = placement.renderer.action_presentation_animation_duration();
        let movement_on_action_track = placement.movement_animation.is_some_and(|movement| {
            placement.movement_track_animation.is_none()
                && placement
                    .animation_type
                    .as_deref()
                    .is_some_and(|animation| animation.eq_ignore_ascii_case(movement.as_str()))
        });
        let movement_duration = if movement_on_action_track {
            action_duration
        } else {
            placement
                .renderer
                .movement_presentation_animation_duration()
        };
        let action_presentation_phase = placement
            .animation_clock
            .sample(time_seconds, action_duration);
        let movement_presentation_phase = placement
            .movement_animation_clock
            .sample(time_seconds, movement_duration);
        let movement_animation = placement
            .movement_track_animation
            .as_deref()
            .or_else(|| placement.movement_animation.map(MovementAnimation::as_str));
        let events = placement.renderer.update_animation(
            queue,
            UnitAnimationFrame {
                simulation_position: animation_position,
                action_presentation_phase,
                movement_presentation_phase,
                movement_on_action_track,
                unit_transform: placement.transform,
                source_owner_id: u64::from(placement.entity_id.as_u32()),
                world,
                entity_id: placement.entity_id,
                movement_animation,
            },
        );
        if placement.visible {
            animation_events.extend(events);
        }
    }
    animation_events
}

pub(super) fn update_renderers(
    placements: &mut [RenderedPlacement],
    queue: &wgpu::Queue,
    view_projection: Mat4,
    lighting: &LightingParams,
    time_seconds: f32,
) {
    for placement in placements.iter_mut().filter(|placement| placement.visible) {
        placement.renderer.update_frame_with_visual_state_at_time(
            queue,
            view_projection,
            placement.transform,
            lighting,
            time_seconds,
            VisualState::new(placement.selection, placement.visual_opacity),
        );
    }
}

fn project_selection(
    world: &SimWorld,
    selection: TargetingSelection,
    transform: Mat4,
    bounds_min: Vec3,
    bounds_max: Vec3,
) -> SelectionOverlay {
    let (minimum_y, maximum_y) = transformed_y_bounds(transform, bounds_min, bounds_max);
    let extent = maximum_y - minimum_y;
    let scale = if extent.abs() > f32::EPSILON {
        -extent.recip()
    } else {
        1.0
    };
    let base_offset = 2.0 - minimum_y * scale;
    let elapsed_seconds = world
        .game_time()
        .wrapping_sub(selection.started_at_ms())
        .to_f32()
        .unwrap_or(f32::MAX)
        * 0.001;
    let raw_scroll = elapsed_seconds * selection.scroll_speed();
    let scroll = if raw_scroll < -4.0 {
        -((-raw_scroll) % 4.0)
    } else {
        raw_scroll
    };
    let [r, g, b, a] = selection.color();
    SelectionOverlay::new(
        [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            f32::from(a) / 255.0,
        ],
        scale,
        base_offset + scroll,
        selection.intensity(),
    )
}

fn transformed_y_bounds(transform: Mat4, minimum: Vec3, maximum: Vec3) -> (f32, f32) {
    let mut minimum_y = f32::INFINITY;
    let mut maximum_y = f32::NEG_INFINITY;
    for x in [minimum.x, maximum.x] {
        for y in [minimum.y, maximum.y] {
            for z in [minimum.z, maximum.z] {
                let world_y = transform.transform_point3(Vec3::new(x, y, z)).y;
                minimum_y = minimum_y.min(world_y);
                maximum_y = maximum_y.max(world_y);
            }
        }
    }
    if minimum_y.is_finite() && maximum_y.is_finite() {
        (minimum_y, maximum_y)
    } else {
        (0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{Mat4, Vec3, project_selection};

    #[test]
    fn selection_projection_uses_sim_time_and_decoded_world_bounds() {
        let mut world = sim::World::new();
        let unit_id = world.create_unit(1);
        assert!(world.flash_entity(unit_id, 500, 3_000, [255, 255, 0, 255], 80.0));
        world.game_time_ms = 500;
        let selection = world.entity_targeting_selection(unit_id).unwrap();
        let overlay = project_selection(
            &world,
            selection,
            Mat4::from_translation(Vec3::new(0.0, 10.0, 0.0)),
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        );

        assert_eq!(
            overlay.color().map(f32::to_bits),
            [1.0, 1.0, 0.0, 1.0].map(f32::to_bits)
        );
        let params = overlay.params();
        assert_eq!(params[0].to_bits(), (-0.5_f32).to_bits());
        assert_eq!(params[1].to_bits(), 4.5_f32.to_bits());
        assert_eq!(params[2].to_bits(), 80.0_f32.to_bits());
        assert_eq!(params[3].to_bits(), 1.0_f32.to_bits());
    }
}
