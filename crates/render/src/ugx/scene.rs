//! Scenario placement for recursively assembled UGX units.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat4, Vec3};
use pipeline::database::hw1::{ProtoObject, Visual};
use pipeline::hw1::scenario::{ScenarioData, ScenarioObject, ScenarioPosition};
use pipeline::source::{AssetSource, StdFileProvider};

use super::renderer::{RendererResources, WorldBindings};
use super::unit::UnitAssetCache;
use super::{Unit, UnitRenderer};
use crate::environment::EnvironmentMap;
use crate::terrain::LightingParams;
use crate::{RenderPhase, WorldRenderer};

/// Authored source of a decoded world placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitPlacementOrigin {
    /// Persistent object from the scenario's `<Objects>` collection.
    ScenarioObject {
        /// Stable object identifier from the SCN.
        object_id: i32,
    },
    /// Skirmish starting-base location from the scenario's `<Positions>` collection.
    PlayerStart {
        /// Authored player assignment (`-1` when chosen at runtime).
        player: i32,
        /// Stable start-position number.
        number: i32,
    },
}

/// One decoded visual placed by a scenario object or player start.
#[derive(Clone, Debug)]
pub struct UnitPlacement {
    origin: UnitPlacementOrigin,
    proto_name: String,
    editor_name: String,
    transform: Mat4,
    unit: Arc<Unit>,
}

impl UnitPlacement {
    /// Returns the authored source of this placement.
    #[must_use]
    pub fn origin(&self) -> UnitPlacementOrigin {
        self.origin
    }

    /// Returns the scenario object's stable identifier, or `None` for a
    /// player-start placement.
    #[must_use]
    pub fn object_id(&self) -> Option<i32> {
        match self.origin {
            UnitPlacementOrigin::ScenarioObject { object_id } => Some(object_id),
            UnitPlacementOrigin::PlayerStart { .. } => None,
        }
    }

    /// Returns the database proto-object name used to resolve the visual.
    #[must_use]
    pub fn proto_name(&self) -> &str {
        &self.proto_name
    }

    /// Returns the optional editor-facing placement name.
    #[must_use]
    pub fn editor_name(&self) -> &str {
        &self.editor_name
    }

    /// Returns the scenario object's model-to-world transform.
    #[must_use]
    pub fn transform(&self) -> Mat4 {
        self.transform
    }

    /// Returns the shared decoded visual graph for this placement.
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
    /// Returns the proto-object whose visual failed to load.
    #[must_use]
    pub fn proto_name(&self) -> &str {
        &self.proto_name
    }

    /// Returns the model or animation loading diagnostic.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Decoded renderable scenario objects and diagnostics for skipped entries.
#[derive(Debug, Default)]
pub struct UnitScene {
    placements: Vec<UnitPlacement>,
    issues: Vec<UnitSceneIssue>,
    object_count: usize,
    player_start_count: usize,
    player_start_failure_count: usize,
    unique_visual_count: usize,
    skipped_squad_count: usize,
    skipped_no_render_count: usize,
    missing_proto_count: usize,
    missing_visual_count: usize,
    invalid_transform_count: usize,
    load_failure_count: usize,
}

impl UnitScene {
    /// Resolves every non-squad scenario object that has a visual definition.
    ///
    /// Decoded units are cached by proto name and variation index. Placements
    /// share those CPU assets while retaining their own authored SCN world
    /// transform. Objects with no visual are expected for gameplay-only markers
    /// and are counted rather than treated as fatal errors.
    #[must_use]
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        objects: &[ScenarioObject],
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
    ) -> Self {
        let mut cache = UnitAssetCache::default();
        Self::load_with_cache(source, objects, visuals, proto_objects, &mut cache)
    }

    fn load_with_cache(
        source: &mut AssetSource<StdFileProvider>,
        objects: &[ScenarioObject],
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
        asset_cache: &mut UnitAssetCache,
    ) -> Self {
        let mut scene = Self {
            object_count: objects.len(),
            ..Self::default()
        };
        let visual_names = visuals
            .keys()
            .map(|name| (name.to_ascii_lowercase(), name.as_str()))
            .collect::<HashMap<_, _>>();
        let no_render_objects = proto_objects
            .iter()
            .filter(|proto| prototype_is_hidden(proto))
            .map(|proto| proto.name.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let mut units = HashMap::<(String, Option<usize>), Option<Arc<Unit>>>::new();

        for object in objects {
            if object.is_squad {
                scene.skipped_squad_count += 1;
                continue;
            }
            let Some(proto_name) = scenario_proto_name(object) else {
                scene.missing_proto_count += 1;
                continue;
            };
            let lookup_name = proto_name.to_ascii_lowercase();
            if no_render_objects.contains(&lookup_name) {
                scene.skipped_no_render_count += 1;
                continue;
            }
            let Some(&visual_name) = visual_names.get(&lookup_name) else {
                scene.missing_visual_count += 1;
                continue;
            };
            let Some(transform) = scenario_transform(object) else {
                scene.invalid_transform_count += 1;
                continue;
            };
            let variation_index = usize::try_from(object.visual_variation_index).ok();
            let cache_key = (lookup_name, variation_index);
            let unit = if let Some(cached) = units.get(&cache_key) {
                cached.clone()
            } else {
                let loaded = match Unit::load_variant_with_cache(
                    source,
                    &visuals[visual_name],
                    variation_index,
                    asset_cache,
                ) {
                    Ok(unit) => {
                        scene.unique_visual_count += 1;
                        Some(Arc::new(unit))
                    }
                    Err(error) => {
                        scene.issues.push(UnitSceneIssue {
                            proto_name: proto_name.clone(),
                            reason: error.to_string(),
                        });
                        None
                    }
                };
                units.insert(cache_key, loaded.clone());
                loaded
            };
            let Some(unit) = unit else {
                scene.load_failure_count += 1;
                continue;
            };
            scene.placements.push(UnitPlacement {
                origin: UnitPlacementOrigin::ScenarioObject {
                    object_id: object.id,
                },
                proto_name,
                editor_name: object.editor_name.clone(),
                transform,
                unit,
            });
        }
        scene
    }

    /// Resolves persistent scenario objects and previews the skirmish base pad
    /// at every authored player-start position.
    ///
    /// `player_start_proto` should come from the decoded
    /// `SkirmishEmptyBaseObject` game-data mapping. Start placements share the
    /// same decoded unit as matching expansion sockets when one is already
    /// present in `<Objects>`.
    #[must_use]
    pub fn load_scenario(
        source: &mut AssetSource<StdFileProvider>,
        scenario: &ScenarioData,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
        player_start_proto: &str,
    ) -> Self {
        let mut cache = UnitAssetCache::default();
        let mut scene = Self::load_with_cache(
            source,
            scenario.objects(),
            visuals,
            proto_objects,
            &mut cache,
        );
        scene.player_start_count = scenario.positions().len();
        scene.append_player_starts(
            source,
            scenario.positions(),
            visuals,
            player_start_proto,
            &mut cache,
        );
        scene
    }

    fn append_player_starts(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        starts: &[ScenarioPosition],
        visuals: &HashMap<String, Visual>,
        proto_name: &str,
        asset_cache: &mut UnitAssetCache,
    ) {
        if starts.is_empty() {
            return;
        }
        let proto_name = proto_name.trim();
        let existing = self
            .placements
            .iter()
            .find(|placement| placement.proto_name.eq_ignore_ascii_case(proto_name))
            .map(|placement| Arc::clone(&placement.unit));
        let unit = if let Some(unit) = existing {
            unit
        } else {
            let Some((visual_name, visual)) = visuals
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(proto_name))
            else {
                self.player_start_failure_count = starts.len();
                self.issues.push(UnitSceneIssue {
                    proto_name: proto_name.to_owned(),
                    reason: "player-start visual definition not found".to_owned(),
                });
                return;
            };
            match Unit::load_variant_with_cache(source, visual, None, asset_cache) {
                Ok(unit) => {
                    self.unique_visual_count += 1;
                    Arc::new(unit)
                }
                Err(error) => {
                    self.player_start_failure_count = starts.len();
                    self.issues.push(UnitSceneIssue {
                        proto_name: visual_name.clone(),
                        reason: error.to_string(),
                    });
                    return;
                }
            }
        };

        for start in starts {
            let Some(transform) = player_start_transform(start) else {
                self.invalid_transform_count += 1;
                self.player_start_failure_count += 1;
                continue;
            };
            self.placements.push(UnitPlacement {
                origin: UnitPlacementOrigin::PlayerStart {
                    player: start.player,
                    number: start.number,
                },
                proto_name: proto_name.to_owned(),
                editor_name: format!("PlayerStart_{}", start.number),
                transform,
                unit: Arc::clone(&unit),
            });
        }
    }

    /// Returns all successfully decoded placements in scenario order.
    #[must_use]
    pub fn placements(&self) -> &[UnitPlacement] {
        &self.placements
    }

    /// Returns one diagnostic per unique visual and variation that failed.
    #[must_use]
    pub fn issues(&self) -> &[UnitSceneIssue] {
        &self.issues
    }

    /// Returns the number of objects supplied by the scenario.
    #[must_use]
    pub fn object_count(&self) -> usize {
        self.object_count
    }

    /// Returns the number of authored skirmish start positions supplied by the
    /// scenario.
    #[must_use]
    pub fn player_start_count(&self) -> usize {
        self.player_start_count
    }

    /// Returns the number of starting-base pads that could not be placed.
    #[must_use]
    pub fn player_start_failure_count(&self) -> usize {
        self.player_start_failure_count
    }

    /// Returns the number of successfully decoded placed visual instances.
    #[must_use]
    pub fn placement_count(&self) -> usize {
        self.placements.len()
    }

    /// Returns the number of distinct decoded proto/variation combinations.
    #[must_use]
    pub fn unique_visual_count(&self) -> usize {
        self.unique_visual_count
    }

    /// Returns the number of squad entries deferred to formation expansion.
    #[must_use]
    pub fn skipped_squad_count(&self) -> usize {
        self.skipped_squad_count
    }

    /// Returns the number of prototype-authored `NoRender` placements omitted.
    #[must_use]
    pub fn skipped_no_render_count(&self) -> usize {
        self.skipped_no_render_count
    }

    /// Returns the number of malformed placements with no resolvable proto name.
    #[must_use]
    pub fn missing_proto_count(&self) -> usize {
        self.missing_proto_count
    }

    /// Returns the number of gameplay-only objects without visual definitions.
    #[must_use]
    pub fn missing_visual_count(&self) -> usize {
        self.missing_visual_count
    }

    /// Returns the number of placements with invalid orientation or position data.
    #[must_use]
    pub fn invalid_transform_count(&self) -> usize {
        self.invalid_transform_count
    }

    /// Returns the number of placements skipped because their shared visual failed.
    #[must_use]
    pub fn load_failure_count(&self) -> usize {
        self.load_failure_count
    }
}

fn prototype_is_hidden(proto: &ProtoObject) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("NoRender"))
}

fn scenario_proto_name(object: &ScenarioObject) -> Option<String> {
    let direct = object.proto_name.trim();
    if !direct.is_empty() {
        return Some(direct.to_owned());
    }

    // Mixed-content object nodes put flags after the proto name. The scenario
    // reader deliberately exposes only the common fields, but EditorName keeps
    // the exact `<proto>_<ID>` identity for these placements.
    let suffix = format!("_{}", object.id);
    object
        .editor_name
        .strip_suffix(&suffix)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

/// Converts an authored SCN `<Object>` position into terrain-world coordinates.
///
/// Object records use the same diagonally transposed X/Z storage axes as the
/// XTD heightfield. Player-start `<Position>` records are a separate domain and
/// are already expressed in terrain-world axes.
#[must_use]
pub fn scenario_object_position_to_world(position: [f32; 3]) -> [f32; 3] {
    [position[2], position[1], position[0]]
}

/// Converts an authored SCN `<Object>` direction into terrain-world axes.
#[must_use]
pub fn scenario_object_direction_to_world(direction: [f32; 3]) -> [f32; 3] {
    [direction[2], direction[1], direction[0]]
}

fn scenario_transform(object: &ScenarioObject) -> Option<Mat4> {
    let position = Vec3::from_array(scenario_object_position_to_world(object.position_vec3()));
    // Swapping X/Z reverses parity. Negating the right axis retains authored
    // facing while keeping the rendered unit upright and right-handed.
    let mut right = -Vec3::from_array(scenario_object_direction_to_world(object.right_vec3()));
    let mut forward = Vec3::from_array(scenario_object_direction_to_world(object.forward_vec3()));
    if !position.is_finite() || !right.is_finite() || !forward.is_finite() {
        return None;
    }
    right = right.try_normalize()?;
    forward = (forward - right * forward.dot(right)).try_normalize()?;
    let up = forward.cross(right).try_normalize()?;
    right = up.cross(forward).try_normalize()?;
    Some(Mat4::from_cols(
        right.extend(0.0),
        up.extend(0.0),
        forward.extend(0.0),
        position.extend(1.0),
    ))
}

fn player_start_transform(start: &ScenarioPosition) -> Option<Mat4> {
    let position = Vec3::from_array(start.position_vec3());
    let authored_forward = Vec3::from_array(start.forward_vec3());
    if !position.is_finite() || !authored_forward.is_finite() {
        return None;
    }
    let forward = Vec3::new(authored_forward.x, 0.0, authored_forward.z).try_normalize()?;
    let right = Vec3::Y.cross(forward).try_normalize()?;
    Some(Mat4::from_cols(
        right.extend(0.0),
        Vec3::Y.extend(0.0),
        forward.extend(0.0),
        position.extend(1.0),
    ))
}

struct RenderedPlacement {
    transform: Mat4,
    renderer: UnitRenderer,
}

/// GPU resources for every successfully decoded scenario unit placement.
pub struct UnitSceneRenderer {
    placements: Vec<RenderedPlacement>,
}

impl UnitSceneRenderer {
    /// Uploads every placed unit and its recursive component graph.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        scene: &UnitScene,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            scene,
            WorldBindings::default(),
        )
    }

    /// Uploads all placements with a scenario-global environment fallback.
    #[must_use]
    pub fn new_with_environment(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        scene: &UnitScene,
        environment: Option<&EnvironmentMap>,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            scene,
            WorldBindings {
                environment,
                ..WorldBindings::default()
            },
        )
    }

    /// Uploads all placements with environment and directional-shadow inputs.
    #[must_use]
    pub fn new_with_environment_and_shadow(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        scene: &UnitScene,
        environment: Option<&EnvironmentMap>,
        shadow_view: Option<&wgpu::TextureView>,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            scene,
            WorldBindings {
                environment,
                directional_shadow: shadow_view,
                ..WorldBindings::default()
            },
        )
    }

    /// Uploads all placements with every scenario-global rendering input.
    #[must_use]
    pub fn new_with_world(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        scene: &UnitScene,
        world: WorldBindings<'_>,
    ) -> Self {
        let resources = RendererResources::new_with_world(device, queue, surface_format, world);
        Self::new_with_resources(device, queue, scene, &resources)
    }

    /// Uploads all placements using an existing scenario-global pipeline set.
    #[must_use]
    pub fn new_with_resources(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &UnitScene,
        resources: &RendererResources,
    ) -> Self {
        let placements = scene
            .placements
            .iter()
            .map(|placement| RenderedPlacement {
                transform: placement.transform,
                renderer: UnitRenderer::new_with_shared(
                    device,
                    queue,
                    &placement.unit,
                    placement.transform,
                    &resources.shared,
                ),
            })
            .collect();
        Self { placements }
    }

    /// Updates the camera and shared frame lighting for every placement.
    pub fn update_frame(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        lighting: &LightingParams,
    ) {
        self.update_frame_at_time(queue, view_projection, lighting, 0.0);
    }

    /// Updates frame lighting and animated legacy material UVs for every placement.
    pub fn update_frame_at_time(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
    ) {
        for placement in &mut self.placements {
            placement.renderer.update_frame_at_time(
                queue,
                view_projection,
                placement.transform,
                lighting,
                time_seconds,
            );
        }
    }

    /// Draws all placements and their recursive component graphs.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.render_phase(RenderPhase::World, pass);
    }

    /// Draws authored screen-space distortion for all scenario placements.
    pub fn render_distortion<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.render_phase(RenderPhase::Distortion, pass);
    }

    /// Draws every shadow-enabled scenario component into one cascade.
    pub fn render_shadow<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>, cascade: usize) {
        self.render_phase(RenderPhase::Shadow { cascade }, pass);
    }

    /// Returns the number of uploaded scenario placements.
    #[must_use]
    pub fn placement_count(&self) -> usize {
        self.placements.len()
    }
}

impl WorldRenderer for UnitSceneRenderer {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        if phase == RenderPhase::Sky {
            return;
        }
        for placement in &self.placements {
            placement.renderer.render_phase(phase, pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};
    use pipeline::database::hw1::ProtoObject;
    use pipeline::hw1::scenario::{ScenarioObject, ScenarioPosition};

    use super::{
        player_start_transform, prototype_is_hidden, scenario_object_direction_to_world,
        scenario_object_position_to_world, scenario_proto_name, scenario_transform,
    };

    #[test]
    fn scenario_object_axes_follow_the_xtd_storage_transpose() {
        assert!(
            Vec3::from_array(scenario_object_position_to_world([12.0, 34.0, 56.0]))
                .abs_diff_eq(Vec3::new(56.0, 34.0, 12.0), 1.0e-6)
        );
        assert!(
            Vec3::from_array(scenario_object_direction_to_world([0.0, 0.0, 1.0]))
                .abs_diff_eq(Vec3::X, 1.0e-6)
        );
    }

    #[test]
    fn scenario_object_basis_transposes_without_flipping_up() {
        let object = ScenarioObject {
            position: "12,34,56".to_owned(),
            forward: "0,0,1".to_owned(),
            right: "1,0,0".to_owned(),
            ..ScenarioObject::default()
        };
        let transform = scenario_transform(&object).expect("valid placement transform");
        let expected = Mat4::from_cols(
            (-Vec3::Z).extend(0.0),
            Vec3::Y.extend(0.0),
            Vec3::X.extend(0.0),
            Vec3::new(56.0, 34.0, 12.0).extend(1.0),
        );
        assert!(transform.abs_diff_eq(expected, 1.0e-6));
        assert!(transform.determinant() > 0.0);
    }

    #[test]
    fn player_start_uses_authored_world_position_and_horizontal_facing() {
        let start = ScenarioPosition {
            position: "178.25,0,212.125".to_owned(),
            forward: "-0.6,0.2,0.8".to_owned(),
            ..ScenarioPosition::default()
        };
        let transform = player_start_transform(&start).expect("valid player start");
        assert!(
            transform
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(178.25, 0.0, 212.125), 1.0e-6)
        );
        assert!(
            transform
                .transform_vector3(Vec3::Z)
                .abs_diff_eq(Vec3::new(-0.6, 0.0, 0.8), 1.0e-6)
        );
        assert!(
            transform
                .transform_vector3(Vec3::Y)
                .abs_diff_eq(Vec3::Y, 1.0e-6)
        );
        assert!(transform.determinant() > 0.0);
    }

    #[test]
    fn scenario_basis_is_orthonormalized_without_flipping_up() {
        let object = ScenarioObject {
            forward: "0.7071,0,0.7071".to_owned(),
            right: "0.7071,0,-0.7071".to_owned(),
            ..ScenarioObject::default()
        };
        let transform = scenario_transform(&object).expect("valid placement transform");
        assert!(
            transform
                .transform_vector3(Vec3::Y)
                .abs_diff_eq(Vec3::Y, 1.0e-5)
        );
        assert!(transform.determinant() > 0.0);
    }

    #[test]
    fn editor_name_recovers_mixed_content_proto_identity() {
        let object = ScenarioObject {
            id: 189,
            editor_name: "hook_bldg_sniperplatform_01_189".to_owned(),
            ..ScenarioObject::default()
        };
        assert_eq!(
            scenario_proto_name(&object).as_deref(),
            Some("hook_bldg_sniperplatform_01")
        );
    }

    #[test]
    fn prototype_no_render_flag_is_case_insensitive() {
        let hidden = ProtoObject {
            flags: vec!["ForceToGaiaPlayer".to_owned(), "nOrEnDeR".to_owned()],
            ..ProtoObject::default()
        };
        assert!(prototype_is_hidden(&hidden));
        assert!(!prototype_is_hidden(&ProtoObject::default()));
    }
}
