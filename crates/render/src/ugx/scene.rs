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
    decoded_visuals: HashMap<String, Option<Arc<Unit>>>,
    simulation_entity_ids: Vec<EntityId>,
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
        Self::load_world_reusing(source, world, visuals, proto_objects, HashMap::new())
    }

    /// Refresh presentation assets when the authoritative entity roster changes.
    ///
    /// Existing decoded visuals are retained by prototype name, so spawning
    /// another instance normally allocates only per-instance presentation data.
    /// Returns whether the roster changed.
    pub fn sync_world(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        world: &SimWorld,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
    ) -> bool {
        if self.roster_matches(world) {
            return false;
        }
        let retained_units = self.decoded_visuals.clone();
        *self = Self::load_world_reusing(source, world, visuals, proto_objects, retained_units);
        true
    }

    /// Return whether this presentation roster matches all live sim visuals.
    #[must_use]
    pub fn roster_matches(&self, world: &SimWorld) -> bool {
        self.simulation_entity_ids
            .iter()
            .copied()
            .eq(simulation_entity_ids(world))
    }

    fn load_world_reusing(
        source: &mut AssetSource<StdFileProvider>,
        world: &SimWorld,
        visuals: &HashMap<String, Visual>,
        proto_objects: &[ProtoObject],
        mut units: HashMap<String, Option<Arc<Unit>>>,
    ) -> Self {
        let mut asset_cache = UnitAssetCache::default();
        let simulation_entity_ids = simulation_entity_ids(world).collect::<Vec<_>>();
        let mut scene = Self {
            simulation_entity_count: simulation_entity_ids.len(),
            simulation_entity_ids,
            ..Self::default()
        };
        let visual_names = visual_name_lookup(visuals);
        let no_render_objects = hidden_proto_names(proto_objects);
        let mut used_visuals = HashSet::new();

        for entity in simulation_visuals(world) {
            let entity_id = entity.id;
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
            let Some(transform) = entity.transform else {
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
                Ok(unit) => unit,
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
            used_visuals.insert(lookup_name);
        }
        scene.unique_visual_count = used_visuals.len();
        scene.decoded_visuals = units
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
) -> Result<Arc<Unit>, Option<String>> {
    if let Some(cached) = units.get(lookup_name) {
        return cached.clone().ok_or(None);
    }
    match Unit::load_variant_with_cache(source, visual, None, asset_cache) {
        Ok(unit) => {
            let unit = Arc::new(unit);
            units.insert(lookup_name.to_owned(), Some(Arc::clone(&unit)));
            Ok(unit)
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

struct SimulationVisual<'a> {
    id: EntityId,
    proto_name: &'a str,
    transform: Option<Mat4>,
}

fn simulation_entity_ids(world: &SimWorld) -> impl Iterator<Item = EntityId> + '_ {
    world.units.ids().chain(world.projectiles.ids())
}

fn simulation_visuals(world: &SimWorld) -> impl Iterator<Item = SimulationVisual<'_>> {
    world
        .units
        .iter()
        .map(|(id, unit)| SimulationVisual {
            id,
            proto_name: &unit.proto_object_name,
            transform: simulation_unit_model_transform(unit),
        })
        .chain(
            world
                .projectiles
                .iter()
                .map(|(id, projectile)| SimulationVisual {
                    id,
                    proto_name: &projectile.proto_object_name,
                    transform: simulation_projectile_transform(projectile),
                }),
        )
}

/// Resolve a render transform for any sim entity represented by this scene.
#[must_use]
pub fn simulation_entity_transform(world: &SimWorld, entity_id: EntityId) -> Option<Mat4> {
    world
        .get_unit(entity_id)
        .and_then(simulation_unit_transform)
        .or_else(|| {
            world
                .get_projectile(entity_id)
                .and_then(simulation_projectile_transform)
        })
}

/// Build a model-to-world matrix solely from authoritative simulation state.
#[must_use]
pub fn simulation_unit_transform(unit: &sim::Unit) -> Option<Mat4> {
    if unit.is_garrisoned() {
        return None;
    }
    simulation_unit_model_transform(unit)
}

fn simulation_unit_model_transform(unit: &sim::Unit) -> Option<Mat4> {
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

/// Build a projectile transform including authored vertical flight direction.
#[must_use]
pub fn simulation_projectile_transform(projectile: &sim::Projectile) -> Option<Mat4> {
    let position = projectile.base.position;
    let forward = projectile.base.forward;
    if !position.is_finite() || !forward.is_finite() {
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
    Some(Mat4::from_cols(
        right.extend(0.0),
        up.extend(0.0),
        forward.extend(0.0),
        position.extend(1.0),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use glam::Vec3;
    use pipeline::database::hw1::ProtoObject;
    use pipeline::source::{AssetSource, StdFileProvider};

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
    fn containment_state_alone_controls_simulation_visibility() {
        let mut world = sim::World::new();
        world.init_players(1);
        let container_squad = world.create_squad_at(0, Vec3::ZERO);
        let container_unit = world.create_building_at(0, Vec3::ZERO);
        assert!(world.attach_unit_to_squad(container_unit, container_squad));
        world.get_unit_mut(container_unit).unwrap().garrison =
            sim::UnitGarrison::container(0.0, false, false, Vec::new());
        let passenger_squad = world.create_squad_at(1, Vec3::ZERO);
        let passenger_unit = world.create_unit_at(1, Vec3::ZERO);
        assert!(world.attach_unit_to_squad(passenger_unit, passenger_squad));

        world
            .issue_garrison_order(1, passenger_squad, container_squad, 0.0)
            .expect("garrison command");
        world.advance_time(50);
        world.update_entities(0.05);
        assert!(world.get_unit(passenger_unit).unwrap().is_garrisoned());
        assert!(simulation_unit_transform(world.get_unit(passenger_unit).unwrap()).is_none());

        world
            .issue_ungarrison_order(1, passenger_squad, None)
            .expect("ungarrison command");
        assert!(simulation_unit_transform(world.get_unit(passenger_unit).unwrap()).is_none());
        world.advance_time(50);
        world.update_entities(0.05);
        assert!(simulation_unit_transform(world.get_unit(passenger_unit).unwrap()).is_some());
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

    #[test]
    fn roster_matching_tracks_generational_sim_entity_ids() {
        let mut world = sim::World::new();
        let first = world.create_unit(0);
        let mut scene = super::UnitScene {
            simulation_entity_ids: vec![first],
            simulation_entity_count: 1,
            ..super::UnitScene::default()
        };
        assert!(scene.roster_matches(&world));

        world.remove_unit(first).expect("first unit");
        let replacement = world.create_unit(0);
        assert_ne!(first, replacement);
        assert!(!scene.roster_matches(&world));

        scene.simulation_entity_ids = vec![replacement];
        assert!(scene.roster_matches(&world));
    }

    #[test]
    fn scene_sync_refreshes_roster_once_per_sim_change() {
        let mut source = AssetSource::with_provider(StdFileProvider);
        let mut world = sim::World::new();
        let visuals = HashMap::new();
        let proto_objects = Vec::new();
        let mut scene = super::UnitScene::load_world(&mut source, &world, &visuals, &proto_objects);
        assert!(!scene.sync_world(&mut source, &world, &visuals, &proto_objects));

        let entity_id = world.create_unit(0);
        world
            .get_unit_mut(entity_id)
            .expect("unit")
            .proto_object_name = "missing_visual".to_owned();
        assert!(scene.sync_world(&mut source, &world, &visuals, &proto_objects));
        assert_eq!(scene.simulation_entity_count(), 1);
        assert_eq!(scene.placement_count(), 0);
        assert!(!scene.sync_world(&mut source, &world, &visuals, &proto_objects));
    }
}
