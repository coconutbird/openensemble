//! Scenario session that the executables share.
//!
//! A [`ScenarioSession`] holds one loaded scenario: the asset source, the authoritative
//! simulation, the game content, the terrain scene and the unit visuals. It steps the
//! simulation, keeps the unit visuals in step with the simulation world and makes the
//! [`WorldView`] that draws the scenario.
//!
//! [`ScenarioInputs`] holds the decoded data of a scenario before the session builds the terrain
//! scene. [`ScenarioInputs::load`] loads a scenario of the installed game.

mod inputs;

use num_traits::ToPrimitive;
use pipeline::hw1;
use pipeline::source::{AssetSource, StdFileProvider};
use render::environment::EnvironmentMap;
use render::lighting::LocalLightView;
use render::particle::ParticleScene;
use render::terrain::{Camera, SimulationCameraAdapter, TerrainScene};
use render::ugx::{AnimationCameraShake, Unit as UgxUnit, UnitScene as UgxUnitScene};
use render::wgpu;
use render::world_view::{
    WorldCamera, WorldView, WorldViewError, WorldViewInputs, WorldViewSettings, WorldViewUnits,
};

pub use inputs::ScenarioInputs;

/// One loaded scenario and its authoritative simulation.
///
/// A session from a legacy `.xtd` file has a terrain scene but no simulation, no game content and
/// no unit visuals.
pub struct ScenarioSession {
    /// Asset source of the scenario. `WorldView` loads the road textures from it.
    pub source: Option<AssetSource<StdFileProvider>>,
    /// Authoritative game state that `sim` loads from the scenario database.
    pub simulation: Option<sim::LoadedScenario>,
    /// Database and presentation catalog layered for the scenario.
    pub content: Option<hw1::World>,
    /// Fixed-step clock and command queue that advance the authoritative world.
    pub clock: sim::Simulation,
    /// All decoded terrain data (mesh, textures, splat, decals, foliage, roads).
    pub scene: TerrainScene,
    /// Scenario GLS and FLS constants. [`Self::create_world_view`] moves them to the `WorldView`.
    pub lightset: Option<hw1::LightSetData>,
    /// Scenario HDR environment cubemap for reflective materials.
    pub environment: Option<EnvironmentMap>,
    /// Authored scenario sky visual.
    pub sky: Option<UgxUnit>,
    /// Visual assets bound to the entities of the simulation world.
    pub units: Option<UgxUnitScene>,
    /// GPU resources that draw the scenario. [`Self::create_world_view`] makes them.
    pub world_view: Option<WorldView>,
    /// If true, the `WorldView` must get the unit placements from `units` again.
    units_dirty: bool,
    /// Game time of the last trigger loop warning. The log gets one warning per second at most.
    trigger_loop_warning_time_ms: Option<u32>,
}

impl ScenarioSession {
    /// Builds the terrain scene and starts the simulation clock if the scenario has a simulation.
    ///
    /// # Errors
    ///
    /// Returns the message of the terrain scene load error.
    pub fn new(inputs: ScenarioInputs) -> Result<Self, String> {
        let ScenarioInputs {
            terrain,
            textures,
            lightset,
            environment,
            sky,
            mut source,
            simulation,
            content,
            units,
        } = inputs;
        let scene = TerrainScene::load(&terrain, textures.as_ref(), source.as_mut())?;
        let mut clock = sim::Simulation::new();
        if simulation.is_some() {
            clock.start();
        }
        Ok(Self {
            source,
            simulation,
            content,
            clock,
            scene,
            lightset,
            environment,
            sky,
            units,
            world_view: None,
            units_dirty: false,
            trigger_loop_warning_time_ms: None,
        })
    }

    /// Returns the authoritative simulation world, if the scenario has one.
    #[must_use]
    pub fn world(&self) -> Option<&sim::World> {
        self.simulation.as_ref().map(|simulation| &simulation.world)
    }

    /// Advances the simulation by the frame time and updates the unit visuals.
    ///
    /// The function logs a warning when a trigger script loops in one update. When the simulation
    /// adds or removes entities, the function loads the new visuals and marks the `WorldView`
    /// unit placements for [`Self::sync_world_view_units`].
    pub fn advance(&mut self, dt_seconds: f32) {
        let Self {
            simulation: Some(simulation),
            content: Some(content),
            clock,
            source: Some(source),
            units: Some(units),
            units_dirty,
            trigger_loop_warning_time_ms,
            ..
        } = self
        else {
            return;
        };
        clock.update_with_scenario(dt_seconds, simulation, &content.database);
        warn_about_trigger_loops(&simulation.world, trigger_loop_warning_time_ms);
        if units.roster_matches_with_gameplay(&simulation.world, &simulation.gameplay) {
            return;
        }
        let active_proto_names =
            render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
        let loaded_visuals = content.load_visuals_for(source, active_proto_names.iter().copied());
        if loaded_visuals > 0 {
            log::info!("Loaded {loaded_visuals} visuals for the updated sim roster");
        }
        *units_dirty |= units.sync_world_with_gameplay(
            source,
            &simulation.world,
            &simulation.gameplay,
            &content.visuals,
            &content.database.objects,
        );
    }

    /// Makes the `WorldView` that draws the scenario, if it does not exist.
    ///
    /// The function moves the lightset to the `WorldView`. After the upload, it releases the
    /// decoded XTT albedo, because the GPU has the composited atlas.
    ///
    /// # Errors
    ///
    /// Returns the `WorldView` error if the GPU resources cannot be made.
    pub fn create_world_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        output_format: wgpu::TextureFormat,
        settings: &WorldViewSettings<'_>,
    ) -> Result<(), WorldViewError> {
        if self.world_view.is_some() {
            return Ok(());
        }
        let units = match (&self.units, &self.simulation) {
            (Some(scene), Some(simulation)) => Some(WorldViewUnits {
                scene,
                world: &simulation.world,
            }),
            _ => None,
        };
        let inputs = WorldViewInputs {
            terrain: &self.scene,
            lightset: self.lightset.take(),
            environment: self.environment.as_ref(),
            sky: self.sky.as_ref(),
            units,
            asset_source: self.source.as_mut(),
            size,
            output_format,
        };
        let world_view = WorldView::new(device, queue, inputs, settings)?;
        self.world_view = Some(world_view);
        self.scene.albedo = None;
        Ok(())
    }

    /// Gives the changed unit placements to the `WorldView` after [`Self::advance`] marked them.
    pub fn sync_world_view_units(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.units_dirty {
            return;
        }
        if let (Some(world_view), Some(units), Some(simulation)) =
            (&mut self.world_view, &self.units, &self.simulation)
        {
            world_view.sync_units(device, queue, units, &simulation.world);
        }
        self.units_dirty = false;
    }

    /// Recreates the `WorldView` resources that depend on the window size.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if let Some(world_view) = &mut self.world_view {
            world_view.resize(device, width, height);
        }
    }
}

/// Returns the `WorldView` camera of an interactive camera for an output of `size` pixels.
///
/// # Panics
///
/// Panics if a dimension of `size` does not fit in `f32`.
#[must_use]
pub fn world_camera(camera: &Camera, size: (u32, u32)) -> WorldCamera {
    let width = size.0.to_f32().expect("surface width must fit f32");
    let height = size.1.to_f32().expect("surface height must fit f32");
    let world_to_view = camera.view_matrix();
    let projection = camera.projection_matrix(width / height);
    WorldCamera {
        view_projection: projection * world_to_view,
        world_to_view,
        position: camera.position,
        depth_unproject: ParticleScene::perspective_depth_unproject(projection),
        viewport: [size.0, size.1],
    }
}

/// Starts the visible animation camera shakes on the camera adapter.
///
/// The executables do not give the selection to this function. Retail suppresses the two
/// shipped `checkSelected` tags unless their owner is selected, so the function always
/// suppresses them.
pub fn begin_camera_shakes(
    adapter: &mut SimulationCameraAdapter,
    view: LocalLightView,
    shakes: Vec<AnimationCameraShake>,
    render_time_seconds: f32,
) {
    for shake in shakes {
        if shake.check_selected()
            || view
                .project_sphere(shake.position().to_array(), 1.0)
                .is_none()
        {
            continue;
        }
        adapter.begin_animation_shake(
            shake.duration_seconds(),
            shake.strength(),
            render_time_seconds,
        );
    }
}

/// Logs a warning when the last trigger update crossed the evaluation warning limit.
///
/// A script that activates its own triggers again in one update (for example because an effect
/// that it needs is not implemented) evaluates up to the loop guard in each tick. That makes each
/// tick slow. The warning names the cause, at most one time for each second of game time.
fn warn_about_trigger_loops(world: &sim::World, last_warning_ms: &mut Option<u32>) {
    if !world.trigger_engine().performance_warning_reached() {
        return;
    }
    let now = world.game_time_ms;
    if last_warning_ms.is_some_and(|last| now.saturating_sub(last) < 1_000) {
        return;
    }
    *last_warning_ms = Some(now);
    log::warn!(
        "Trigger update at {now} ms evaluated more conditions than the warning limit; a trigger \
         script loops in one update"
    );
}
