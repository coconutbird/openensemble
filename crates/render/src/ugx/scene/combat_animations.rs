//! Read-only projection of simulation-owned combat animation state.

use sim::{EntityId, GameplayCatalog, Unit as SimUnit, World as SimWorld};

use super::simulation_visuals;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SimulationEntityState {
    pub(super) id: EntityId,
    pub(super) animation_revision: u32,
    pub(super) visual_variation_index: Option<usize>,
    pub(super) visual_mesh_revision: u32,
    pub(super) combat_animation: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CombatAnimation<'animation> {
    pub(super) animation_type: &'animation str,
    pub(super) asset_path: &'animation str,
    pub(super) duration_seconds: f32,
    pub(super) revision: u32,
}

pub(super) fn active_combat_animation<'animation>(
    world: &'animation SimWorld,
    gameplay: Option<&'animation GameplayCatalog>,
    entity_id: EntityId,
) -> Option<CombatAnimation<'animation>> {
    let unit = world.get_unit(entity_id)?;
    active_unit_combat_animation(unit, gameplay?)
}

pub(super) fn active_combat_animation_key(
    world: &SimWorld,
    gameplay: Option<&GameplayCatalog>,
    entity_id: EntityId,
) -> Option<String> {
    let animation = active_combat_animation(world, gameplay, entity_id)?;
    Some(format!(
        "{}\0{}\0{}",
        animation.animation_type.to_ascii_lowercase(),
        animation.asset_path.to_ascii_lowercase(),
        animation.revision
    ))
}

pub(super) fn simulation_entity_states<'world>(
    world: &'world SimWorld,
    gameplay: Option<&'world GameplayCatalog>,
) -> impl Iterator<Item = SimulationEntityState> + 'world {
    simulation_visuals(world).map(move |visual| {
        let combat_animation = visual
            .animation_type
            .is_none()
            .then(|| active_combat_animation_key(world, gameplay, visual.id))
            .flatten();
        SimulationEntityState {
            id: visual.id,
            animation_revision: visual.animation_revision,
            visual_variation_index: visual.visual_variation_index,
            visual_mesh_revision: visual
                .visual_mesh_mask
                .map_or(0, sim::UnitVisualMeshMask::revision),
            combat_animation,
        }
    })
}

pub(super) fn combat_animation_position(
    world: &SimWorld,
    entity_id: EntityId,
    duration_seconds: f32,
) -> Option<f32> {
    world
        .get_unit(entity_id)?
        .combat
        .animation_position(duration_seconds)
}

fn active_unit_combat_animation<'animation>(
    unit: &'animation SimUnit,
    gameplay: &'animation GameplayCatalog,
) -> Option<CombatAnimation<'animation>> {
    let action_name = unit.combat.action_name()?;
    let profile = gameplay
        .object(&unit.proto_object_name)?
        .attack_profile(action_name)?;
    let charged = unit.combat.uses_charged_animation();
    let index = unit.combat.animation_index()?;
    let animation = profile.cycle_animations(charged).get(index)?;
    unit.combat.animation_position(animation.duration)?;
    Some(CombatAnimation {
        animation_type: profile.cycle_animation_type(charged),
        asset_path: &animation.asset_path,
        duration_seconds: animation.duration,
        revision: u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
    })
}
