//! Scenario loading for a [`super::ScenarioSession`].
//!
//! The functions load the simulation, the game content, the sky, the environment map and the
//! unit visuals of a scenario of the installed game.

use pipeline::hw1;
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::{xtd, xtt};
use render::environment::EnvironmentMap;
use render::ugx::{Unit as UgxUnit, UnitScene as UgxUnitScene};

/// The decoded data of one scenario, before [`super::ScenarioSession::new`] builds the terrain
/// scene.
pub struct ScenarioInputs {
    /// The XTD terrain file.
    pub terrain: xtd::XtdFile,
    /// The XTT terrain texture file, if it is available.
    pub textures: Option<xtt::XttFile>,
    /// Scenario GLS and FLS constants.
    pub lightset: Option<hw1::LightSetData>,
    /// Scenario HDR environment cubemap.
    pub environment: Option<EnvironmentMap>,
    /// Authored scenario sky visual.
    pub sky: Option<UgxUnit>,
    /// Asset source of the scenario.
    pub source: Option<AssetSource<StdFileProvider>>,
    /// Authoritative game state that `sim` loads from the scenario database.
    pub simulation: Option<sim::LoadedScenario>,
    /// Database and presentation catalog layered for the scenario.
    pub content: Option<hw1::World>,
    /// Visual assets bound to the entities of the simulation world.
    pub units: Option<UgxUnitScene>,
}

impl ScenarioInputs {
    /// Loads a scenario of the installed game with the lobby `settings`.
    ///
    /// The function loads the simulation through `sim`, the scenario sky and environment map,
    /// and the visuals of all active prototypes. [`crate::game_dir`] gives the game directory.
    ///
    /// # Errors
    ///
    /// Returns a message if the simulation, the sky, the environment map or the terrain cannot be
    /// loaded.
    pub fn load(scenario_name: &str, settings: &sim::GameSettings) -> Result<Self, String> {
        log::info!("Loading scenario database and game state via sim: {scenario_name}");
        let dir = crate::game_dir();
        let dir_str = dir.to_string_lossy();
        let sim::LoadedGameScenario {
            simulation,
            mut content,
            mut source,
        } = sim::load_scenario_from_game_dir(&dir_str, scenario_name, settings)
            .map_err(|error| format!("Failed to load simulation scenario: {error}"))?;
        log_simulation_summary(&simulation);

        let sky = content
            .manifest
            .sky_ref
            .as_deref()
            .map(|reference| load_sky_unit(&mut source, reference))
            .transpose()?;
        if let (Some(reference), Some(sky)) = (&content.manifest.sky_ref, &sky) {
            log::info!(
                "Loaded scenario sky '{}': {} components, {} triangles",
                reference,
                sky.component_count(),
                sky.triangle_count(),
            );
        }
        let environment = content
            .manifest
            .terrain_env_ref
            .as_deref()
            .map(|path| EnvironmentMap::load(&mut source, path))
            .transpose()
            .map_err(|error| format!("Failed to load scenario environment: {error}"))?;
        if let Some(environment) = &environment {
            log::info!(
                "Loaded scenario environment {} ({}x{}, {} mips, HDR scale {})",
                environment.path(),
                environment.size(),
                environment.size(),
                environment.mip_count(),
                environment.hdr_scale(),
            );
        }
        let active_proto_names =
            render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
        let loaded_visuals =
            content.load_visuals_for(&mut source, active_proto_names.iter().copied());
        log::info!("Loaded {loaded_visuals} active proto visual definitions");
        let units = UgxUnitScene::load_world_with_gameplay(
            &mut source,
            &simulation.world,
            &simulation.gameplay,
            &content.visuals,
            &content.database.objects,
        );
        log_unit_scene_summary(&units);

        let terrain = content.terrain_data.take().ok_or_else(|| {
            format!("World loaded but no XTD terrain data for scenario '{scenario_name}'")
        })?;
        Ok(Self {
            terrain,
            textures: content.terrain_textures.take(),
            lightset: content.lightset.take(),
            environment,
            sky,
            source: Some(source),
            simulation: Some(simulation),
            content: Some(content),
            units: Some(units),
        })
    }
}

fn log_simulation_summary(simulation: &sim::LoadedScenario) {
    log::info!(
        "Entered sim scenario with {} players, {} initial bases, {} squads, {} units/buildings, and {} projectiles",
        simulation.world.player_count(),
        simulation.initial_base_ids.len(),
        simulation.world.squads.len(),
        simulation.world.units.len(),
        simulation.world.projectiles.len(),
    );
    for player in simulation.world.players() {
        log::info!(
            "Player {} '{}': {:?} civ {} leader {} team {} state {:?}",
            player.id,
            player.name,
            player.player_type,
            player.civ_id,
            player.leader_id,
            player.team_id,
            player.state,
        );
    }
}

fn log_unit_scene_summary(scene: &UgxUnitScene) {
    log::info!(
        "Resolved simulation UGX scene: {} placements from {} sim entities across {} visuals ({} NoRender, {} missing visuals, {} missing prototypes, {} invalid transforms, {} asset load failures)",
        scene.placement_count(),
        scene.simulation_entity_count(),
        scene.unique_visual_count(),
        scene.skipped_no_render_count(),
        scene.missing_visual_count(),
        scene.missing_proto_count(),
        scene.invalid_transform_count(),
        scene.load_failure_count(),
    );
    for issue in scene.issues() {
        log::warn!(
            "Scenario visual '{}' was skipped: {}",
            issue.proto_name(),
            issue.reason()
        );
    }
    log::info!(
        "Resolved {} PFX graphs ({} issues) and {} LGT graphs ({} issues)",
        scene.particle_effect_count(),
        scene.particle_effect_issue_count(),
        scene.light_effect_count(),
        scene.light_effect_issue_count(),
    );
    for issue in scene.light_effect_issues() {
        log::warn!("Scenario LGT attachment was skipped: {issue}");
    }
}

fn load_sky_unit(
    source: &mut AssetSource<StdFileProvider>,
    reference: &str,
) -> Result<UgxUnit, String> {
    let path = canonical_sky_visual_path(reference);
    let document = source
        .read_xmb(&path)
        .ok_or_else(|| format!("scenario sky visual was not found: {path}"))?;
    let visual = pipeline::database::hw1::visual::parse(&document)
        .map_err(|error| format!("failed to parse scenario sky visual '{path}': {error}"))?;
    UgxUnit::load(source, &visual)
        .map_err(|error| format!("failed to load scenario sky visual '{path}': {error}"))
}

fn canonical_sky_visual_path(reference: &str) -> String {
    let mut normalized = reference
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    if normalized.to_ascii_lowercase().ends_with(".xmb") {
        normalized.truncate(normalized.len() - 4);
    }
    if !normalized.to_ascii_lowercase().ends_with(".vis") {
        normalized.push_str(".vis");
    }
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_sky_visual_path;

    #[test]
    fn sky_visual_path_gets_the_art_folder_and_the_vis_extension() {
        assert_eq!(
            canonical_sky_visual_path("environment/sky/sky_bg"),
            "art\\environment\\sky\\sky_bg.vis"
        );
        assert_eq!(
            canonical_sky_visual_path(" \\Art\\environment\\sky_bg.vis.xmb "),
            "Art\\environment\\sky_bg.vis"
        );
    }
}
