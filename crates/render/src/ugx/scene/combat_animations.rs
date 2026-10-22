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
    pub(super) auxiliary_combat_animations: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CombatAnimation<'animation> {
    pub(super) animation_type: &'animation str,
    pub(super) asset_path: &'animation str,
    pub(super) duration_seconds: f32,
    pub(super) revision: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AuxiliaryCombatAnimation<'animation> {
    pub(super) action_name: &'animation str,
    pub(super) component: &'animation str,
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

pub(super) fn active_auxiliary_combat_animations<'animation>(
    world: &'animation SimWorld,
    gameplay: Option<&'animation GameplayCatalog>,
    entity_id: EntityId,
) -> Vec<AuxiliaryCombatAnimation<'animation>> {
    let Some(unit) = world.get_unit(entity_id) else {
        return Vec::new();
    };
    let Some(object) = gameplay.and_then(|gameplay| gameplay.object(&unit.proto_object_name))
    else {
        return Vec::new();
    };
    unit.turret_combat
        .attacks()
        .filter_map(|state| {
            let profile = object.attack_profile(state.action_name())?;
            let hardpoint = profile.hardpoint.as_ref()?;
            let component = match state.kind() {
                sim::TurretAttackKind::Secondary => hardpoint.yaw_attachment()?,
                sim::TurretAttackKind::Slave => hardpoint.pitch_attachment()?,
            };
            let index = state.animation_index()?;
            let animation = profile.animations.get(index)?;
            state.animation_position(animation.duration)?;
            Some(AuxiliaryCombatAnimation {
                action_name: state.action_name(),
                component,
                animation_type: &profile.animation_type,
                asset_path: &animation.asset_path,
                duration_seconds: animation.duration,
                revision: u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
            })
        })
        .collect()
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
        let auxiliary_combat_animations =
            active_auxiliary_combat_animations(world, gameplay, visual.id)
                .into_iter()
                .map(auxiliary_animation_key)
                .collect();
        SimulationEntityState {
            id: visual.id,
            animation_revision: visual.animation_revision,
            visual_variation_index: visual.visual_variation_index,
            visual_mesh_revision: visual
                .visual_mesh_mask
                .map_or(0, sim::UnitVisualMeshMask::revision),
            combat_animation,
            auxiliary_combat_animations,
        }
    })
}

fn auxiliary_animation_key(animation: AuxiliaryCombatAnimation<'_>) -> String {
    format!(
        "{}\0{}\0{}\0{}\0{}",
        animation.action_name.to_ascii_lowercase(),
        animation.component.to_ascii_lowercase(),
        animation.animation_type.to_ascii_lowercase(),
        animation.asset_path.to_ascii_lowercase(),
        animation.revision,
    )
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
