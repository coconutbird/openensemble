//! Focused helpers for scene roster loading and retained asset reuse.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use pipeline::database::hw1::{ProtoObject, Visual};
use pipeline::source::{AssetSource, StdFileProvider};
use sim::World as SimWorld;

use super::{
    ImpactDecalAssets, ImpactVisualAssets, LightAssets, ParticleAssets, SimulationVisual,
    TerrainEffectAssets, UnitPlacement, UnitScene, UnitSceneIssue,
    combat_animations::active_combat_animation,
    hidden_proto_names, load_cached_unit,
    movement_animations::{MovementAnimation, MovementAnimationProfile},
    simulation_entity_secondary_transform, simulation_visuals, visual_name_lookup,
};
use crate::terrain_effect::TerrainImpactAssets;
use crate::ugx::unit::{UnitAnimationRequest, UnitAssetCache, UnitIkProfile};
use crate::ugx::{Unit, UnitAttachmentKind};

#[derive(Clone, Copy)]
pub(super) struct SceneInputs<'input> {
    pub(super) world: &'input SimWorld,
    pub(super) gameplay: Option<&'input sim::GameplayCatalog>,
    pub(super) visuals: &'input HashMap<String, Visual>,
    pub(super) proto_objects: &'input [ProtoObject],
    pub(super) previous_movement: Option<&'input HashMap<sim::EntityId, MovementAnimation>>,
    pub(super) previous_animation_selections: Option<&'input HashMap<sim::EntityId, (String, u64)>>,
}

#[derive(Default)]
pub(super) struct SceneAssets {
    pub(super) units: HashMap<String, Option<Arc<Unit>>>,
    pub(super) lights: LightAssets,
    pub(super) particles: ParticleAssets,
    pub(super) terrain_effects: TerrainEffectAssets,
    pub(super) terrain_impacts: Option<TerrainImpactAssets>,
    pub(super) terrain_impact_issues: Vec<String>,
    pub(super) impact_decals: ImpactDecalAssets,
    pub(super) impact_visuals: ImpactVisualAssets,
}

#[derive(Clone, Copy)]
struct PlacementAnimation<'animation> {
    animation_type: Option<&'animation str>,
    animation_asset: Option<&'animation str>,
    uses_simulation_clock: bool,
    revision: u32,
    combat_duration: Option<f32>,
    movement: Option<MovementAnimation>,
    movement_track: Option<MovementAnimation>,
}

pub(super) fn load_placements(
    scene: &mut UnitScene,
    source: &mut AssetSource<StdFileProvider>,
    inputs: SceneInputs<'_>,
    units: &mut HashMap<String, Option<Arc<Unit>>>,
) {
    let mut asset_cache = UnitAssetCache::default();
    let visual_names = visual_name_lookup(inputs.visuals);
    let proto_objects = inputs
        .proto_objects
        .iter()
        .map(|proto| (proto.name.to_ascii_lowercase(), proto))
        .collect::<HashMap<_, _>>();
    let no_render_objects = hidden_proto_names(inputs.proto_objects);
    let mut used_visuals = HashSet::new();
    for entity in simulation_visuals(inputs.world) {
        let proto_name = entity.proto_name.trim();
        if proto_name.is_empty() {
            scene.missing_proto_count += 1;
            continue;
        }
        let lookup_name = proto_name.to_ascii_lowercase();
        if no_render_objects.contains(&lookup_name) {
            scene.skipped_no_render_count += 1;
            continue;
        }
        let Some(&visual_name) = visual_names.get(&lookup_name) else {
            scene.missing_visual_count += 1;
            continue;
        };
        let visual = &inputs.visuals[visual_name];
        let Some(transform) = entity.transform else {
            scene.invalid_transform_count += 1;
            continue;
        };
        let proto = proto_objects.get(&lookup_name).copied();
        let movement_profile = inputs
            .world
            .get_unit(entity.id)
            .and(proto)
            .map(MovementAnimationProfile::from_proto);
        let ik_profile = proto.map(UnitIkProfile::from_proto).unwrap_or_default();
        let animation = select_placement_animation(inputs, &entity, movement_profile);
        let selection_identity = animation_selection_identity(
            animation.animation_type,
            animation.animation_asset,
            animation.uses_simulation_clock,
            animation.revision,
        );
        let animation_roll =
            retained_animation_roll(inputs, &entity, animation, &selection_identity);
        let movement_animation_roll = animation
            .movement_track
            .map_or(0, |movement| movement_roll(entity.id, movement));
        scene
            .animation_selections
            .insert(entity.id, (selection_identity, animation_roll));
        let unit = load_cached_unit(
            source,
            visual,
            &lookup_name,
            entity.visual_variation_index,
            UnitAnimationRequest {
                animation_type: animation.animation_type,
                animation_asset: animation.animation_asset,
                uses_simulation_clock: animation.uses_simulation_clock,
                animation_roll,
                movement_animation_type: animation.movement_track.map(MovementAnimation::as_str),
                movement_animation_roll,
            },
            &mut asset_cache,
            units,
        );
        let unit = match unit {
            Ok(unit) => unit,
            Err(reason) => {
                record_load_failure(scene, proto_name, reason);
                continue;
            }
        };
        scene.placements.push(UnitPlacement {
            entity_id: entity.id,
            proto_name: proto_name.to_owned(),
            visual_variation_index: entity.visual_variation_index,
            animation_type: animation.animation_type.map(str::to_owned),
            animation_asset: animation.animation_asset.map(str::to_owned),
            animation_uses_simulation_clock: animation.uses_simulation_clock,
            animation_revision: animation.revision,
            combat_animation_duration: animation.combat_duration,
            movement_animation: animation.movement,
            movement_track_animation: animation.movement_track,
            movement_profile,
            ik_profile,
            visual_mesh_mask: entity.visual_mesh_mask.cloned().unwrap_or_default(),
            visual_opacity: entity.visual_opacity,
            transform,
            secondary_transform: simulation_entity_secondary_transform(inputs.world, entity.id),
            unit,
        });
        used_visuals.insert(lookup_name);
    }
    scene.unique_visual_count = used_visuals.len();
}

fn retained_animation_roll(
    inputs: SceneInputs<'_>,
    entity: &SimulationVisual<'_>,
    animation: PlacementAnimation<'_>,
    selection_identity: &str,
) -> u64 {
    inputs
        .previous_animation_selections
        .and_then(|selections| selections.get(&entity.id))
        .filter(|(identity, _)| identity == selection_identity)
        .map_or_else(
            || {
                animation_roll(
                    entity.id,
                    inputs.world.game_time(),
                    animation.animation_type,
                )
            },
            |(_, roll)| *roll,
        )
}

fn select_placement_animation<'animation>(
    inputs: SceneInputs<'animation>,
    entity: &SimulationVisual<'animation>,
    movement_profile: Option<MovementAnimationProfile>,
) -> PlacementAnimation<'animation> {
    let combat = entity
        .animation_type
        .is_none()
        .then(|| active_combat_animation(inputs.world, inputs.gameplay, entity.id))
        .flatten();
    let authoritative = entity.animation_type.is_some() || combat.is_some();
    let action_animation = entity
        .animation_type
        .or(combat.map(|animation| animation.animation_type));
    let movement = select_movement_animation(
        inputs.world,
        entity.id,
        movement_profile,
        inputs.previous_movement,
    );
    let movement_track = if authoritative {
        movement_profile
            .zip(movement)
            .and_then(|(profile, movement)| {
                inputs
                    .world
                    .get_unit(entity.id)
                    .and_then(|unit| profile.lower_body_track(unit, movement, action_animation))
            })
    } else {
        None
    };
    PlacementAnimation {
        animation_type: action_animation.or_else(|| movement.map(MovementAnimation::as_str)),
        animation_asset: entity
            .animation_asset
            .or(combat.map(|animation| animation.asset_path)),
        uses_simulation_clock: authoritative,
        revision: combat.map_or(entity.animation_revision, |animation| animation.revision),
        combat_duration: combat.map(|animation| animation.duration_seconds),
        movement,
        movement_track,
    }
}

fn select_movement_animation(
    world: &SimWorld,
    entity_id: sim::EntityId,
    profile: Option<MovementAnimationProfile>,
    previous: Option<&HashMap<sim::EntityId, MovementAnimation>>,
) -> Option<MovementAnimation> {
    let unit = world.get_unit(entity_id)?;
    let previous = previous
        .and_then(|animations| animations.get(&entity_id))
        .copied()
        .unwrap_or_default();
    profile.map(|profile| profile.select(unit, previous))
}

fn movement_roll(entity_id: sim::EntityId, movement: MovementAnimation) -> u64 {
    animation_roll(entity_id, 0, Some(movement.as_str())) ^ 0xA076_1D64_78BD_642F
}

fn animation_roll(entity_id: sim::EntityId, game_time_ms: u32, animation: Option<&str>) -> u64 {
    let seed = (u64::from(entity_id.as_u32()) << 32) | u64::from(game_time_ms);
    animation
        .unwrap_or("Idle")
        .bytes()
        .fold(seed, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
        })
}

fn animation_selection_identity(
    animation_type: Option<&str>,
    animation_asset: Option<&str>,
    uses_simulation_clock: bool,
    revision: u32,
) -> String {
    format!(
        "{}\0{}\0{uses_simulation_clock}\0{revision}",
        animation_type.unwrap_or_default().to_ascii_lowercase(),
        animation_asset.unwrap_or_default().to_ascii_lowercase()
    )
}

fn record_load_failure(scene: &mut UnitScene, proto_name: &str, reason: Option<String>) {
    if let Some(reason) = reason {
        scene.issues.push(UnitSceneIssue {
            proto_name: proto_name.to_owned(),
            reason,
        });
    }
    scene.load_failure_count += 1;
}

pub(super) fn load_attachment_assets(
    scene: &mut UnitScene,
    source: &mut AssetSource<StdFileProvider>,
) {
    load_terrain_impact_assets(scene, source);
    if let Some(assets) = &scene.terrain_impact_assets {
        scene
            .impact_visual_assets
            .load_referenced(source, assets.visual_names());
    }
    let mut particle_paths = attachment_paths(scene, UnitAttachmentKind::Particle);
    particle_paths.extend(animation_tag_paths(scene, "Particle"));
    scene
        .terrain_effect_assets
        .load_for_placements(source, &scene.placements);
    scene
        .impact_visual_assets
        .load_referenced(source, scene.terrain_effect_assets.visual_names());
    let impact_terrain_paths =
        impact_visual_attachment_paths(scene, UnitAttachmentKind::TerrainEffect)
            .collect::<Vec<_>>();
    scene
        .terrain_effect_assets
        .load_referenced(source, impact_terrain_paths);
    particle_paths.extend(scene.terrain_effect_assets.particle_paths());
    if let Some(assets) = &scene.terrain_impact_assets {
        particle_paths.extend(assets.particle_paths());
    }
    particle_paths.extend(scene.impact_visual_assets.particle_paths());
    scene
        .particle_assets
        .load_referenced(source, particle_paths);
    let mut light_paths = attachment_paths(scene, UnitAttachmentKind::Light);
    light_paths.extend(animation_tag_paths(scene, "Light"));
    light_paths.extend(scene.terrain_effect_assets.light_paths());
    if let Some(assets) = &scene.terrain_impact_assets {
        light_paths.extend(assets.light_paths());
    }
    scene.light_assets.load_referenced(source, light_paths);
    let mut decal_paths = scene.terrain_effect_assets.decal_paths();
    if let Some(assets) = &scene.terrain_impact_assets {
        decal_paths.extend(assets.decal_paths());
    }
    scene
        .impact_decal_assets
        .load_referenced(source, decal_paths);
}

fn load_terrain_impact_assets(scene: &mut UnitScene, source: &mut AssetSource<StdFileProvider>) {
    if scene.terrain_impact_assets.is_some() || !scene.terrain_impact_issues.is_empty() {
        return;
    }
    match TerrainImpactAssets::load(source) {
        Ok(assets) => {
            scene.terrain_impact_issues = assets.issues().to_vec();
            scene.terrain_impact_assets = Some(assets);
        }
        Err(error) => scene.terrain_impact_issues.push(error.to_string()),
    }
}

fn attachment_paths(scene: &UnitScene, kind: UnitAttachmentKind) -> Vec<String> {
    let placed = scene
        .placements
        .iter()
        .flat_map(|placement| {
            placement.unit.attachments_for_animations(
                placement.animation_type(),
                placement.movement_track_animation_type(),
            )
        })
        .filter(|attachment| attachment.kind == kind)
        .filter_map(|attachment| attachment.asset_path.clone());
    placed
        .chain(impact_visual_attachment_paths(scene, kind))
        .collect()
}

fn impact_visual_attachment_paths(
    scene: &UnitScene,
    kind: UnitAttachmentKind,
) -> impl Iterator<Item = String> + '_ {
    scene
        .impact_visual_assets
        .loaded()
        .flat_map(|unit| unit.attachments_for_animation(None))
        .filter(move |attachment| attachment.kind == kind)
        .filter_map(|attachment| attachment.asset_path.clone())
}

fn animation_tag_paths(scene: &UnitScene, tag_type: &str) -> Vec<String> {
    scene
        .placements
        .iter()
        .map(|placement| placement.unit.as_ref())
        .chain(scene.impact_visual_assets.loaded().map(Arc::as_ref))
        .flat_map(Unit::animation_tags)
        .filter(|tag| tag.tag_type.eq_ignore_ascii_case(tag_type))
        .filter_map(|tag| tag.name.clone())
        .collect()
}
