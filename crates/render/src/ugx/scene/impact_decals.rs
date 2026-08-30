//! Preloaded materials and timed terrain patches for TFX impact decals.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::source::{AssetSource, StdFileProvider};

use crate::terrain::{
    LightingParams, TerrainPatchInstance, TerrainPatchMaterial, TerrainPatchRenderer,
    TerrainPatchRendererDescriptor, TerrainPatchWorldBindings, canonical_patch_path,
};
use crate::terrain_effect::{TerrainDecalOrientation, TerrainImpactDecal};
use crate::{RenderPhase, wgpu};

#[derive(Clone, Debug, Default)]
pub(super) struct ImpactDecalAssets {
    materials: HashMap<String, Option<Arc<TerrainPatchMaterial>>>,
    issues: Vec<String>,
}

impl ImpactDecalAssets {
    pub(super) fn load_referenced(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        paths: impl IntoIterator<Item = String>,
    ) {
        for path in paths {
            let key = decal_key(&path);
            if self.materials.contains_key(&key) {
                continue;
            }
            match TerrainPatchMaterial::load(source, &path) {
                Ok(material) => {
                    self.materials.insert(key, Some(Arc::new(material)));
                }
                Err(error) => {
                    self.issues.push(error.to_string());
                    self.materials.insert(key, None);
                }
            }
        }
    }

    fn loaded_count(&self) -> usize {
        self.materials
            .values()
            .filter(|material| material.is_some())
            .count()
    }

    fn issues(&self) -> &[String] {
        &self.issues
    }
}

impl super::UnitScene {
    /// Return the number of unique TFX impact-decal materials decoded.
    #[must_use]
    pub fn impact_decal_material_count(&self) -> usize {
        self.impact_decal_assets.loaded_count()
    }

    /// Return missing or malformed impact-decal material diagnostics.
    #[must_use]
    pub fn impact_decal_material_issues(&self) -> &[String] {
        self.impact_decal_assets.issues()
    }

    /// Return the number of impact-decal material diagnostics.
    #[must_use]
    pub fn impact_decal_material_issue_count(&self) -> usize {
        self.impact_decal_assets.issues().len()
    }
}

struct LiveDecal {
    material_key: String,
    instance: TerrainPatchInstance,
    elapsed_seconds: f32,
    opaque_seconds: f32,
    fade_seconds: f32,
}

struct DecalBatch {
    instances: Vec<TerrainPatchInstance>,
    renderer: TerrainPatchRenderer,
}

pub(super) struct ImpactDecalRenderer {
    batches: BTreeMap<String, DecalBatch>,
    live: BTreeMap<u64, LiveDecal>,
    last_time_seconds: Option<f32>,
}

impl ImpactDecalRenderer {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        world: TerrainPatchWorldBindings<'_>,
        assets: &ImpactDecalAssets,
    ) -> Self {
        let materials = assets
            .materials
            .iter()
            .filter_map(|(key, material)| {
                material
                    .as_ref()
                    .map(|material| (key.clone(), Arc::clone(material)))
            })
            .collect::<BTreeMap<_, _>>();
        let batches = materials
            .into_iter()
            .map(|(key, material)| {
                let renderer = TerrainPatchRenderer::new(
                    device,
                    queue,
                    TerrainPatchRendererDescriptor {
                        color_format,
                        depth_format,
                        material: &material,
                        world,
                    },
                );
                (
                    key,
                    DecalBatch {
                        instances: Vec::new(),
                        renderer,
                    },
                )
            })
            .collect();
        Self {
            batches,
            live: BTreeMap::new(),
            last_time_seconds: None,
        }
    }

    pub(super) fn spawn(
        &mut self,
        presentation_id: u64,
        decal: &TerrainImpactDecal,
        position: Vec3,
        authored_forward: Vec3,
    ) {
        if !position.is_finite() {
            return;
        }
        let material_key = decal_key(&decal.path);
        if !self.batches.contains_key(&material_key) {
            return;
        }
        let Some(forward) = decal_forward(decal, authored_forward, presentation_id) else {
            return;
        };
        let right = Vec3::Y.cross(forward).normalize_or_zero();
        if right == Vec3::ZERO {
            return;
        }
        let opaque_seconds = finite_nonnegative(decal.fully_opaque_seconds);
        let fade_seconds = finite_nonnegative(decal.fade_out_seconds);
        self.live.insert(
            presentation_id,
            LiveDecal {
                material_key,
                instance: TerrainPatchInstance {
                    center: position.to_array(),
                    axis_u: (right * (finite_nonnegative(decal.size_x) * 0.5)).to_array(),
                    axis_v: (forward * (finite_nonnegative(decal.size_z) * 0.5)).to_array(),
                    y_offset: 0.25,
                    intensity: 3.0,
                    color: [1.0; 4],
                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                },
                elapsed_seconds: 0.0,
                opaque_seconds,
                fade_seconds,
            },
        );
    }

    pub(super) fn update_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        time_seconds: f32,
        view_projection: glam::Mat4,
        lighting: &LightingParams,
    ) {
        let delta_seconds = self.last_time_seconds.map_or(0.0, |last| {
            if time_seconds.is_finite() {
                (time_seconds - last).max(0.0)
            } else {
                0.0
            }
        });
        self.last_time_seconds = time_seconds.is_finite().then_some(time_seconds);
        for batch in self.batches.values_mut() {
            batch.instances.clear();
        }
        for decal in self.live.values_mut() {
            decal.elapsed_seconds += delta_seconds;
            let Some(alpha) = decal_alpha(decal) else {
                continue;
            };
            let mut instance = decal.instance;
            instance.color[3] = alpha;
            if let Some(batch) = self.batches.get_mut(&decal.material_key) {
                batch.instances.push(instance);
            }
        }
        self.live.retain(|_, decal| decal_alpha(decal).is_some());
        for batch in self.batches.values_mut() {
            batch
                .renderer
                .update_frame(queue, view_projection, lighting);
            if let Err(error) = batch
                .renderer
                .update_instances(device, queue, &batch.instances)
            {
                log::warn!("could not upload impact decals: {error}");
            }
        }
    }

    pub(super) fn render_phase<'pass>(
        &'pass self,
        phase: RenderPhase,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        if phase != RenderPhase::World {
            return;
        }
        for batch in self.batches.values() {
            batch.renderer.render(pass);
        }
    }

    pub(super) fn live_count(&self) -> usize {
        self.live.len()
    }
}

fn decal_key(path: &str) -> String {
    canonical_patch_path(path).to_ascii_lowercase()
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn decal_forward(
    decal: &TerrainImpactDecal,
    authored_forward: Vec3,
    presentation_id: u64,
) -> Option<Vec3> {
    match decal.orientation {
        TerrainDecalOrientation::Aligned => {
            Vec3::new(authored_forward.x, 0.0, authored_forward.z).try_normalize()
        }
        TerrainDecalOrientation::Random => {
            let angle = deterministic_unit(presentation_id, &decal.path) * std::f32::consts::TAU;
            Some(Vec3::new(angle.sin(), 0.0, angle.cos()))
        }
    }
}

fn deterministic_unit(presentation_id: u64, path: &str) -> f32 {
    let mut hash = presentation_id ^ 0x082e_fa98_ec4e_6c89;
    for byte in path.bytes() {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let fraction = u32::try_from((hash ^ (hash >> 32)) >> 40).unwrap_or(0);
    fraction.to_f32().unwrap_or(0.0) / 16_777_216.0
}

fn decal_alpha(decal: &LiveDecal) -> Option<f32> {
    if decal.elapsed_seconds < decal.opaque_seconds {
        return Some(1.0);
    }
    if decal.fade_seconds <= 0.0 {
        return None;
    }
    let fade_position = (decal.elapsed_seconds - decal.opaque_seconds) / decal.fade_seconds;
    (fade_position < 1.0).then_some((1.0 - fade_position).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrected_random_orientation_is_deterministic_and_unit_length() {
        let decal = TerrainImpactDecal {
            path: "decals\\impact".to_owned(),
            size_x: 2.0,
            size_z: 3.0,
            fully_opaque_seconds: 1.0,
            fade_out_seconds: 2.0,
            orientation: TerrainDecalOrientation::Random,
        };
        let first = decal_forward(&decal, Vec3::Z, 17).unwrap();
        let second = decal_forward(&decal, Vec3::X, 17).unwrap();
        assert!(first.abs_diff_eq(second, 0.0));
        assert!((first.length() - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn hold_then_fade_matches_recovered_lit_decal_timing() {
        let mut decal = LiveDecal {
            material_key: String::new(),
            instance: TerrainPatchInstance::default(),
            elapsed_seconds: 1.0,
            opaque_seconds: 1.0,
            fade_seconds: 2.0,
        };
        assert_eq!(decal_alpha(&decal), Some(1.0));
        decal.elapsed_seconds = 2.0;
        assert_eq!(decal_alpha(&decal), Some(0.5));
        decal.elapsed_seconds = 3.0;
        assert_eq!(decal_alpha(&decal), None);
    }
}
