//! Presentation assets for UGX units bound to authoritative simulation entities.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat4, Vec3};
use pipeline::database::hw1::{ProtoObject, Visual};
use pipeline::source::{AssetSource, StdFileProvider};
use sim::{EntityId, TeamId, World as SimWorld};

use super::Unit;
use super::unit::{UnitAnimationRequest, UnitAssetCache, UnitIkProfile};
use crate::terrain_effect::TerrainImpactAssets;

mod combat_animations;
mod impact_decals;
mod impact_visuals;
mod lights;
mod loading;
mod movement_animations;
mod particles;
mod renderer;
mod terrain_effects;

use combat_animations::{SimulationEntityState, simulation_entity_states};
use impact_decals::ImpactDecalAssets;
use impact_visuals::ImpactVisualAssets;
use lights::LightAssets;
use loading::{SceneAssets, SceneInputs, load_attachment_assets, load_placements};
use movement_animations::{MovementAnimation, MovementAnimationProfile};
use particles::ParticleAssets;
use terrain_effects::TerrainEffectAssets;

pub use renderer::{
    AnimationCameraShake, AnimationTerrainAlpha, AnimationTerrainAlphaShape, UnitSceneRenderer,
    simulation_entity_secondary_transform,
};
pub use sim::{
    ScenarioPositionAxes, scenario_object_direction_to_world, scenario_object_position_to_world,
};

/// One decoded visual bound to an entity in the simulation world.
#[derive(Clone, Debug)]
pub struct UnitPlacement {
    entity_id: EntityId,
    proto_name: String,
    visual_variation_index: Option<usize>,
    animation_type: Option<String>,
    animation_asset: Option<String>,
    animation_uses_simulation_clock: bool,
    animation_revision: u32,
    combat_animation_duration: Option<f32>,
    movement_animation: Option<MovementAnimation>,
    movement_track_animation: Option<MovementAnimation>,
    movement_profile: Option<MovementAnimationProfile>,
    ik_profile: UnitIkProfile,
    visual_mesh_mask: sim::UnitVisualMeshMask,
    visual_opacity: f32,
    transform: Mat4,
    secondary_transform: Option<Mat4>,
    unit: Arc<Unit>,
}

impl UnitPlacement {
    /// Return the authoritative simulation entity for this visual.
    #[must_use]
    pub const fn entity_id(&self) -> EntityId {
        self.entity_id
    }

    /// Return the database proto-object name used to resolve the visual.
    #[must_use]
    pub fn proto_name(&self) -> &str {
        &self.proto_name
    }

    /// Return the visual variation selected by authoritative simulation state.
    #[must_use]
    pub const fn visual_variation_index(&self) -> Option<usize> {
        self.visual_variation_index
    }

    /// Return the sim animation revision captured by this decoded visual.
    #[must_use]
    pub const fn animation_revision(&self) -> u32 {
        self.animation_revision
    }

    /// Return the authoritative visual animation type captured for this placement.
    #[must_use]
    pub fn animation_type(&self) -> Option<&str> {
        self.animation_type.as_deref()
    }

    /// Return the exact simulation-selected animation asset when one exists.
    #[must_use]
    pub fn animation_asset(&self) -> Option<&str> {
        self.animation_asset.as_deref()
    }

    #[must_use]
    pub const fn animation_uses_simulation_clock(&self) -> bool {
        self.animation_uses_simulation_clock
    }

    fn movement_track_animation_type(&self) -> Option<&'static str> {
        self.movement_track_animation.map(MovementAnimation::as_str)
    }

    fn ik_profile(&self) -> &UnitIkProfile {
        &self.ik_profile
    }

    /// Return the authoritative per-mesh visibility captured from simulation.
    #[must_use]
    pub const fn visual_mesh_mask(&self) -> &sim::UnitVisualMeshMask {
        &self.visual_mesh_mask
    }

    /// Return the authoritative whole-model opacity captured from simulation.
    #[must_use]
    pub const fn visual_opacity(&self) -> f32 {
        self.visual_opacity
    }

    /// Return the simulation transform captured while building the scene.
    ///
    /// [`UnitSceneRenderer`] refreshes this from the live sim world every frame.
    #[must_use]
    pub const fn transform(&self) -> Mat4 {
        self.transform
    }

    /// Return the optional sim-owned second world matrix for beam visuals.
    #[must_use]
    pub const fn secondary_transform(&self) -> Option<Mat4> {
        self.secondary_transform
    }

    /// Return the shared decoded visual graph for this placement.
    #[must_use]
    pub fn unit(&self) -> &Unit {
        &self.unit
    }
}

/// A unique visual-loading failure encountered while assembling a scene.
#[derive(Clone, Debug)]
pub struct UnitSceneIssue {
    proto_name: String,
    reason: String,
}

impl UnitSceneIssue {
    /// Return the proto-object whose visual failed to load.
    #[must_use]
    pub fn proto_name(&self) -> &str {
        &self.proto_name
    }

    /// Return the model or animation loading diagnostic.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Decoded presentation assets for the current simulation entity roster.
#[derive(Debug, Default)]
pub struct UnitScene {
    placements: Vec<UnitPlacement>,
    issues: Vec<UnitSceneIssue>,
    decoded_visuals: HashMap<String, Option<Arc<Unit>>>,
    light_assets: LightAssets,
    particle_assets: ParticleAssets,
    terrain_effect_assets: TerrainEffectAssets,
    terrain_impact_assets: Option<TerrainImpactAssets>,
    terrain_impact_issues: Vec<String>,
    impact_decal_assets: ImpactDecalAssets,
    impact_visual_assets: ImpactVisualAssets,
    animation_selections: HashMap<EntityId, (String, u64)>,
    simulation_entity_states: Vec<SimulationEntityState>,
    simulation_entity_count: usize,
    unique_visual_count: usize,
    skipped_no_render_count: usize,
    missing_proto_count: usize,
    missing_visual_count: usize,
    invalid_transform_count: usize,
    load_failure_count: usize,
}

impl UnitScene {
    /// Resolve visual assets for every renderable entity in the authoritative sim world.
    ///
    /// Asset decoding is cached by prototype, sim-selected variation, and
    /// scripted animation. The GPU renderer fetches current transforms from
    /// [`SimWorld`] every frame.
    #[must_use]
    pub fn load_world(
        source: &mut AssetSource<StdFileProvider>,
        world: &SimWorld,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
    ) -> Self {
        Self::load_world_reusing(
            source,
            SceneInputs {
                world,
                gameplay: None,
                visuals,
                proto_objects,
                previous_movement: None,
                previous_animation_selections: None,
            },
            SceneAssets::default(),
        )
    }

    /// Resolve visuals including simulation-owned combat animation selections.
    #[must_use]
    pub fn load_world_with_gameplay(
        source: &mut AssetSource<StdFileProvider>,
        world: &SimWorld,
        gameplay: &sim::GameplayCatalog,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
    ) -> Self {
        Self::load_world_reusing(
            source,
            SceneInputs {
                world,
                gameplay: Some(gameplay),
                visuals,
                proto_objects,
                previous_movement: None,
                previous_animation_selections: None,
            },
            SceneAssets::default(),
        )
    }

    /// Refresh presentation assets when the authoritative entity roster changes.
    ///
    /// Existing decoded visuals are retained by prototype, variation, and
    /// animation, so an identical spawn normally allocates only per-instance
    /// presentation data. Returns whether the roster changed.
    pub fn sync_world(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        world: &SimWorld,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
    ) -> bool {
        self.sync_world_projection(source, world, None, visuals, proto_objects)
    }

    /// Refresh visuals including simulation-owned combat animation selections.
    pub fn sync_world_with_gameplay(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        world: &SimWorld,
        gameplay: &sim::GameplayCatalog,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
    ) -> bool {
        self.sync_world_projection(source, world, Some(gameplay), visuals, proto_objects)
    }

    fn sync_world_projection(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        world: &SimWorld,
        gameplay: Option<&sim::GameplayCatalog>,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
    ) -> bool {
        if self.roster_matches_projection(world, gameplay) {
            return false;
        }
        let previous_movement = self
            .placements
            .iter()
            .filter_map(|placement| {
                placement
                    .movement_animation
                    .map(|animation| (placement.entity_id, animation))
            })
            .collect::<HashMap<_, _>>();
        let previous_animation_selections = self.animation_selections.clone();
        *self = Self::load_world_reusing(
            source,
            SceneInputs {
                world,
                gameplay,
                visuals,
                proto_objects,
                previous_movement: Some(&previous_movement),
                previous_animation_selections: Some(&previous_animation_selections),
            },
            SceneAssets {
                units: self.decoded_visuals.clone(),
                lights: self.light_assets.clone(),
                particles: self.particle_assets.clone(),
                terrain_effects: self.terrain_effect_assets.clone(),
                terrain_impacts: self.terrain_impact_assets.clone(),
                terrain_impact_issues: self.terrain_impact_issues.clone(),
                impact_decals: self.impact_decal_assets.clone(),
                impact_visuals: self.impact_visual_assets.clone(),
            },
        );
        true
    }

    /// Return whether this presentation roster matches all live sim visuals.
    #[must_use]
    pub fn roster_matches(&self, world: &SimWorld) -> bool {
        self.roster_matches_projection(world, None)
    }

    /// Return whether visuals also match live combat animation selections.
    #[must_use]
    pub fn roster_matches_with_gameplay(
        &self,
        world: &SimWorld,
        gameplay: &sim::GameplayCatalog,
    ) -> bool {
        self.roster_matches_projection(world, Some(gameplay))
    }

    fn roster_matches_projection(
        &self,
        world: &SimWorld,
        gameplay: Option<&sim::GameplayCatalog>,
    ) -> bool {
        self.simulation_entity_states
            .iter()
            .cloned()
            .eq(simulation_entity_states(world, gameplay))
            && self.placements.iter().all(|placement| {
                let Some(profile) = placement.movement_profile else {
                    return true;
                };
                let Some(unit) = world.get_unit(placement.entity_id) else {
                    return false;
                };
                let previous = placement.movement_animation.unwrap_or_default();
                profile.select(unit, previous) == previous
            })
    }

    fn load_world_reusing(
        source: &mut AssetSource<StdFileProvider>,
        inputs: SceneInputs<'_>,
        mut assets: SceneAssets,
    ) -> Self {
        let simulation_entity_states =
            simulation_entity_states(inputs.world, inputs.gameplay).collect::<Vec<_>>();
        let mut scene = Self {
            simulation_entity_count: simulation_entity_states.len(),
            simulation_entity_states,
            light_assets: assets.lights,
            particle_assets: assets.particles,
            terrain_effect_assets: assets.terrain_effects,
            terrain_impact_assets: assets.terrain_impacts,
            terrain_impact_issues: assets.terrain_impact_issues,
            impact_decal_assets: assets.impact_decals,
            impact_visual_assets: assets.impact_visuals,
            ..Self::default()
        };
        load_placements(&mut scene, source, inputs, &mut assets.units);

        load_attachment_assets(&mut scene, source);
        scene.decoded_visuals = assets
            .units
            .into_iter()
            .filter(|(_, decoded)| decoded.is_some())
            .collect();
        scene
    }

    /// Return all successfully decoded placements in entity-pool order.
    #[must_use]
    pub fn placements(&self) -> &[UnitPlacement] {
        &self.placements
    }

    /// Return one diagnostic per unique visual that failed to load.
    #[must_use]
    pub fn issues(&self) -> &[UnitSceneIssue] {
        &self.issues
    }

    /// Return the number of unit and projectile entities considered from the sim world.
    #[must_use]
    pub const fn simulation_entity_count(&self) -> usize {
        self.simulation_entity_count
    }

    /// Return the number of successfully decoded placed visual instances.
    #[must_use]
    pub fn placement_count(&self) -> usize {
        self.placements.len()
    }

    /// Return the number of distinct decoded proto visuals.
    #[must_use]
    pub const fn unique_visual_count(&self) -> usize {
        self.unique_visual_count
    }

    /// Return the number of prototype-authored `NoRender` entities omitted.
    #[must_use]
    pub const fn skipped_no_render_count(&self) -> usize {
        self.skipped_no_render_count
    }

    /// Return the number of sim entities without a database proto name.
    #[must_use]
    pub const fn missing_proto_count(&self) -> usize {
        self.missing_proto_count
    }

    /// Return the number of sim entities without visual definitions.
    #[must_use]
    pub const fn missing_visual_count(&self) -> usize {
        self.missing_visual_count
    }

    /// Return the number of entities with invalid sim transforms.
    #[must_use]
    pub const fn invalid_transform_count(&self) -> usize {
        self.invalid_transform_count
    }

    /// Return the number of entities skipped because their shared visual failed.
    #[must_use]
    pub const fn load_failure_count(&self) -> usize {
        self.load_failure_count
    }

    /// Return the number of global impact TFX graphs decoded for this scene.
    #[must_use]
    pub fn terrain_impact_effect_count(&self) -> usize {
        self.terrain_impact_assets
            .as_ref()
            .map_or(0, TerrainImpactAssets::loaded_effect_count)
    }

    /// Return missing or malformed impact catalog/TFX diagnostics.
    #[must_use]
    pub fn terrain_impact_effect_issues(&self) -> &[String] {
        &self.terrain_impact_issues
    }

    /// Return the number of impact catalog or TFX preload diagnostics.
    #[must_use]
    pub fn terrain_impact_effect_issue_count(&self) -> usize {
        self.terrain_impact_issues.len()
    }
}

fn visual_name_lookup(visuals: &HashMap<String, Visual>) -> HashMap<String, &str> {
    visuals
        .keys()
        .map(|name| (name.to_ascii_lowercase(), name.as_str()))
        .collect()
}

fn hidden_proto_names(proto_objects: &[ProtoObject]) -> HashSet<String> {
    proto_objects
        .iter()
        .filter(|proto| prototype_is_hidden(proto))
        .map(|proto| proto.name.to_ascii_lowercase())
        .collect()
}

fn load_cached_unit(
    source: &mut AssetSource<StdFileProvider>,
    visual: &Visual,
    lookup_name: &str,
    visual_variation_index: Option<usize>,
    animation: UnitAnimationRequest<'_>,
    asset_cache: &mut UnitAssetCache,
    units: &mut HashMap<String, Option<Arc<Unit>>>,
) -> Result<Arc<Unit>, Option<String>> {
    let cache_key = visual_cache_key(lookup_name, visual_variation_index, animation);
    if let Some(cached) = units.get(&cache_key) {
        return cached.clone().ok_or(None);
    }
    match Unit::load_variant_with_animation_cache(
        source,
        visual,
        visual_variation_index,
        animation,
        asset_cache,
    ) {
        Ok(unit) => {
            let unit = Arc::new(unit);
            units.insert(cache_key, Some(Arc::clone(&unit)));
            Ok(unit)
        }
        Err(error) => {
            units.insert(cache_key, None);
            Err(Some(error.to_string()))
        }
    }
}

fn visual_cache_key(
    lookup_name: &str,
    visual_variation_index: Option<usize>,
    animation: UnitAnimationRequest<'_>,
) -> String {
    let variation =
        visual_variation_index.map_or_else(|| "random".to_owned(), |index| index.to_string());
    let animation_asset = animation
        .animation_asset
        .map_or_else(String::new, str::to_ascii_lowercase);
    let animation_type = animation
        .animation_type
        .map_or_else(String::new, str::to_ascii_lowercase);
    let movement_animation = animation
        .movement_animation_type
        .map_or_else(String::new, str::to_ascii_lowercase);
    let sim_clock = animation.uses_simulation_clock;
    let animation_roll = animation.animation_roll;
    let movement_roll = animation.movement_animation_roll;
    format!(
        "{lookup_name}\0variation={variation}\0animation_type={animation_type}\0animation={animation_asset}\0sim_clock={sim_clock}\0roll={animation_roll}\0movement={movement_animation}\0movement_roll={movement_roll}"
    )
}

fn prototype_is_hidden(proto: &ProtoObject) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("NoRender"))
}

struct SimulationVisual<'a> {
    id: EntityId,
    proto_name: &'a str,
    visual_variation_index: Option<usize>,
    animation_type: Option<&'a str>,
    animation_asset: Option<&'a str>,
    animation_revision: u32,
    visual_mesh_mask: Option<&'a sim::UnitVisualMeshMask>,
    visual_opacity: f32,
    transform: Option<Mat4>,
}

const AIRCRAFT_KAMIKAZE_ANIMATION_REVISION: u32 = 0x8000_0001;

/// Iterate prototype names for every renderer-facing entity in the sim roster.
///
/// Presentation asset loaders use this same projection as [`UnitScene`], so
/// class-0 scenario visuals cannot be omitted by a separate roster policy.
pub fn simulation_proto_names(world: &SimWorld) -> impl Iterator<Item = &str> {
    world
        .objects
        .iter()
        .filter(|(_, object)| object.is_visual() && object.object_state.is_render_enabled())
        .map(|(_, object)| object.proto_object_name.as_str())
        .chain(
            world
                .units
                .iter()
                .filter(|(_, unit)| unit.object_state.is_render_enabled())
                .map(|(_, unit)| unit.proto_object_name.as_str()),
        )
        .chain(
            world
                .projectiles
                .iter()
                .filter(|(_, projectile)| projectile.object_state.is_render_enabled())
                .map(|(_, projectile)| projectile.visual_proto_object_name()),
        )
}

fn simulation_visuals(world: &SimWorld) -> impl Iterator<Item = SimulationVisual<'_>> {
    world
        .objects
        .iter()
        .filter(|(_, object)| object.is_visual() && object.object_state.is_render_enabled())
        .map(|(id, object)| SimulationVisual {
            id,
            proto_name: &object.proto_object_name,
            visual_variation_index: object.object_state.visual_variation_index(),
            animation_type: object
                .object_state
                .scripted_animation()
                .map(sim::ScriptedAnimation::animation_type),
            animation_asset: object
                .object_state
                .scripted_animation()
                .and_then(sim::ScriptedAnimation::asset_path),
            animation_revision: object
                .object_state
                .scripted_animation()
                .map_or(0, sim::ScriptedAnimation::revision),
            visual_mesh_mask: None,
            visual_opacity: 1.0,
            transform: simulation_object_transform(object),
        })
        .chain(
            world
                .units
                .iter()
                .filter(|(_, unit)| unit.object_state.is_render_enabled())
                .map(|(id, unit)| simulation_unit_visual(id, unit)),
        )
        .chain(
            world
                .projectiles
                .iter()
                .filter(|(_, projectile)| projectile.object_state.is_render_enabled())
                .map(|(id, projectile)| SimulationVisual {
                    id,
                    proto_name: projectile.visual_proto_object_name(),
                    visual_variation_index: projectile.object_state.visual_variation_index(),
                    animation_type: projectile
                        .object_state
                        .scripted_animation()
                        .map(sim::ScriptedAnimation::animation_type),
                    animation_asset: projectile
                        .object_state
                        .scripted_animation()
                        .and_then(sim::ScriptedAnimation::asset_path),
                    animation_revision: projectile
                        .object_state
                        .scripted_animation()
                        .map_or(0, sim::ScriptedAnimation::revision),
                    visual_mesh_mask: None,
                    visual_opacity: 1.0,
                    transform: simulation_projectile_transform(projectile),
                }),
        )
}

fn simulation_unit_visual(id: EntityId, unit: &sim::Unit) -> SimulationVisual<'_> {
    let scripted = unit.object_state.scripted_animation();
    let targeted_kamikaze = unit.aircraft_crash_phase() == sim::AircraftCrashPhase::Crashing
        && unit.kamikaze_target().is_some();
    SimulationVisual {
        id,
        proto_name: &unit.proto_object_name,
        visual_variation_index: unit.object_state.visual_variation_index(),
        animation_type: if targeted_kamikaze {
            Some("Kamikaze")
        } else {
            scripted.map(sim::ScriptedAnimation::animation_type)
        },
        animation_asset: if targeted_kamikaze {
            None
        } else {
            scripted.and_then(sim::ScriptedAnimation::asset_path)
        },
        animation_revision: if targeted_kamikaze {
            AIRCRAFT_KAMIKAZE_ANIMATION_REVISION
        } else {
            scripted.map_or(0, sim::ScriptedAnimation::revision)
        },
        visual_mesh_mask: Some(unit.visual_mesh_mask()),
        visual_opacity: unit.visual_opacity(),
        transform: simulation_unit_model_transform(unit),
    }
}

/// Resolve a render transform for any sim entity represented by this scene.
#[must_use]
pub fn simulation_entity_transform(world: &SimWorld, entity_id: EntityId) -> Option<Mat4> {
    world
        .get_object(entity_id)
        .filter(|object| object.is_visual() && object.object_state.is_render_enabled())
        .and_then(simulation_object_transform)
        .or_else(|| {
            world
                .get_unit(entity_id)
                .filter(|unit| unit.object_state.is_render_enabled())
                .and_then(simulation_unit_transform)
        })
        .or_else(|| {
            world
                .get_projectile(entity_id)
                .filter(|projectile| projectile.object_state.is_render_enabled())
                .and_then(simulation_projectile_transform)
        })
}

/// Project authoritative whole-model opacity for any live render entity.
#[must_use]
pub fn simulation_entity_visual_opacity(world: &SimWorld, entity_id: EntityId) -> Option<f32> {
    world
        .get_unit(entity_id)
        .filter(|unit| unit.object_state.is_render_enabled())
        .map(sim::Unit::visual_opacity)
        .or_else(|| {
            world
                .get_object(entity_id)
                .filter(|object| object.is_visual() && object.object_state.is_render_enabled())
                .map(|_| 1.0)
        })
        .or_else(|| {
            world
                .get_projectile(entity_id)
                .filter(|projectile| projectile.object_state.is_render_enabled())
                .map(|_| 1.0)
        })
}

fn simulation_object_transform(object: &sim::Object) -> Option<Mat4> {
    simulation_model_transform(object.base.position, object.base.forward)
}

/// Project the simulation's authoritative team visibility into presentation.
#[must_use]
pub fn simulation_entity_visible_to_team(
    world: &SimWorld,
    team_id: TeamId,
    entity_id: EntityId,
) -> bool {
    world.is_entity_visible_to_team(team_id, entity_id)
}

/// Project one authoritative targeting-selection request into presentation.
#[must_use]
pub fn simulation_entity_flash(
    world: &SimWorld,
    entity_id: EntityId,
) -> Option<sim::TargetingSelection> {
    world.entity_targeting_selection(entity_id)
}

/// Project one authoritative scripted-animation request into presentation.
#[must_use]
pub fn simulation_entity_animation(
    world: &SimWorld,
    entity_id: EntityId,
) -> Option<&sim::ScriptedAnimation> {
    world.entity_scripted_animation(entity_id)
}

/// Build a model-to-world matrix solely from authoritative simulation state.
#[must_use]
pub fn simulation_unit_transform(unit: &sim::Unit) -> Option<Mat4> {
    if unit.is_garrisoned() || !unit.object_state.is_render_enabled() {
        return None;
    }
    simulation_unit_model_transform(unit)
}

fn simulation_unit_model_transform(unit: &sim::Unit) -> Option<Mat4> {
    if unit.is_crashing() {
        simulation_directional_model_transform(unit.base.position, unit.base.forward, Vec3::ZERO)
    } else {
        simulation_model_transform(unit.base.position, unit.base.forward)
    }
}

fn simulation_model_transform(position: Vec3, world_forward: Vec3) -> Option<Mat4> {
    if !position.is_finite() || !world_forward.is_finite() {
        return None;
    }
    let forward = Vec3::new(world_forward.x, 0.0, world_forward.z).try_normalize()?;
    let right = Vec3::Y.cross(forward).try_normalize()?;
    Some(Mat4::from_cols(
        right.extend(0.0),
        Vec3::Y.extend(0.0),
        forward.extend(0.0),
        position.extend(1.0),
    ))
}

/// Build a projectile transform including authored vertical flight direction.
#[must_use]
pub fn simulation_projectile_transform(projectile: &sim::Projectile) -> Option<Mat4> {
    let position = projectile.base.position;
    let forward = projectile.base.forward;
    let visual_center_offset = projectile.visual_center_offset();
    simulation_directional_model_transform(position, forward, visual_center_offset)
}

fn simulation_directional_model_transform(
    position: Vec3,
    forward: Vec3,
    visual_center_offset: Vec3,
) -> Option<Mat4> {
    if !position.is_finite() || !forward.is_finite() || !visual_center_offset.is_finite() {
        return None;
    }
    let forward = forward.try_normalize()?;
    let reference_up = if forward.dot(Vec3::Y).abs() > 0.999 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let right = reference_up.cross(forward).try_normalize()?;
    let up = forward.cross(right).try_normalize()?;
    let visual_position = position
        + right * visual_center_offset.x
        + up * visual_center_offset.y
        + forward * visual_center_offset.z;
    Some(Mat4::from_cols(
        right.extend(0.0),
        up.extend(0.0),
        forward.extend(0.0),
        visual_position.extend(1.0),
    ))
}

#[cfg(test)]
#[path = "scene/attachment_tests.rs"]
mod attachment_tests;

#[cfg(test)]
#[path = "scene/tests.rs"]
mod tests;
