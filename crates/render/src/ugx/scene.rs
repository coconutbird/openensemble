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

/// Horizontal axis convention used by scenario `<Position>` records.
///
/// Persistent `<Object>` records always use the XTD storage transpose. Player
/// positions vary between maps, so nearby transposed `sys_unitstart` object
/// markers provide map-local evidence for their convention.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScenarioPositionAxes {
    /// Position X/Z values are already expressed in terrain-world order.
    #[default]
    AuthoredWorld,
    /// Position X/Z values use the diagonally transposed terrain storage order.
    Transposed,
}

impl ScenarioPositionAxes {
    /// Infers the player-position convention from unit-start object markers.
    ///
    /// Maps without usable marker evidence retain the direct convention used
    /// by Blood Gulch and the original player-start implementation.
    #[must_use]
    pub fn infer(scenario: &ScenarioData, max_players: Option<u32>) -> Self {
        let mut authored_score = 0.0;
        let mut transposed_score = 0.0;
        let mut evidence_count = 0_u32;

        for start in scenario
            .positions()
            .iter()
            .filter(|start| is_player_start(start, max_players))
        {
            let authored = Self::AuthoredWorld.position_to_world(start.position_vec3());
            let transposed = Self::Transposed.position_to_world(start.position_vec3());
            let Some(authored_distance) = nearest_unit_start_distance_squared(scenario, authored)
            else {
                continue;
            };
            let Some(transposed_distance) =
                nearest_unit_start_distance_squared(scenario, transposed)
            else {
                continue;
            };
            authored_score += authored_distance;
            transposed_score += transposed_distance;
            evidence_count += 1;
        }

        if evidence_count > 0 && transposed_score < authored_score {
            Self::Transposed
        } else {
            Self::AuthoredWorld
        }
    }

    /// Converts a scenario position into terrain-world coordinates.
    #[must_use]
    pub fn position_to_world(self, position: [f32; 3]) -> [f32; 3] {
        match self {
            Self::AuthoredWorld => position,
            Self::Transposed => [position[2], position[1], position[0]],
        }
    }

    /// Converts a scenario position direction into terrain-world axes.
    #[must_use]
    pub fn direction_to_world(self, direction: [f32; 3]) -> [f32; 3] {
        self.position_to_world(direction)
    }
}

#[derive(Clone, Copy)]
struct PlayerStartLayout {
    max_players: Option<u32>,
    axes: ScenarioPositionAxes,
}

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
    /// present in `<Objects>`. `max_players` limits numbered positions using
    /// the active scenario descriptor and excludes editor-only extra positions.
    #[must_use]
    pub fn load_scenario(
        source: &mut AssetSource<StdFileProvider>,
        scenario: &ScenarioData,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
        max_players: Option<u32>,
        player_start_proto: &str,
    ) -> Self {
        let mut cache = UnitAssetCache::default();
        let position_axes = ScenarioPositionAxes::infer(scenario, max_players);
        let mut scene = Self::load_with_cache(
            source,
            scenario.objects(),
            visuals,
            proto_objects,
            &mut cache,
        );
        scene.player_start_count = scenario
            .positions()
            .iter()
            .filter(|start| is_player_start(start, max_players))
            .count();
        scene.append_player_starts(
            source,
            scenario.positions(),
            visuals,
            PlayerStartLayout {
                max_players,
                axes: position_axes,
            },
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
        layout: PlayerStartLayout,
        proto_name: &str,
        asset_cache: &mut UnitAssetCache,
    ) {
        let start_count = starts
            .iter()
            .filter(|start| is_player_start(start, layout.max_players))
            .count();
        if start_count == 0 {
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
                self.player_start_failure_count = start_count;
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
                    self.player_start_failure_count = start_count;
                    self.issues.push(UnitSceneIssue {
                        proto_name: visual_name.clone(),
                        reason: error.to_string(),
                    });
                    return;
                }
            }
        };

        for start in starts
            .iter()
            .filter(|start| is_player_start(start, layout.max_players))
        {
            let Some(transform) = player_start_transform(start, layout.axes) else {
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

fn is_player_start(start: &ScenarioPosition, max_players: Option<u32>) -> bool {
    u32::try_from(start.number)
        .is_ok_and(|number| number > 0 && max_players.is_none_or(|maximum| number <= maximum))
}

fn nearest_unit_start_distance_squared(
    scenario: &ScenarioData,
    player_position: [f32; 3],
) -> Option<f32> {
    scenario
        .objects()
        .iter()
        .filter(|object| {
            scenario_proto_name(object)
                .is_some_and(|name| name.to_ascii_lowercase().starts_with("sys_unitstart"))
        })
        .map(|marker| {
            let position = scenario_object_position_to_world(marker.position_vec3());
            let delta_x = position[0] - player_position[0];
            let delta_z = position[2] - player_position[2];
            delta_x.mul_add(delta_x, delta_z * delta_z)
        })
        .filter(|distance| distance.is_finite())
        .min_by(f32::total_cmp)
}

/// Converts an authored SCN `<Object>` position into terrain-world coordinates.
///
/// Object records use the same diagonally transposed X/Z storage axes as the
/// XTD heightfield.
#[must_use]
pub fn scenario_object_position_to_world(position: [f32; 3]) -> [f32; 3] {
    [position[2], position[1], position[0]]
}

/// Converts an authored SCN `<Object>` direction into terrain-world axes.
#[must_use]
pub fn scenario_object_direction_to_world(direction: [f32; 3]) -> [f32; 3] {
    scenario_object_position_to_world(direction)
}

fn scenario_transform(object: &ScenarioObject) -> Option<Mat4> {
    let position = Vec3::from_array(scenario_object_position_to_world(object.position_vec3()));
    let mut right = -Vec3::from_array(scenario_object_direction_to_world(object.right_vec3()));
    let mut forward = Vec3::from_array(scenario_object_direction_to_world(object.forward_vec3()));
    if !position.is_finite() || !right.is_finite() || !forward.is_finite() {
        return None;
    }
    // Swapping X/Z reverses parity. Negating the right axis retains authored
    // facing while keeping the rendered unit upright and right-handed.
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

fn player_start_transform(
    start: &ScenarioPosition,
    position_axes: ScenarioPositionAxes,
) -> Option<Mat4> {
    let position = Vec3::from_array(position_axes.position_to_world(start.position_vec3()));
    let world_forward = Vec3::from_array(position_axes.direction_to_world(start.forward_vec3()));
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
    use pipeline::hw1::scenario::{
        ObjectsWrapper, PositionsWrapper, ScenarioData, ScenarioObject, ScenarioPosition,
    };

    use super::{
        ScenarioPositionAxes, is_player_start, player_start_transform, prototype_is_hidden,
        scenario_object_direction_to_world, scenario_object_position_to_world, scenario_proto_name,
        scenario_transform,
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
    fn unit_start_evidence_transposes_tundra_player_positions() {
        let scenario = scenario_with_axis_evidence(
            &[
                ("181.5322,-1.0026,214.2044", "sys_unitstart_01"),
                ("710.5275,-1.0232,682.3585", "sys_unitstart_01"),
            ],
            &["114.7806,-0.4714,222.1855", "777.4794,-0.6362,668.1328"],
        );

        assert_eq!(
            ScenarioPositionAxes::infer(&scenario, Some(2)),
            ScenarioPositionAxes::Transposed
        );
    }

    #[test]
    fn direct_player_positions_remain_direct_when_marker_evidence_matches() {
        let scenario =
            scenario_with_axis_evidence(&[("20,0,80", "sys_unitstart_01")], &["80,0,20"]);

        assert_eq!(
            ScenarioPositionAxes::infer(&scenario, Some(1)),
            ScenarioPositionAxes::AuthoredWorld
        );
    }

    #[test]
    fn scenarios_without_marker_evidence_keep_direct_player_positions() {
        assert_eq!(
            ScenarioPositionAxes::infer(&ScenarioData::default(), None),
            ScenarioPositionAxes::AuthoredWorld
        );
    }

    #[test]
    fn player_start_selection_uses_the_scenario_player_limit() {
        let start = ScenarioPosition {
            number: 3,
            default_camera: true,
            ..ScenarioPosition::default()
        };
        assert!(is_player_start(&start, None));
        assert!(!is_player_start(&start, Some(2)));

        let start = ScenarioPosition { number: 2, ..start };
        assert!(is_player_start(&start, Some(2)));
        assert!(!is_player_start(&ScenarioPosition::default(), None));
    }

    #[test]
    fn player_start_uses_authored_world_position_and_horizontal_facing() {
        let start = ScenarioPosition {
            position: "178.25,0,212.125".to_owned(),
            forward: "-0.6,0.2,0.8".to_owned(),
            ..ScenarioPosition::default()
        };
        let transform = player_start_transform(&start, ScenarioPositionAxes::AuthoredWorld)
            .expect("valid player start");
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
    fn transposed_player_start_converts_position_and_facing() {
        let start = ScenarioPosition {
            position: "178.25,0,212.125".to_owned(),
            forward: "-0.6,0.2,0.8".to_owned(),
            ..ScenarioPosition::default()
        };
        let transform = player_start_transform(&start, ScenarioPositionAxes::Transposed)
            .expect("valid player start");
        assert!(
            transform
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(212.125, 0.0, 178.25), 1.0e-6)
        );
        assert!(
            transform
                .transform_vector3(Vec3::Z)
                .abs_diff_eq(Vec3::new(0.8, 0.0, -0.6), 1.0e-6)
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

    fn scenario_with_axis_evidence(objects: &[(&str, &str)], positions: &[&str]) -> ScenarioData {
        ScenarioData {
            objects: Some(ObjectsWrapper {
                entries: objects
                    .iter()
                    .enumerate()
                    .map(|(index, (position, proto_name))| ScenarioObject {
                        id: i32::try_from(index).expect("test object index fits i32"),
                        proto_name: (*proto_name).to_owned(),
                        position: (*position).to_owned(),
                        ..ScenarioObject::default()
                    })
                    .collect(),
            }),
            positions: Some(PositionsWrapper {
                entries: positions
                    .iter()
                    .enumerate()
                    .map(|(index, position)| ScenarioPosition {
                        number: i32::try_from(index + 1).expect("test position index fits i32"),
                        position: (*position).to_owned(),
                        ..ScenarioPosition::default()
                    })
                    .collect(),
            }),
            ..ScenarioData::default()
        }
    }
}
