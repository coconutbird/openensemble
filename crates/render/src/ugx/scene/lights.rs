use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use glam::{Mat4, Vec3};
use pipeline::source::{AssetSource, StdFileProvider};

use super::super::{UnitAttachment, UnitAttachmentKind, UnitAttachmentTrigger, UnitRenderer};
use crate::light_effect::{LightEffect, LightEffectRuntime, LightEffectSample};
use crate::light_volume::MAX_BUFFERED_LIGHTS;
use crate::lighting::{LocalLight, LocalLightSet, LocalLightView, MAX_LOCAL_LIGHTS};
use crate::local_shadow::LocalShadowRequest;

const LIGHT_FADE_START_PIXELS: f32 = 15.0;
const LIGHT_FADE_END_PIXELS: f32 = 30.0;

#[derive(Clone, Debug, Default)]
pub(super) struct LightAssets {
    assets: HashMap<String, Option<Arc<LightEffect>>>,
    issues: Vec<String>,
}

impl LightAssets {
    pub(super) fn load_referenced(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        paths: impl IntoIterator<Item = String>,
    ) {
        for path in paths {
            let canonical = canonical_light_path(&path);
            let key = canonical.to_ascii_lowercase();
            if self.assets.contains_key(&key) {
                continue;
            }
            match LightEffect::load(source, &canonical) {
                Ok(effect) => {
                    self.assets.insert(key, Some(Arc::new(effect)));
                }
                Err(error) => {
                    self.issues.push(error.to_string());
                    self.assets.insert(key, None);
                }
            }
        }
    }

    fn get(&self, path: &str) -> Option<&Arc<LightEffect>> {
        self.assets
            .get(&canonical_light_path(path).to_ascii_lowercase())?
            .as_ref()
    }

    fn loaded_count(&self) -> usize {
        self.assets.values().filter(|asset| asset.is_some()).count()
    }

    fn issue_count(&self) -> usize {
        self.issues.len()
    }

    fn issues(&self) -> &[String] {
        &self.issues
    }
}

impl super::UnitScene {
    /// Return the number of unique `.lgt` scenes decoded for the current roster.
    #[must_use]
    pub fn light_effect_count(&self) -> usize {
        self.light_assets.loaded_count()
    }

    /// Return the number of missing or invalid `.lgt` assets encountered.
    #[must_use]
    pub fn light_effect_issue_count(&self) -> usize {
        self.light_assets.issue_count()
    }

    /// Return diagnostics for missing or invalid `.lgt` assets.
    #[must_use]
    pub fn light_effect_issues(&self) -> &[String] {
        self.light_assets.issues()
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EffectKey {
    owner_id: u64,
    attachment_index: usize,
    animation_revision: u32,
    asset_key: String,
}

struct LiveEffect {
    runtime: LightEffectRuntime,
    transform: Mat4,
    attached: bool,
    visible: bool,
}

struct TimedEffect {
    runtime: LightEffectRuntime,
    transform: Mat4,
    remaining_seconds: f32,
}

#[derive(Clone, Copy)]
pub(super) struct LightPlacementState<'renderer> {
    pub(super) owner_id: u64,
    pub(super) animation_type: Option<&'renderer str>,
    pub(super) movement_animation_type: Option<&'renderer str>,
    pub(super) animation_revision: u32,
    pub(super) transform: Mat4,
    pub(super) visible: bool,
    pub(super) renderer: &'renderer UnitRenderer,
}

pub(super) struct AttachedLightRenderer {
    assets: LightAssets,
    effects: BTreeMap<EffectKey, LiveEffect>,
    timed_effects: BTreeMap<u64, TimedEffect>,
    last_time_seconds: Option<f32>,
    buffered_lights: Vec<LocalLight>,
    shadow_requests: Vec<LocalShadowRequest>,
    omitted_light_count: usize,
    omitted_buffered_light_count: usize,
}

impl AttachedLightRenderer {
    pub(super) fn new(assets: &LightAssets) -> Self {
        Self {
            assets: assets.clone(),
            effects: BTreeMap::new(),
            timed_effects: BTreeMap::new(),
            last_time_seconds: None,
            buffered_lights: Vec::new(),
            shadow_requests: Vec::new(),
            omitted_light_count: 0,
            omitted_buffered_light_count: 0,
        }
    }

    pub(super) fn begin_sync(&mut self, assets: &LightAssets) {
        self.assets = assets.clone();
        for effect in self.effects.values_mut() {
            effect.attached = false;
        }
    }

    pub(super) fn spawn_timed(
        &mut self,
        presentation_id: u64,
        path: &str,
        transform: Mat4,
        lifespan_seconds: f32,
    ) {
        if !lifespan_seconds.is_finite() || lifespan_seconds <= 0.0 {
            return;
        }
        let Some(asset) = self.assets.get(path).cloned() else {
            return;
        };
        self.timed_effects.insert(
            presentation_id,
            TimedEffect {
                runtime: LightEffectRuntime::new(asset),
                transform,
                remaining_seconds: lifespan_seconds,
            },
        );
    }

    pub(super) fn update_timed_transform(&mut self, presentation_id: u64, transform: Mat4) {
        if let Some(effect) = self.timed_effects.get_mut(&presentation_id) {
            effect.transform = transform;
        }
    }

    pub(super) fn sync_placement(&mut self, placement: LightPlacementState<'_>) {
        for (attachment_index, attachment) in active_lights(placement) {
            let Some(path) = attachment.asset_path.as_deref() else {
                continue;
            };
            let Some(asset) = self.assets.get(path).cloned() else {
                continue;
            };
            let asset_key = canonical_light_path(path).to_ascii_lowercase();
            let key = effect_key(
                placement.owner_id,
                attachment_index,
                placement.animation_revision,
                &attachment.trigger,
                &asset_key,
            );
            let transform = attachment.world_transform(placement.transform);
            self.effects
                .entry(key)
                .and_modify(|effect| {
                    effect.transform = transform;
                    effect.attached = true;
                    effect.visible = placement.visible;
                })
                .or_insert_with(|| LiveEffect {
                    runtime: LightEffectRuntime::new(asset),
                    transform,
                    attached: true,
                    visible: placement.visible,
                });
        }
    }

    pub(super) fn end_sync(&mut self) {
        self.effects.retain(|_, effect| effect.attached);
    }

    pub(super) fn begin_frame(&mut self) {
        for effect in self.effects.values_mut() {
            effect.visible = false;
        }
    }

    pub(super) fn update_placement(&mut self, placement: LightPlacementState<'_>) {
        for (attachment_index, attachment) in active_lights(placement) {
            let Some(path) = attachment.asset_path.as_deref() else {
                continue;
            };
            let asset_key = canonical_light_path(path).to_ascii_lowercase();
            let key = effect_key(
                placement.owner_id,
                attachment_index,
                placement.animation_revision,
                &attachment.trigger,
                &asset_key,
            );
            if let Some(effect) = self.effects.get_mut(&key) {
                effect.transform = attachment.world_transform(placement.transform);
                effect.visible = placement.visible;
            }
        }
    }

    pub(super) fn remove_owner(&mut self, owner_id: u64) {
        self.effects.retain(|key, _| key.owner_id != owner_id);
    }

    pub(super) fn finish_frame(
        &mut self,
        time_seconds: f32,
        view: LocalLightView,
        intensity_scale: f32,
        output: &mut LocalLightSet,
    ) {
        let delta_seconds = self.last_time_seconds.map_or(0.0, |last| {
            if time_seconds.is_finite() && last.is_finite() {
                (time_seconds - last).max(0.0)
            } else {
                0.0
            }
        });
        self.last_time_seconds = time_seconds.is_finite().then_some(time_seconds);

        let mut direct_candidates = Vec::new();
        let mut buffered_candidates = Vec::new();
        self.collect_attachment_candidates(
            delta_seconds,
            view,
            intensity_scale,
            &mut direct_candidates,
            &mut buffered_candidates,
        );
        self.collect_timed_candidates(
            delta_seconds,
            view,
            intensity_scale,
            &mut direct_candidates,
            &mut buffered_candidates,
        );
        direct_candidates.sort_by(|a, b| {
            influence_score(a.light, view.camera_position)
                .total_cmp(&influence_score(b.light, view.camera_position))
        });
        self.omitted_light_count = direct_candidates.len().saturating_sub(MAX_LOCAL_LIGHTS);
        direct_candidates.truncate(MAX_LOCAL_LIGHTS);
        self.omitted_buffered_light_count = buffered_candidates
            .len()
            .saturating_sub(MAX_BUFFERED_LIGHTS);
        buffered_candidates.truncate(MAX_BUFFERED_LIGHTS);
        self.buffered_lights = buffered_candidates;
        self.shadow_requests = direct_candidates
            .iter()
            .enumerate()
            .filter_map(|(light_index, candidate)| {
                candidate.shadow_id.map(|stable_id| LocalShadowRequest {
                    light_index,
                    stable_id,
                    screen_radius: candidate.screen_radius,
                    spot_right: candidate.spot_right,
                })
            })
            .collect();
        output
            .replace(
                direct_candidates
                    .into_iter()
                    .map(|candidate| candidate.light)
                    .collect(),
            )
            .expect("attached light selection is bounded to the shader limit");
    }

    fn collect_attachment_candidates(
        &mut self,
        delta_seconds: f32,
        view: LocalLightView,
        intensity_scale: f32,
        direct: &mut Vec<DirectCandidate>,
        buffered: &mut Vec<LocalLight>,
    ) {
        for (key, effect) in self.effects.iter_mut().filter(|(_, effect)| effect.visible) {
            let samples = effect.runtime.advance_and_sample_with_flags(
                delta_seconds,
                effect.transform,
                intensity_scale,
            );
            for (sample_index, sample) in samples.into_iter().enumerate() {
                append_light_candidate(
                    sample,
                    view,
                    shadow_stable_id(key, sample_index),
                    direct,
                    buffered,
                );
            }
        }
    }

    fn collect_timed_candidates(
        &mut self,
        delta_seconds: f32,
        view: LocalLightView,
        intensity_scale: f32,
        direct: &mut Vec<DirectCandidate>,
        buffered: &mut Vec<LocalLight>,
    ) {
        for (&presentation_id, effect) in &mut self.timed_effects {
            let active_seconds = effect.remaining_seconds.min(delta_seconds);
            let samples = effect.runtime.advance_and_sample_with_flags(
                active_seconds,
                effect.transform,
                intensity_scale,
            );
            effect.remaining_seconds = (effect.remaining_seconds - delta_seconds).max(0.0);
            for (sample_index, sample) in samples.into_iter().enumerate() {
                append_light_candidate(
                    sample,
                    view,
                    timed_shadow_stable_id(presentation_id, sample_index),
                    direct,
                    buffered,
                );
            }
        }
        self.timed_effects
            .retain(|_, effect| effect.remaining_seconds > 0.0);
    }

    pub(super) fn live_effect_count(&self) -> usize {
        self.effects.len() + self.timed_effects.len()
    }

    pub(super) const fn omitted_light_count(&self) -> usize {
        self.omitted_light_count
    }

    pub(super) fn buffered_lights(&self) -> &[LocalLight] {
        &self.buffered_lights
    }

    pub(super) fn shadow_requests(&self) -> &[LocalShadowRequest] {
        &self.shadow_requests
    }

    pub(super) const fn omitted_buffered_light_count(&self) -> usize {
        self.omitted_buffered_light_count
    }
}

struct DirectCandidate {
    light: LocalLight,
    screen_radius: f32,
    shadow_id: Option<u64>,
    spot_right: [f32; 3],
}

struct FadedLight {
    light: LocalLight,
    screen_radius: f32,
}

fn active_lights(
    placement: LightPlacementState<'_>,
) -> impl Iterator<Item = (usize, &UnitAttachment)> {
    placement
        .renderer
        .attachments()
        .iter()
        .enumerate()
        .filter(move |(_, attachment)| {
            attachment.kind == UnitAttachmentKind::Light
                && attachment
                    .trigger
                    .matches_animations(placement.animation_type, placement.movement_animation_type)
        })
}

fn effect_key(
    owner_id: u64,
    attachment_index: usize,
    animation_revision: u32,
    trigger: &UnitAttachmentTrigger,
    asset_key: &str,
) -> EffectKey {
    EffectKey {
        owner_id,
        attachment_index,
        animation_revision: match trigger {
            UnitAttachmentTrigger::Persistent => 0,
            UnitAttachmentTrigger::Animation(_) => animation_revision,
        },
        asset_key: asset_key.to_owned(),
    }
}

fn canonical_light_path(path: &str) -> String {
    let mut normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    if !normalized
        .get(normalized.len().saturating_sub(4)..)
        .is_some_and(|extension| extension.eq_ignore_ascii_case(".lgt"))
    {
        normalized.push_str(".lgt");
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

fn influence_score(light: LocalLight, camera_position: Vec3) -> f32 {
    let distance_squared = Vec3::from_array(light.position).distance_squared(camera_position);
    distance_squared / light.radius.max(0.001).powi(2)
}

fn screen_faded_light(mut light: LocalLight, view: LocalLightView) -> Option<FadedLight> {
    let mut screen_radius = view.project_sphere(light.position, light.radius)?[2];
    if let crate::lighting::LocalLightShape::Spot {
        direction,
        outer_cos,
        ..
    } = light.shape
    {
        if !view.capped_cone_visible(light.position, direction, outer_cos, light.radius) {
            return None;
        }
        if let Some(cone_radius) =
            view.project_capped_cone_radius(light.position, direction, outer_cos, light.radius)
        {
            screen_radius = screen_radius.min(cone_radius);
        }
    }
    let fade_byte = (255.0 * (screen_radius - LIGHT_FADE_START_PIXELS)
        / (LIGHT_FADE_END_PIXELS - LIGHT_FADE_START_PIXELS))
        .trunc()
        .clamp(0.0, 255.0);
    if fade_byte < 1.0 {
        return None;
    }
    let fade = fade_byte / 255.0;
    light.color = light.color.map(|channel| channel * fade);
    Some(FadedLight {
        light,
        screen_radius,
    })
}

fn append_light_candidate(
    sample: LightEffectSample,
    view: LocalLightView,
    stable_id: u64,
    direct: &mut Vec<DirectCandidate>,
    buffered: &mut Vec<LocalLight>,
) {
    let Some(faded) = screen_faded_light(sample.light, view) else {
        return;
    };
    if sample.flags.light_buffered() && !sample.flags.shadows() {
        buffered.push(faded.light);
    } else {
        direct.push(DirectCandidate {
            light: faded.light,
            screen_radius: faded.screen_radius,
            shadow_id: sample.flags.shadows().then_some(stable_id),
            spot_right: sample.spot_right,
        });
    }
}

fn shadow_stable_id(key: &EffectKey, sample_index: usize) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    [
        key.owner_id,
        key.attachment_index as u64,
        u64::from(key.animation_revision),
        sample_index as u64,
    ]
    .into_iter()
    .fold(OFFSET, |hash, value| (hash ^ value).wrapping_mul(PRIME))
}

fn timed_shadow_stable_id(presentation_id: u64, sample_index: usize) -> u64 {
    const OFFSET: u64 = 0x517c_c1b7_2722_0a95;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    [presentation_id, sample_index as u64]
        .into_iter()
        .fold(OFFSET, |hash, value| (hash ^ value).wrapping_mul(PRIME))
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::{canonical_light_path, screen_faded_light};
    use crate::lighting::{LocalLight, LocalLightView};

    #[test]
    fn canonical_path_adds_only_a_missing_extension() {
        assert_eq!(canonical_light_path("art/foo/bar"), "art\\foo\\bar.lgt");
        assert_eq!(canonical_light_path("art/foo/bar.lgt"), "art\\foo\\bar.lgt");
        assert_eq!(canonical_light_path("art/foo/bar.LGT"), "art\\foo\\bar.LGT");
        assert_eq!(canonical_light_path("effects/foo"), "art\\effects\\foo.lgt");
    }

    #[test]
    fn projected_light_fade_keeps_the_retail_byte_quantization() {
        let projection = Mat4::orthographic_rh(-10.0, 10.0, -10.0, 10.0, 0.1, 100.0);
        let view = LocalLightView::new(projection, Mat4::IDENTITY, Vec3::ZERO, [200, 100]);
        let light = LocalLight::omni([0.0, 0.0, -10.0], [1.0; 3], 2.0);
        let faded = screen_faded_light(light, view).expect("twenty-pixel light remains visible");
        let expected = 85.0 / 255.0;
        assert!((faded.light.color[0] - expected).abs() < 1.0e-6);
        assert!((faded.screen_radius - 20.0).abs() < 1.0e-4);

        let tiny = LocalLight::omni([0.0, 0.0, -10.0], [1.0; 3], 0.5);
        assert!(screen_faded_light(tiny, view).is_none());
    }
}
