//! Drawing of a loaded scenario.
//!
//! `WorldView` owns all GPU resources that draw one scenario: the tessellated terrain, the
//! terrain compositor, the directional and local shadows, the foliage, the roads, the sky, the
//! units with their particles, impacts and lights, and the tone mapping.
//!
//! Use it in this sequence:
//!
//! 1. Make a `WorldView` with `WorldView::new` after the scenario data is loaded.
//! 2. For each frame, call `WorldView::update`, then `WorldView::encode`.
//! 3. When the simulation entity set changes, call `WorldView::sync_units`.
//! 4. When the output size changes, call `WorldView::resize`.
//!
//! `WorldView::render_offscreen` draws one frame into a different target, with its own size and
//! depth. Call `WorldView::end_offscreen` after the last offscreen frame.

mod dynamic_alpha;
mod foliage;
mod frame;
mod packed;
mod passes;
mod roads;
mod shadow;
mod terrain;
mod units;

use glam::{Mat4, Vec3};
use pipeline::hw1::LightSetData;
use pipeline::source::{AssetSource, StdFileProvider};

use crate::environment::EnvironmentMap;
use crate::lighting::{LocalLightSet, LocalLightView};
use crate::particle::ParticleSceneTextures;
use crate::postprocess::{HDR_COLOR_FORMAT, ToneMapResources};
use crate::terrain::{CompositorResources, LodConfig, TerrainScene};
use crate::ugx::{Unit, UnitRenderer, UnitScene, UnitSceneRenderer};
use crate::wgpu;
use foliage::FoliageResources;
use roads::RoadResources;
use shadow::ShadowResources;
use terrain::{TerrainBuild, TerrainChunkGrid, TerrainResources};

/// An error that prevents the creation of a `WorldView`.
#[derive(Debug, thiserror::Error)]
pub enum WorldViewError {
    /// The terrain scene has no packed XTD data.
    #[error("Packed XTD terrain data is required by the GPU terrain renderer")]
    MissingPackedTerrain,
}

/// The unit visuals of a scenario and the simulation world that places them.
#[derive(Clone, Copy)]
pub struct WorldViewUnits<'a> {
    /// Visual assets bound to the simulation entities.
    pub scene: &'a UnitScene,
    /// Authoritative simulation world.
    pub world: &'a sim::World,
}

/// The loaded scenario data that `WorldView::new` uploads to the GPU.
pub struct WorldViewInputs<'a> {
    /// Decoded terrain data. The terrain must contain the packed XTD data.
    pub terrain: &'a TerrainScene,
    /// Scenario GLS and FLS constants.
    pub lightset: Option<LightSetData>,
    /// Scenario HDR environment cubemap.
    pub environment: Option<&'a EnvironmentMap>,
    /// Scenario sky visual.
    pub sky: Option<&'a Unit>,
    /// Unit visuals and the simulation world.
    pub units: Option<WorldViewUnits<'a>>,
    /// Asset source for the road textures.
    pub asset_source: Option<&'a mut AssetSource<StdFileProvider>>,
    /// Width and height of the output in pixels.
    pub size: [u32; 2],
    /// Texture format of the output view.
    pub output_format: wgpu::TextureFormat,
}

/// The camera of one frame.
#[derive(Clone, Copy, Debug)]
pub struct WorldCamera {
    /// World-to-clip matrix.
    pub view_projection: Mat4,
    /// World-to-view matrix.
    pub world_to_view: Mat4,
    /// Camera position in world space.
    pub position: Vec3,
    /// Depth reconstruction coefficients for soft particles.
    pub depth_unproject: [f32; 2],
    /// Width and height of the viewport in pixels.
    pub viewport: [u32; 2],
}

impl WorldCamera {
    /// Returns the view that selects and fades the visible local lights.
    #[must_use]
    pub fn light_view(&self) -> LocalLightView {
        LocalLightView::new(
            self.view_projection,
            self.world_to_view,
            self.position,
            self.viewport,
        )
    }
}

/// Display settings of one frame.
#[derive(Clone, Copy, Debug)]
pub struct WorldViewSettings<'a> {
    /// Terrain display mode. Mode 0 is the retail lit view.
    pub debug_mode: u32,
    /// Terrain compositor display mode. Mode 0 is the normal composite.
    pub compositor_debug_mode: u32,
    /// Normal map strength (`gBumpPower` in the game).
    pub bump_power: f32,
    /// Distance bands of the compositor LOD. `None` keeps the current chunk LOD levels.
    pub lod: Option<&'a LodConfig>,
    /// Start and end distance of the foliage fade. `None` keeps the current distances.
    pub foliage_fade: Option<[f32; 2]>,
    /// Draw the sky, the units, the foliage, the roads and the particles.
    pub details: bool,
    /// Use the scenario tone-map settings. If false, use the default settings.
    pub scenario_tone_map: bool,
}

/// The inputs of one frame.
#[derive(Clone, Copy, Debug)]
pub struct WorldFrame<'a> {
    /// Camera of the frame.
    pub camera: WorldCamera,
    /// Authoritative simulation world, if the scenario has one.
    pub world: Option<&'a sim::World>,
    /// Monotonic render time for animations, in seconds.
    pub time_seconds: f32,
    /// Display settings of the frame.
    pub settings: WorldViewSettings<'a>,
}

/// GPU resources and frame orchestration that draw one loaded scenario.
pub struct WorldView {
    terrain: TerrainResources,
    shadow: ShadowResources,
    compositor: CompositorResources,
    compositor_bind_group: wgpu::BindGroup,
    compositor_debug_mode: u32,
    chunk_centers: Vec<[f32; 3]>,
    chunk_layer_counts: Vec<u32>,
    chunk_decal_layer_counts: Vec<u32>,
    foliage: Option<FoliageResources>,
    roads: Option<RoadResources>,
    sky: Option<UnitRenderer>,
    units: Option<UnitSceneRenderer>,
    tone_map: ToneMapResources,
    local_lights: LocalLightSet,
    lightset: Option<LightSetData>,
    clear_color: wgpu::Color,
    details: bool,
    size: [u32; 2],
    offscreen_depth: Option<(wgpu::Texture, wgpu::TextureView)>,
}

impl WorldView {
    /// Uploads the scenario data and makes all GPU resources.
    ///
    /// `settings` gives the initial terrain uniforms. Each `update` writes them again.
    ///
    /// # Errors
    ///
    /// Returns `WorldViewError::MissingPackedTerrain` if the terrain has no packed XTD data.
    ///
    /// # Panics
    ///
    /// Panics if the terrain build does not make the terrain, shadow or compositor resources.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        inputs: WorldViewInputs<'_>,
        settings: &WorldViewSettings<'_>,
    ) -> Result<Self, WorldViewError> {
        let tone_map = ToneMapResources::new(device, inputs.output_format, inputs.size);
        let raw_data = inputs
            .terrain
            .raw_xtd_data
            .as_ref()
            .ok_or(WorldViewError::MissingPackedTerrain)?;
        let mut build = TerrainBuild {
            scene: inputs.terrain,
            scene_format: HDR_COLOR_FORMAT,
            debug_mode: settings.debug_mode,
            bump_power: settings.bump_power,
            asset_source: inputs.asset_source,
            compositor: None,
            compositor_bind_group: None,
            chunk_centers: Vec::new(),
            gpu: None,
            shadow_resources: None,
            foliage_resources: None,
            road_resources: None,
        };
        build.create_gpu_tessellation_resources(
            device,
            queue,
            raw_data,
            inputs.terrain.albedo.as_ref(),
            inputs.size,
        );
        let TerrainBuild {
            compositor,
            compositor_bind_group,
            chunk_centers,
            gpu,
            shadow_resources,
            foliage_resources,
            road_resources,
            ..
        } = build;
        let terrain = gpu.expect("the terrain build makes the terrain resources");
        let shadow = shadow_resources.expect("the terrain build makes the shadow resources");
        let compositor = compositor.expect("the terrain build makes the compositor");
        let compositor_bind_group =
            compositor_bind_group.expect("the terrain build makes the compositor bind group");
        let (chunk_layer_counts, chunk_decal_layer_counts) =
            chunk_layer_counts(inputs.terrain, terrain.chunk_grid);
        let renderers = units::create_unit_renderers(
            device,
            queue,
            &units::UnitRendererInputs {
                terrain: &terrain,
                shadow: &shadow,
                raw_data,
                environment: inputs.environment,
                sky: inputs.sky,
                units: inputs.units,
                scene_format: HDR_COLOR_FORMAT,
            },
        );
        Ok(Self {
            terrain,
            shadow,
            compositor,
            compositor_bind_group,
            compositor_debug_mode: 0,
            chunk_centers,
            chunk_layer_counts,
            chunk_decal_layer_counts,
            foliage: foliage_resources,
            roads: road_resources,
            sky: renderers.sky,
            units: renderers.units,
            tone_map,
            local_lights: LocalLightSet::default(),
            lightset: inputs.lightset,
            clear_color: wgpu::Color::BLACK,
            details: true,
            size: inputs.size,
            offscreen_depth: None,
        })
    }

    /// Recreates the resources that depend on the output size.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.size = [width, height];
        let (depth_texture, depth_view) = create_depth_texture(device, width, height);
        self.terrain.depth_texture = depth_texture;
        self.terrain.depth_view = depth_view;
        self.bind_window_depth_to_particles();
        self.tone_map.resize(device, self.size);
    }

    /// Synchronizes the unit placements after the simulation entity set changes.
    pub fn sync_units(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &UnitScene,
        world: &sim::World,
    ) {
        if let Some(renderer) = &mut self.units {
            renderer.sync_scene(device, queue, scene, world);
            log::info!(
                "Synchronized {} simulation-backed UGX placements",
                renderer.placement_count()
            );
        }
    }

    /// Returns the number of terrain chunks that the compositor must still composite.
    #[must_use]
    pub fn dirty_terrain_chunk_count(&self) -> usize {
        self.compositor.dirty_chunk_count()
    }

    /// Records all passes of the frame and the tone mapping into `output`.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        self.encode_with_depth(encoder, output, &self.terrain.depth_view);
    }

    /// Updates and records one frame into an offscreen `output`.
    ///
    /// The frame uses `frame.camera.viewport` as its size and its own depth texture. The output
    /// must have the output format of the `WorldView`. Call `end_offscreen` after the last
    /// offscreen frame.
    pub fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &WorldFrame<'_>,
        output: &wgpu::TextureView,
    ) {
        let [width, height] = frame.camera.viewport;
        self.tone_map.resize(device, frame.camera.viewport);
        let (depth_texture, depth_view) = create_depth_texture(device, width, height);
        if let Some(renderer) = &mut self.units {
            renderer.set_particle_scene_textures(ParticleSceneTextures {
                depth: &depth_view,
                light_volume: Some(self.terrain.light_volume.color_view()),
            });
        }
        // An offscreen view has no interactive camera, so it ignores the camera shakes.
        let _camera_shakes = self.update(device, queue, frame);
        self.encode_with_depth(encoder, output, &depth_view);
        self.offscreen_depth = Some((depth_texture, depth_view));
    }

    /// Restores the output size and the depth texture after offscreen frames.
    pub fn end_offscreen(&mut self, device: &wgpu::Device) {
        self.offscreen_depth = None;
        self.bind_window_depth_to_particles();
        self.tone_map.resize(device, self.size);
    }

    fn bind_window_depth_to_particles(&mut self) {
        if let Some(renderer) = &mut self.units {
            renderer.set_particle_scene_textures(ParticleSceneTextures {
                depth: &self.terrain.depth_view,
                light_volume: Some(self.terrain.light_volume.color_view()),
            });
        }
    }
}

/// Counts the splat and decal layers of each chunk in the order of the unique atlas.
fn chunk_layer_counts(scene: &TerrainScene, chunk_grid: TerrainChunkGrid) -> (Vec<u32>, Vec<u32>) {
    let mut layer_counts = vec![1; chunk_grid.total_chunks()];
    for chunk in &scene.chunk_splat_data {
        if let Some(index) = chunk_grid.chunk_index(chunk.grid_x, chunk.grid_z) {
            layer_counts[index] = u32::try_from(chunk.layer_texture_ids.len())
                .expect("terrain layer count must fit u32");
        }
    }
    let mut decal_layer_counts = vec![0; chunk_grid.total_chunks()];
    for chunk in &scene.chunk_decal_data {
        if let Some(index) = chunk_grid.chunk_index(chunk.grid_x, chunk.grid_z) {
            decal_layer_counts[index] =
                u32::try_from(chunk.decal_layer_ids.len()).expect("decal layer count must fit u32");
        }
    }
    (layer_counts, decal_layer_counts)
}

/// Creates a depth texture and its view.
fn create_depth_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
