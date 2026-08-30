//! Renderer-local routing, metering, and item selection for sim impact events.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::source::{AssetSource, StdFileProvider};
use sim::{ImpactEffectRequest, ImpactEffectSize, ImpactSurface, PlayerId, World};

use super::{
    ImpactEffectCatalog, ImpactEffectDefinition, TerrainEffect, TerrainEffectAction,
    TerrainEffectSize, TerrainSurfaceCatalog, TerrainSurfaceEffect, canonical_terrain_effect_path,
};

const SURFACE_EFFECT_PATH: &str = "effects\\terraineffects\\terrainTile";
const VOXEL_EXTENT: Vec3 = Vec3::new(8.0, 16.0, 8.0);

/// Why a selected TFX item was instantiated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainEffectRouteKind {
    /// Tactic impact prototype's primary effect.
    Impact,
    /// Global terrain-tile effect added for non-flying targets.
    Surface,
    /// Renderer-owned animation tag evaluated against the contacted terrain.
    AnimationTag,
}

/// Fully resolved TFX item ready for particle/decal/light/visual/audio dispatch.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTerrainEffect {
    /// Sim request sequence that selected this item.
    pub sequence: u64,
    /// Impact, secondary surface, or presentation animation-tag route.
    pub kind: TerrainEffectRouteKind,
    /// Canonical TFX asset path.
    pub terrain_effect_path: String,
    /// Selected authored surface item and its actions.
    pub item: TerrainSurfaceEffect,
    /// World-space effect origin.
    pub position: Vec3,
    /// Retail forward axis supplied to action runtimes.
    pub forward: Vec3,
    /// Owner forwarded to temporary visuals.
    pub player_id: PlayerId,
    /// Temporary-visual lifetime from the impact prototype.
    pub lifespan_seconds: f32,
}

/// Immutable impact catalogs and decoded TFX graphs used by presentation.
///
/// Loading these ahead of time keeps the render loop independent from the
/// filesystem and also exposes every referenced PFX/LGT route to the scene's
/// existing GPU asset caches.
#[derive(Clone, Debug)]
pub struct TerrainImpactAssets {
    impact_effects: ImpactEffectCatalog,
    surfaces: TerrainSurfaceCatalog,
    effects: HashMap<String, Option<Arc<TerrainEffect>>>,
    meter_settings: HashMap<String, MeterSettings>,
    issues: Vec<String>,
}

impl TerrainImpactAssets {
    /// Decode the global impact tables and every TFX they can route to.
    ///
    /// # Errors
    ///
    /// Returns the underlying catalog error when either required global table
    /// is unavailable or malformed. Individual TFX failures are retained as
    /// diagnostics so one bad route cannot disable all other impact effects.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let impact_effects = ImpactEffectCatalog::load(source)?;
        let surfaces = TerrainSurfaceCatalog::load(source)?;
        let meter_settings = impact_effects
            .definitions()
            .iter()
            .map(|definition| {
                (
                    effect_key(&definition.terrain_effect_path),
                    MeterSettings::from_definition(definition),
                )
            })
            .collect();
        let mut referenced = BTreeMap::new();
        for definition in impact_effects.definitions() {
            referenced.insert(
                effect_key(&definition.terrain_effect_path),
                definition.terrain_effect_path.clone(),
            );
        }
        referenced.insert(
            effect_key(SURFACE_EFFECT_PATH),
            SURFACE_EFFECT_PATH.to_owned(),
        );
        let mut effects = HashMap::new();
        let mut issues = Vec::new();
        for (key, path) in referenced {
            match TerrainEffect::load(source, &path) {
                Ok(effect) => {
                    effects.insert(key, Some(Arc::new(effect)));
                }
                Err(error) => {
                    push_unique(&mut issues, error.to_string());
                    effects.insert(key, None);
                }
            }
        }
        Ok(Self {
            impact_effects,
            surfaces,
            effects,
            meter_settings,
            issues,
        })
    }

    /// Return all PFX paths reachable from the preloaded impact routes.
    #[must_use]
    pub fn particle_paths(&self) -> Vec<String> {
        self.action_paths(|action| {
            if let TerrainEffectAction::Particle(path) = action {
                Some(path.as_str())
            } else {
                None
            }
        })
    }

    /// Return all LGT paths reachable from the preloaded impact routes.
    #[must_use]
    pub fn light_paths(&self) -> Vec<String> {
        self.action_paths(|action| {
            if let TerrainEffectAction::Light(light) = action {
                Some(light.path.as_str())
            } else {
                None
            }
        })
    }

    /// Return all terrain-patch material paths reachable from impact decals.
    #[must_use]
    pub fn decal_paths(&self) -> Vec<String> {
        self.action_paths(|action| {
            if let TerrainEffectAction::ImpactDecal(decal) = action {
                Some(decal.path.as_str())
            } else {
                None
            }
        })
    }

    /// Return all proto-visual names reachable from temporary impact visuals.
    #[must_use]
    pub fn visual_names(&self) -> Vec<String> {
        self.action_paths(|action| {
            if let TerrainEffectAction::Visual(name) = action {
                Some(name.as_str())
            } else {
                None
            }
        })
    }

    /// Return how many unique TFX graphs decoded successfully.
    #[must_use]
    pub fn loaded_effect_count(&self) -> usize {
        self.effects
            .values()
            .filter(|effect| effect.is_some())
            .count()
    }

    /// Return missing or malformed TFX diagnostics collected during preload.
    #[must_use]
    pub fn issues(&self) -> &[String] {
        &self.issues
    }

    fn action_paths(
        &self,
        mut select: impl for<'action> FnMut(&'action TerrainEffectAction) -> Option<&'action str>,
    ) -> Vec<String> {
        let mut paths = BTreeMap::new();
        for action in self
            .effects
            .values()
            .filter_map(Option::as_deref)
            .flat_map(|effect| &effect.surfaces)
            .flat_map(|surface| &surface.actions)
        {
            if let Some(path) = select(action) {
                paths.insert(action_path_key(path), path.to_owned());
            }
        }
        paths.into_values().collect()
    }
}

/// Renderer-owned consumer for authoritative projectile-impact requests.
#[derive(Debug)]
pub struct TerrainImpactRouter {
    assets: TerrainImpactAssets,
    meter_log: HashMap<MeterKey, VecDeque<u32>>,
    cursor: u64,
    issues: Vec<String>,
}

impl TerrainImpactRouter {
    /// Load global impact and surface prototype tables.
    ///
    /// # Errors
    ///
    /// Returns the underlying catalog error when either required table is not
    /// available in the active asset stack.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self::new(TerrainImpactAssets::load(source)?))
    }

    /// Create mutable routing state over already-decoded immutable assets.
    #[must_use]
    pub fn new(assets: TerrainImpactAssets) -> Self {
        Self {
            issues: assets.issues.clone(),
            assets,
            meter_log: HashMap::new(),
            cursor: 0,
        }
    }

    /// Consume retained sim requests newer than this router's local cursor.
    pub fn route_new(&mut self, world: &World) -> Vec<ResolvedTerrainEffect> {
        let requests = world
            .impact_effect_requests_after(self.cursor)
            .cloned()
            .collect::<Vec<_>>();
        let mut resolved = Vec::new();
        for request in requests {
            self.cursor = request.sequence();
            self.route_request(&request, &mut resolved);
        }
        resolved
    }

    /// Last sim request sequence examined, including missing/metered effects.
    #[must_use]
    pub const fn cursor(&self) -> u64 {
        self.cursor
    }

    /// Missing or malformed assets encountered while routing requests.
    #[must_use]
    pub fn issues(&self) -> &[String] {
        &self.issues
    }

    fn route_request(
        &mut self,
        request: &ImpactEffectRequest,
        output: &mut Vec<ResolvedTerrainEffect>,
    ) {
        let Some(definition) = self
            .assets
            .impact_effects
            .get(&request.effect().name)
            .cloned()
        else {
            self.record_issue(format!(
                "impact-effect prototype not found: {}",
                request.effect().name
            ));
            return;
        };
        let surface_name = self.surface_name(request.surface()).to_owned();
        let size = effect_size(request.effect().size);
        let main_path = definition.terrain_effect_path.clone();
        if self.check_meter(&main_path, request.position(), request.occurred_at_ms()) {
            output.extend(self.select_route(
                request,
                RouteSelection {
                    definition: &definition,
                    path: &main_path,
                    surface_name: &surface_name,
                    size,
                    kind: TerrainEffectRouteKind::Impact,
                },
            ));
        }
        if request.emits_surface_effect() {
            output.extend(self.select_route(
                request,
                RouteSelection {
                    definition: &definition,
                    path: SURFACE_EFFECT_PATH,
                    surface_name: &surface_name,
                    size,
                    kind: TerrainEffectRouteKind::Surface,
                },
            ));
        }
    }

    fn select_route(
        &self,
        request: &ImpactEffectRequest,
        selection: RouteSelection<'_>,
    ) -> Option<ResolvedTerrainEffect> {
        let effect = self.effect(selection.path)?;
        let roll = presentation_roll(request.sequence(), selection.path, selection.kind);
        let item = effect
            .select(selection.surface_name, selection.size, roll)
            .cloned()?;
        Some(ResolvedTerrainEffect {
            sequence: request.sequence(),
            kind: selection.kind,
            terrain_effect_path: canonical_terrain_effect_path(selection.path),
            item,
            position: request.position(),
            forward: request.forward(),
            player_id: request.player_id(),
            lifespan_seconds: selection.definition.lifespan_seconds,
        })
    }

    fn effect(&self, path: &str) -> Option<&Arc<TerrainEffect>> {
        self.assets.effects.get(&effect_key(path))?.as_ref()
    }

    fn surface_name<'request>(
        &'request self,
        surface: Option<&'request ImpactSurface>,
    ) -> &'request str {
        match surface {
            Some(ImpactSurface::Terrain(surface_type)) => self
                .assets
                .surfaces
                .name(*surface_type)
                .unwrap_or("UNDEFINED"),
            Some(ImpactSurface::Object(name)) => name,
            None => "default",
        }
    }

    fn check_meter(&mut self, path: &str, position: Vec3, now_ms: u32) -> bool {
        let Some(settings) = self.assets.meter_settings.get(&effect_key(path)).copied() else {
            return true;
        };
        if settings.length_ms == 0 || settings.count == 0 {
            return true;
        }
        let key = MeterKey::new(path, position);
        let log = self.meter_log.entry(key).or_default();
        let earliest = now_ms.saturating_sub(settings.length_ms);
        while log.front().is_some_and(|time| *time <= earliest) {
            log.pop_front();
        }
        if log.len() >= settings.count as usize {
            return false;
        }
        log.push_back(now_ms);
        true
    }

    fn record_issue(&mut self, issue: String) {
        push_unique(&mut self.issues, issue);
    }
}

#[derive(Clone, Copy)]
struct RouteSelection<'route> {
    definition: &'route ImpactEffectDefinition,
    path: &'route str,
    surface_name: &'route str,
    size: TerrainEffectSize,
    kind: TerrainEffectRouteKind,
}

#[derive(Clone, Copy, Debug)]
struct MeterSettings {
    length_ms: u32,
    count: u32,
}

impl MeterSettings {
    fn from_definition(definition: &ImpactEffectDefinition) -> Self {
        let milliseconds = f64::from(definition.lifespan_seconds) * 1_000.0;
        Self {
            length_ms: if milliseconds.is_finite() && milliseconds > 0.0 {
                milliseconds
                    .min(f64::from(u32::MAX))
                    .to_u32()
                    .unwrap_or(u32::MAX)
            } else {
                0
            },
            count: definition.meter_limit,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct MeterKey {
    effect: String,
    voxel: [i32; 3],
}

impl MeterKey {
    fn new(path: &str, position: Vec3) -> Self {
        Self {
            effect: effect_key(path),
            voxel: [
                voxel_coordinate(position.x, VOXEL_EXTENT.x),
                voxel_coordinate(position.y, VOXEL_EXTENT.y),
                voxel_coordinate(position.z, VOXEL_EXTENT.z),
            ],
        }
    }
}

fn effect_key(path: &str) -> String {
    canonical_terrain_effect_path(path).to_ascii_lowercase()
}

fn action_path_key(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

fn push_unique(issues: &mut Vec<String>, issue: String) {
    if !issues.contains(&issue) {
        issues.push(issue);
    }
}

fn voxel_coordinate(value: f32, extent: f32) -> i32 {
    let coordinate = (value / extent).trunc();
    coordinate.to_i32().unwrap_or_else(|| {
        if coordinate.is_sign_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}

const fn effect_size(size: ImpactEffectSize) -> TerrainEffectSize {
    match size {
        ImpactEffectSize::Small => TerrainEffectSize::Small,
        ImpactEffectSize::Medium => TerrainEffectSize::Medium,
        ImpactEffectSize::Large => TerrainEffectSize::Large,
        ImpactEffectSize::Generic => TerrainEffectSize::Generic,
    }
}

fn presentation_roll(sequence: u64, path: &str, kind: TerrainEffectRouteKind) -> u32 {
    let mut value = sequence
        ^ match kind {
            TerrainEffectRouteKind::Impact => 0x243f_6a88_85a3_08d3,
            TerrainEffectRouteKind::Surface => 0x1319_8a2e_0370_7344,
            TerrainEffectRouteKind::AnimationTag => 0xa409_3822_299f_31d0,
        };
    for byte in path.bytes() {
        value ^= u64::from(byte.to_ascii_lowercase());
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    let [byte_0, byte_1, byte_2, byte_3, _, _, _, _] = (value ^ (value >> 31)).to_le_bytes();
    u32::from_le_bytes([byte_0, byte_1, byte_2, byte_3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_works_from_game_time_zero_instead_of_retail_underflowing() {
        let path = "effects\\impact";
        let assets = TerrainImpactAssets {
            impact_effects: ImpactEffectCatalog::default(),
            surfaces: TerrainSurfaceCatalog::default(),
            effects: HashMap::new(),
            meter_settings: HashMap::from([(
                effect_key(path),
                MeterSettings {
                    length_ms: 3_000,
                    count: 2,
                },
            )]),
            issues: Vec::new(),
        };
        let mut router = TerrainImpactRouter::new(assets);
        let position = Vec3::new(1.0, 2.0, 3.0);

        assert!(router.check_meter(path, position, 10));
        assert!(router.check_meter(path, position, 20));
        assert!(!router.check_meter(path, position, 30));
        assert!(router.check_meter(path, position, 3_021));
    }

    #[test]
    fn meter_voxels_use_retail_truncation_toward_zero() {
        let positive = MeterKey::new("impact", Vec3::new(7.9, 15.9, 7.9));
        let negative = MeterKey::new("impact", Vec3::new(-7.9, -15.9, -7.9));
        assert_eq!(positive.voxel, [0, 0, 0]);
        assert_eq!(negative.voxel, [0, 0, 0]);
    }
}
