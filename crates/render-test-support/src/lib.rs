//! Scene helpers for the renderer integration tests.
//!
//! Use this crate only as a dev-dependency of `render`. Keep the setup and the expectations of
//! each test in the test file.

use pipeline::source::{AssetSource, StdFileProvider};
use render::ugx::{UnitPlacement, UnitScene, simulation_proto_names};
use sim::EntityId;

/// Load the visuals of each prototype that the sim world shows.
///
/// The function uses [`simulation_proto_names`], so it loads the same prototypes that
/// [`UnitScene`] projects.
pub fn load_active_visuals(
    content: &mut pipeline::hw1::World,
    source: &mut AssetSource<StdFileProvider>,
    world: &sim::World,
) {
    let active = simulation_proto_names(world).collect::<Vec<_>>();
    content.load_visuals_for(source, active.iter().copied());
}

/// Load the active visuals, then synchronize `scene` with the sim world.
///
/// # Panics
///
/// Panics if the synchronization fails or if the scene roster does not match the sim world.
pub fn sync_scene(
    scene: &mut UnitScene,
    content: &mut pipeline::hw1::World,
    source: &mut AssetSource<StdFileProvider>,
    world: &sim::World,
) {
    load_active_visuals(content, source, world);
    assert!(scene.sync_world(source, world, &content.visuals, &content.database.objects));
    assert!(scene.roster_matches(world));
}

/// Find the scene placement of a sim entity.
#[must_use]
pub fn placement(scene: &UnitScene, entity_id: EntityId) -> Option<&UnitPlacement> {
    scene
        .placements()
        .iter()
        .find(|placement| placement.entity_id() == entity_id)
}

/// Assert that the scene projects `entity_id` with the visual of `prototype`.
///
/// # Panics
///
/// Panics if the entity has no placement or if its placement uses a different prototype. The
/// message contains the scene issues.
pub fn assert_visual_loaded(scene: &UnitScene, entity_id: EntityId, prototype: &str) {
    assert!(
        placement(scene, entity_id).is_some_and(|placement| placement.proto_name() == prototype),
        "{prototype} was not projected; issues: {:?}",
        scene
            .issues()
            .iter()
            .map(|issue| (issue.proto_name(), issue.reason()))
            .collect::<Vec<_>>()
    );
}
