//! Presentation assets for UGX units bound to authoritative simulation entities.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat4, Vec3};
use pipeline::database::hw1::{ProtoObject, Visual};
use pipeline::source::{AssetSource, StdFileProvider};
use sim::{EntityId, World as SimWorld};

use super::Unit;
use super::unit::UnitAssetCache;

mod renderer;

pub use renderer::UnitSceneRenderer;
pub use sim::{
    ScenarioPositionAxes, scenario_object_direction_to_world, scenario_object_position_to_world,
};

/// One decoded visual bound to an entity in the simulation world.
#[derive(Clone, Debug)]
pub struct UnitPlacement {
    entity_id: EntityId,
    proto_name: String,
    transform: Mat4,
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

    /// Return the simulation transform captured while building the scene.
    ///
    /// [`UnitSceneRenderer`] refreshes this from the live sim world every frame.
    #[must_use]
    pub const fn transform(&self) -> Mat4 {
        self.transform
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
    /// Asset decoding is cached by proto name. The resulting placements retain
    /// only entity IDs; the GPU renderer fetches current transforms from
    /// [`SimWorld`] every frame.
    #[must_use]
    pub fn load_world(
        source: &mut AssetSource<StdFileProvider>,
        world: &SimWorld,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
    ) -> Self {
        let mut asset_cache = UnitAssetCache::default();
        let mut scene = Self {
            simulation_entity_count: world.units.len(),
            ..Self::default()
        };
        let visual_names = visual_name_lookup(visuals);
        let no_render_objects = hidden_proto_names(proto_objects);
        let mut units = HashMap::<String, Option<Arc<Unit>>>::new();

        for (entity_id, entity) in world.units.iter() {
            let proto_name = entity.proto_object_name.trim();
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
            let Some(transform) = simulation_unit_transform(entity) else {
                scene.invalid_transform_count += 1;
                continue;
            };
            let unit = match load_cached_unit(
                source,
                &visuals[visual_name],
                &lookup_name,
                &mut asset_cache,
                &mut units,
            ) {
                Ok((unit, newly_loaded)) => {
                    scene.unique_visual_count += usize::from(newly_loaded);
                    unit
                }
                Err(reason) => {
                    if let Some(reason) = reason {
                        scene.issues.push(UnitSceneIssue {
                            proto_name: proto_name.to_owned(),
                            reason,
                        });
                    }
                    scene.load_failure_count += 1;
                    continue;
                }
            };
            scene.placements.push(UnitPlacement {
                entity_id,
                proto_name: proto_name.to_owned(),
                transform,
                unit,
            });
        }
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

    /// Return the number of unit-pool entities considered from the sim world.
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
    asset_cache: &mut UnitAssetCache,
    units: &mut HashMap<String, Option<Arc<Unit>>>,
) -> Result<(Arc<Unit>, bool), Option<String>> {
    if let Some(cached) = units.get(lookup_name) {
        return cached.clone().map(|unit| (unit, false)).ok_or(None);
    }
    match Unit::load_variant_with_cache(source, visual, None, asset_cache) {
        Ok(unit) => {
            let unit = Arc::new(unit);
            units.insert(lookup_name.to_owned(), Some(Arc::clone(&unit)));
            Ok((unit, true))
        }
        Err(error) => {
            units.insert(lookup_name.to_owned(), None);
            Err(Some(error.to_string()))
        }
    }
}

fn prototype_is_hidden(proto: &ProtoObject) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("NoRender"))
}

/// Build a model-to-world matrix solely from authoritative simulation state.
#[must_use]
pub fn simulation_unit_transform(unit: &sim::Unit) -> Option<Mat4> {
    let position = unit.base.position;
    let world_forward = unit.base.forward;
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

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use pipeline::database::hw1::ProtoObject;

    use super::{prototype_is_hidden, simulation_unit_transform};

    #[test]
    fn simulation_transform_uses_only_live_position_and_facing() {
        let mut world = sim::World::new();
        let entity_id = world.create_unit_at(0, Vec3::new(12.0, 3.0, 56.0));
        let unit = world.get_unit_mut(entity_id).expect("new unit");
        unit.base.set_forward(Vec3::X);

        let transform = simulation_unit_transform(unit).expect("valid transform");
        assert!(
            transform
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(12.0, 3.0, 56.0), 1.0e-6)
        );
        assert!(
            transform
                .transform_vector3(Vec3::Z)
                .abs_diff_eq(Vec3::X, 1.0e-6)
        );
        assert!(transform.determinant() > 0.0);
    }

    #[test]
    fn invalid_simulation_facing_has_no_render_transform() {
        let mut world = sim::World::new();
        let entity_id = world.create_unit_at(0, Vec3::ZERO);
        let unit = world.get_unit_mut(entity_id).expect("new unit");
        unit.base.forward = Vec3::ZERO;

        assert!(simulation_unit_transform(unit).is_none());
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
