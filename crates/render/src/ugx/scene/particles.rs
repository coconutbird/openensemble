//! Cached CPU assets for PFX attachments used by the current unit roster.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use glam::Mat4;
use pipeline::source::{AssetSource, StdFileProvider};
use sim::World as SimWorld;

use crate::particle::{
    ParticleEffect, ParticleEffectRuntime, ParticleEmitterKind, ParticleEmitterState,
    ParticleInstance, ParticleMaterial, ParticleNestedEvent, ParticleRenderContext,
    ParticleRenderer, ParticleScene, ParticleSceneTextures, ParticleTextureArray,
    canonical_effect_path,
};
use crate::ugx::{UnitAttachment, UnitAttachmentKind, UnitAttachmentTrigger, UnitRenderer};
use crate::{RenderPhase, WorldRenderer, wgpu};

use super::terrain_effects::TerrainEffectAssets;

#[derive(Clone, Debug)]
pub(super) struct ParticleAsset {
    pub(super) effect: ParticleEffect,
    pub(super) materials: Vec<Option<ParticleMaterial>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ParticleAssets {
    assets: HashMap<String, Option<Arc<ParticleAsset>>>,
    issues: Vec<String>,
}

impl ParticleAssets {
    pub(super) fn load_referenced(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        paths: impl IntoIterator<Item = String>,
    ) {
        let mut pending = paths.into_iter().collect::<VecDeque<_>>();
        while let Some(path) = pending.pop_front() {
            let key = canonical_effect_path(&path).to_ascii_lowercase();
            if self.assets.contains_key(&key) {
                continue;
            }
            self.assets.insert(key.clone(), None);
            let effect = match ParticleEffect::load(source, &path) {
                Ok(effect) => effect,
                Err(error) => {
                    self.issues.push(error.to_string());
                    continue;
                }
            };
            for nested in effect.emitters.iter().filter_map(|emitter| {
                if let ParticleEmitterKind::NestedEffect(path) = &emitter.kind {
                    Some(path.clone())
                } else {
                    None
                }
            }) {
                pending.push_back(nested);
            }
            let materials = effect
                .emitters
                .iter()
                .map(|emitter| {
                    if matches!(&emitter.kind, ParticleEmitterKind::NestedEffect(_)) {
                        return None;
                    }
                    match emitter.load_material(source) {
                        Ok(material) => {
                            if material.unavailable_texture_sets > 0 {
                                self.issues.push(format!(
                                    "particle effect '{key}' emitter '{}' disabled {} unavailable optional texture sets",
                                    emitter.name, material.unavailable_texture_sets
                                ));
                            }
                            let fallback_layers = material
                                .diffuse
                                .iter()
                                .chain(std::iter::once(&material.intensity))
                                .flatten()
                                .map(ParticleTextureArray::fallback_layer_count)
                                .sum::<usize>();
                            if fallback_layers > 0 {
                                self.issues.push(format!(
                                    "particle effect '{key}' emitter '{}' reused a decoded texture for {fallback_layers} missing stages",
                                    emitter.name
                                ));
                            }
                            Some(material)
                        }
                        Err(error) => {
                            self.issues.push(format!(
                                "particle effect '{key}' emitter '{}' material: {error}",
                                emitter.name
                            ));
                            None
                        }
                    }
                })
                .collect();
            self.assets
                .insert(key, Some(Arc::new(ParticleAsset { effect, materials })));
        }
    }

    pub(super) fn get(&self, path: &str) -> Option<&Arc<ParticleAsset>> {
        self.assets
            .get(&canonical_effect_path(path).to_ascii_lowercase())?
            .as_ref()
    }

    pub(super) fn loaded_count(&self) -> usize {
        self.assets.values().filter(|asset| asset.is_some()).count()
    }

    pub(super) fn issue_count(&self) -> usize {
        self.issues.len()
    }

    pub(super) fn issues(&self) -> &[String] {
        &self.issues
    }
}

impl super::UnitScene {
    /// Return the number of unique PFX graphs decoded for the current roster.
    #[must_use]
    pub fn particle_effect_count(&self) -> usize {
        self.particle_assets.loaded_count()
    }

    /// Return the number of missing or invalid PFX/material assets encountered.
    #[must_use]
    pub fn particle_effect_issue_count(&self) -> usize {
        self.particle_assets.issue_count()
    }

    /// Return diagnostics for missing or invalid PFX/material assets.
    #[must_use]
    pub fn particle_effect_issues(&self) -> &[String] {
        self.particle_assets.issues()
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EffectKey {
    owner_id: u64,
    attachment_index: usize,
    animation_revision: u32,
    asset_key: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct NestedEffectKey {
    parent_runtime_id: u64,
    emitter_index: usize,
    particle_id: u64,
}

struct LiveEffect {
    runtime_id: u64,
    asset_key: String,
    runtime: ParticleEffectRuntime,
    transform: Mat4,
    secondary_transform: Mat4,
    attached: bool,
    visible: bool,
    emitter_opacity: f32,
    auto_stop_seconds: Option<f32>,
}

struct PendingNestedEvents {
    parent_runtime_id: u64,
    parent_visible: bool,
    parent_opacity: f32,
    emitter_index: usize,
    events: Vec<ParticleNestedEvent>,
}

struct NestedSpawn {
    key: NestedEffectKey,
    path: String,
    transform: Mat4,
    visible: bool,
    emitter_opacity: f32,
}

#[derive(Clone, Copy)]
pub(super) struct ParticlePlacementState<'renderer> {
    pub(super) owner_id: u64,
    pub(super) animation_type: Option<&'renderer str>,
    pub(super) movement_animation_type: Option<&'renderer str>,
    pub(super) animation_revision: u32,
    pub(super) transform: Mat4,
    pub(super) secondary_transform: Mat4,
    pub(super) visible: bool,
    pub(super) emitter_opacity: f32,
    pub(super) renderer: &'renderer UnitRenderer,
    pub(super) world: &'renderer SimWorld,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct BatchKey {
    asset_key: String,
    emitter_index: usize,
}

struct ParticleBatch {
    key: BatchKey,
    material: ParticleMaterial,
    sort_particles: bool,
    instances: Vec<ParticleInstance>,
    renderer: ParticleRenderer,
}

pub(super) struct AttachedParticleRenderer {
    assets: ParticleAssets,
    terrain_effect_assets: TerrainEffectAssets,
    effects: BTreeMap<EffectKey, LiveEffect>,
    retired_effects: BTreeMap<u64, LiveEffect>,
    nested_effects: BTreeMap<NestedEffectKey, LiveEffect>,
    batches: Vec<ParticleBatch>,
    batch_indices: HashMap<BatchKey, usize>,
    depth: wgpu::TextureView,
    light_volume: Option<wgpu::TextureView>,
    color_format: wgpu::TextureFormat,
    last_time_seconds: Option<f32>,
    next_runtime_id: u64,
    one_shot_runtime_ids: BTreeMap<u64, u64>,
}

impl AttachedParticleRenderer {
    pub(super) fn new(
        color_format: wgpu::TextureFormat,
        scene_textures: ParticleSceneTextures<'_>,
    ) -> Self {
        Self {
            assets: ParticleAssets::default(),
            terrain_effect_assets: TerrainEffectAssets::default(),
            effects: BTreeMap::new(),
            retired_effects: BTreeMap::new(),
            nested_effects: BTreeMap::new(),
            batches: Vec::new(),
            batch_indices: HashMap::new(),
            depth: scene_textures.depth.clone(),
            light_volume: scene_textures.light_volume.cloned(),
            color_format,
            last_time_seconds: None,
            next_runtime_id: 1,
            one_shot_runtime_ids: BTreeMap::new(),
        }
    }

    pub(super) fn begin_sync(
        &mut self,
        assets: &ParticleAssets,
        terrain_effect_assets: &TerrainEffectAssets,
    ) {
        self.assets = assets.clone();
        self.terrain_effect_assets = terrain_effect_assets.clone();
        for effect in self.effects.values_mut() {
            effect.attached = false;
        }
    }

    pub(super) fn set_scene_textures(&mut self, scene_textures: ParticleSceneTextures<'_>) {
        self.depth = scene_textures.depth.clone();
        self.light_volume = scene_textures.light_volume.cloned();
        self.batches.clear();
        self.batch_indices.clear();
    }

    pub(super) fn spawn_one_shot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        presentation_id: u64,
        path: &str,
        transform: Mat4,
        lifespan_seconds: f32,
    ) {
        let Some(asset) = self.assets.get(path).cloned() else {
            return;
        };
        let asset_key = canonical_effect_path(path).to_ascii_lowercase();
        self.ensure_batches(device, queue, &asset_key, &asset);
        let runtime_id = self.allocate_runtime_id();
        let seed = one_shot_particle_seed(presentation_id, &asset_key);
        let auto_stop_seconds = if lifespan_seconds.is_finite() {
            lifespan_seconds.max(0.0)
        } else {
            3.0
        };
        self.retired_effects.insert(
            runtime_id,
            LiveEffect {
                runtime_id,
                asset_key,
                runtime: ParticleEffectRuntime::new(&asset.effect, seed, transform),
                transform,
                secondary_transform: transform,
                attached: false,
                visible: true,
                emitter_opacity: 1.0,
                auto_stop_seconds: Some(auto_stop_seconds),
            },
        );
        self.one_shot_runtime_ids
            .insert(presentation_id, runtime_id);
    }

    pub(super) fn update_one_shot_transform(&mut self, presentation_id: u64, transform: Mat4) {
        let Some(runtime_id) = self.one_shot_runtime_ids.get(&presentation_id) else {
            return;
        };
        if let Some(effect) = self.retired_effects.get_mut(runtime_id) {
            effect.transform = transform;
            effect.secondary_transform = transform;
        }
    }

    pub(super) fn sync_placement(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        placement: ParticlePlacementState<'_>,
    ) {
        for (attachment_index, attachment) in placement
            .renderer
            .attachments()
            .iter()
            .enumerate()
            .filter(|(_, attachment)| {
                active_particle(
                    attachment,
                    placement.animation_type,
                    placement.movement_animation_type,
                )
            })
        {
            self.sync_attachment(device, queue, placement, attachment_index, attachment);
        }
    }

    fn sync_attachment(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        placement: ParticlePlacementState<'_>,
        attachment_index: usize,
        attachment: &UnitAttachment,
    ) {
        let world_transform = attachment.world_transform(placement.transform);
        let Some(path) = attachment_particle_path(
            &self.terrain_effect_assets,
            attachment,
            world_transform,
            placement.world,
        ) else {
            return;
        };
        let Some(asset) = self.assets.get(&path).cloned() else {
            return;
        };
        let asset_key = canonical_effect_path(&path).to_ascii_lowercase();
        self.ensure_batches(device, queue, &asset_key, &asset);
        let key = effect_key(
            placement.owner_id,
            attachment_index,
            placement.animation_revision,
            &attachment.trigger,
            &asset_key,
        );
        let secondary_world_transform = attachment.world_transform(placement.secondary_transform);
        let seed = particle_seed(&key);
        if !self.effects.contains_key(&key) {
            let runtime_id = self.allocate_runtime_id();
            self.effects.insert(
                key.clone(),
                LiveEffect {
                    runtime_id,
                    asset_key,
                    runtime: ParticleEffectRuntime::new(&asset.effect, seed, world_transform),
                    transform: world_transform,
                    secondary_transform: secondary_world_transform,
                    attached: true,
                    visible: placement.visible,
                    emitter_opacity: placement.emitter_opacity,
                    auto_stop_seconds: None,
                },
            );
        }
        let effect = self
            .effects
            .get_mut(&key)
            .expect("particle effect was inserted above");
        effect.attached = true;
        effect.visible = placement.visible;
        effect.transform = world_transform;
        effect.secondary_transform = secondary_world_transform;
        effect.emitter_opacity = placement.emitter_opacity;
    }

    pub(super) fn end_sync(&mut self) {
        self.retire_detached();
    }

    fn retire_detached(&mut self) {
        let detached = self
            .effects
            .iter()
            .filter_map(|(key, effect)| (!effect.attached).then_some(key.clone()))
            .collect::<Vec<_>>();
        for key in detached {
            let mut effect = self
                .effects
                .remove(&key)
                .expect("detached effect key came from the active map");
            effect.runtime.stop();
            self.retired_effects.insert(effect.runtime_id, effect);
        }
    }

    pub(super) fn begin_frame(&mut self) {
        for effect in self.effects.values_mut() {
            effect.attached = false;
            effect.visible = false;
        }
    }

    pub(super) fn update_placement(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        placement: ParticlePlacementState<'_>,
    ) {
        for (attachment_index, attachment) in placement
            .renderer
            .attachments()
            .iter()
            .enumerate()
            .filter(|(_, attachment)| {
                active_particle(
                    attachment,
                    placement.animation_type,
                    placement.movement_animation_type,
                )
            })
        {
            self.sync_attachment(device, queue, placement, attachment_index, attachment);
        }
    }

    pub(super) fn finish_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        time_seconds: f32,
        scene: &ParticleScene,
        sun_color: [f32; 4],
    ) {
        self.retire_detached();
        let delta_seconds = self.last_time_seconds.map_or(0.0, |last| {
            if time_seconds.is_finite() {
                (time_seconds - last).max(0.0)
            } else {
                0.0
            }
        });
        self.last_time_seconds = time_seconds.is_finite().then_some(time_seconds);
        self.update_live_effects(device, queue, delta_seconds);
        self.effects.retain(|_, effect| live_effect_needed(effect));
        self.retired_effects
            .retain(|_, effect| live_effect_needed(effect));
        self.one_shot_runtime_ids
            .retain(|_, runtime_id| self.retired_effects.contains_key(runtime_id));
        self.nested_effects
            .retain(|_, effect| live_effect_needed(effect));
        self.ensure_live_batches(device, queue);
        self.upload_batches(device, queue, scene, sun_color);
    }

    fn ensure_live_batches(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let keys = self
            .effects
            .values()
            .chain(self.retired_effects.values())
            .chain(self.nested_effects.values())
            .map(|effect| effect.asset_key.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for key in keys {
            if let Some(asset) = self.assets.get(&key).cloned() {
                self.ensure_batches(device, queue, &key, &asset);
            }
        }
    }

    fn update_live_effects(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta_seconds: f32,
    ) {
        let mut pending = VecDeque::new();
        let mut parent_state = HashMap::new();
        for effect in self
            .effects
            .values_mut()
            .chain(self.retired_effects.values_mut())
        {
            update_live_effect(effect, delta_seconds);
            parent_state.insert(effect.runtime_id, (effect.visible, effect.emitter_opacity));
            drain_nested_events(effect, &mut pending);
        }
        let keys = self.nested_effects.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            let Some(effect) = self.nested_effects.get_mut(&key) else {
                continue;
            };
            if effect.attached
                && let Some(&(visible, opacity)) = parent_state.get(&key.parent_runtime_id)
            {
                effect.visible = visible;
                effect.emitter_opacity = opacity;
            }
            update_live_effect(effect, delta_seconds);
            parent_state.insert(effect.runtime_id, (effect.visible, effect.emitter_opacity));
            drain_nested_events(effect, &mut pending);
        }
        while let Some(events) = pending.pop_front() {
            self.apply_nested_events(device, queue, events, &mut pending);
        }
    }

    fn apply_nested_events(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: PendingNestedEvents,
        pending: &mut VecDeque<PendingNestedEvents>,
    ) {
        for event in source.events {
            let particle_id = match &event {
                ParticleNestedEvent::Spawn { particle_id, .. }
                | ParticleNestedEvent::Transform { particle_id, .. }
                | ParticleNestedEvent::Release { particle_id, .. } => *particle_id,
            };
            let key = NestedEffectKey {
                parent_runtime_id: source.parent_runtime_id,
                emitter_index: source.emitter_index,
                particle_id,
            };
            match event {
                ParticleNestedEvent::Spawn {
                    path, transform, ..
                } => self.spawn_nested_effect(
                    device,
                    queue,
                    NestedSpawn {
                        key,
                        path,
                        transform,
                        visible: source.parent_visible,
                        emitter_opacity: source.parent_opacity,
                    },
                    pending,
                ),
                ParticleNestedEvent::Transform { transform, .. } => {
                    if let Some(effect) = self.nested_effects.get_mut(&key) {
                        effect.transform = transform;
                        effect.secondary_transform = transform;
                        effect.visible = source.parent_visible;
                        effect.emitter_opacity = source.parent_opacity;
                    }
                }
                ParticleNestedEvent::Release {
                    kill_immediately, ..
                } => {
                    if let Some(effect) = self.nested_effects.get_mut(&key) {
                        effect.attached = false;
                        if kill_immediately {
                            effect.runtime.kill();
                        } else {
                            effect.runtime.stop();
                        }
                        drain_nested_events(effect, pending);
                    }
                }
            }
        }
    }

    fn spawn_nested_effect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        spawn: NestedSpawn,
        pending: &mut VecDeque<PendingNestedEvents>,
    ) {
        if let Some(effect) = self.nested_effects.get_mut(&spawn.key) {
            effect.attached = true;
            effect.transform = spawn.transform;
            effect.secondary_transform = spawn.transform;
            effect.visible = spawn.visible;
            effect.emitter_opacity = spawn.emitter_opacity;
            return;
        }
        let Some(asset) = self.assets.get(&spawn.path).cloned() else {
            return;
        };
        let asset_key = canonical_effect_path(&spawn.path).to_ascii_lowercase();
        self.ensure_batches(device, queue, &asset_key, &asset);
        let runtime_id = self.allocate_runtime_id();
        let seed = nested_particle_seed(&spawn.key);
        let mut effect = LiveEffect {
            runtime_id,
            asset_key,
            runtime: ParticleEffectRuntime::new(&asset.effect, seed, spawn.transform),
            transform: spawn.transform,
            secondary_transform: spawn.transform,
            attached: true,
            visible: spawn.visible,
            emitter_opacity: spawn.emitter_opacity,
            auto_stop_seconds: None,
        };
        drain_nested_events(&mut effect, pending);
        self.nested_effects.insert(spawn.key, effect);
    }

    fn allocate_runtime_id(&mut self) -> u64 {
        let id = self.next_runtime_id;
        self.next_runtime_id = self.next_runtime_id.wrapping_add(1).max(1);
        id
    }

    fn ensure_batches(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        asset_key: &str,
        asset: &ParticleAsset,
    ) {
        for (emitter_index, material) in asset.materials.iter().enumerate() {
            let Some(material) = material else {
                continue;
            };
            let key = BatchKey {
                asset_key: asset_key.to_owned(),
                emitter_index,
            };
            if self.batch_indices.contains_key(&key) {
                continue;
            }
            let scene_textures = ParticleSceneTextures {
                depth: &self.depth,
                light_volume: self.light_volume.as_ref(),
            };
            let renderer = match ParticleRenderer::new(
                device,
                queue,
                self.color_format,
                material,
                scene_textures,
            ) {
                Ok(renderer) => renderer,
                Err(error) => {
                    log::warn!(
                        "could not upload PFX '{asset_key}' emitter {emitter_index}: {error}"
                    );
                    continue;
                }
            };
            let batch_index = self.batches.len();
            self.batch_indices.insert(key.clone(), batch_index);
            self.batches.push(ParticleBatch {
                key,
                material: material.clone(),
                sort_particles: asset.effect.emitters[emitter_index].sort_particles,
                instances: Vec::new(),
                renderer,
            });
        }
    }

    fn upload_batches(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &ParticleScene,
        sun_color: [f32; 4],
    ) {
        for batch in &mut self.batches {
            batch.instances.clear();
        }
        for effect in self
            .effects
            .values()
            .chain(self.retired_effects.values())
            .chain(self.nested_effects.values())
            .filter(|effect| effect.visible)
        {
            append_effect_instances(
                &self.batch_indices,
                &mut self.batches,
                effect,
                scene,
                sun_color,
            );
        }
        for batch in &mut self.batches {
            if batch.sort_particles {
                let camera = glam::Vec3::from_array(scene.camera_position);
                batch.instances.sort_by(|left, right| {
                    let left = glam::Vec3::from_array(left.position).distance_squared(camera);
                    let right = glam::Vec3::from_array(right.position).distance_squared(camera);
                    right.total_cmp(&left)
                });
            }
            batch.renderer.update_scene(queue, scene);
            if let Err(error) = batch
                .renderer
                .update_instances(device, queue, &batch.instances)
            {
                log::warn!(
                    "could not update PFX '{}' emitter {}: {error}",
                    batch.key.asset_key,
                    batch.key.emitter_index
                );
            }
        }
    }

    pub(super) fn render_phase<'pass>(
        &'pass self,
        phase: RenderPhase,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        for batch in &self.batches {
            batch.renderer.render_phase(phase, pass);
        }
    }

    pub(super) fn live_effect_count(&self) -> usize {
        self.effects.len() + self.retired_effects.len() + self.nested_effects.len()
    }
}

fn active_particle(
    attachment: &UnitAttachment,
    animation_type: Option<&str>,
    movement_animation_type: Option<&str>,
) -> bool {
    matches!(
        attachment.kind,
        UnitAttachmentKind::Particle | UnitAttachmentKind::TerrainEffect
    ) && attachment
        .trigger
        .matches_animations(animation_type, movement_animation_type)
}

fn attachment_particle_path(
    terrain_effect_assets: &TerrainEffectAssets,
    attachment: &UnitAttachment,
    world_transform: Mat4,
    world: &SimWorld,
) -> Option<String> {
    let path = attachment.asset_path.as_deref()?;
    match attachment.kind {
        UnitAttachmentKind::Particle => Some(path.to_owned()),
        UnitAttachmentKind::TerrainEffect => {
            let surface_type = world.terrain_surface_type(world_transform.w_axis.truncate())?;
            terrain_effect_assets
                .particle_path(path, surface_type)
                .map(str::to_owned)
        }
        _ => None,
    }
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

fn particle_seed(key: &EffectKey) -> u32 {
    fold_u64(key.owner_id)
        ^ u32::try_from(key.attachment_index)
            .unwrap_or(u32::MAX)
            .wrapping_mul(0x9e37_79b9)
        ^ key.animation_revision.rotate_left(13)
}

fn nested_particle_seed(key: &NestedEffectKey) -> u32 {
    let parent = fold_u64(key.parent_runtime_id);
    let particle = fold_u64(key.particle_id);
    parent
        ^ u32::try_from(key.emitter_index)
            .unwrap_or(u32::MAX)
            .wrapping_mul(0x9e37_79b9)
        ^ particle.rotate_left(13)
}

fn one_shot_particle_seed(presentation_id: u64, asset_key: &str) -> u32 {
    let mut hash = presentation_id ^ 0xa409_3822_299f_31d0;
    for byte in asset_key.bytes() {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    fold_u64(hash ^ (hash >> 29))
}

fn fold_u64(value: u64) -> u32 {
    let bytes = value.to_le_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        ^ u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
}

fn drain_nested_events(effect: &mut LiveEffect, pending: &mut VecDeque<PendingNestedEvents>) {
    for (emitter_index, emitter) in effect.runtime.emitters_mut().iter_mut().enumerate() {
        let events = emitter.take_nested_events();
        if !events.is_empty() {
            pending.push_back(PendingNestedEvents {
                parent_runtime_id: effect.runtime_id,
                parent_visible: effect.visible,
                parent_opacity: effect.emitter_opacity,
                emitter_index,
                events,
            });
        }
    }
}

fn update_live_effect(effect: &mut LiveEffect, delta_seconds: f32) {
    let Some(remaining) = effect.auto_stop_seconds else {
        effect.runtime.update_with_secondary(
            delta_seconds,
            effect.transform,
            effect.secondary_transform,
        );
        return;
    };
    let emitting_seconds = remaining.min(delta_seconds);
    effect.runtime.update_with_secondary(
        emitting_seconds,
        effect.transform,
        effect.secondary_transform,
    );
    let remaining = (remaining - emitting_seconds).max(0.0);
    if remaining > 0.0 {
        effect.auto_stop_seconds = Some(remaining);
        return;
    }
    effect.auto_stop_seconds = None;
    effect.runtime.stop();
    let draining_seconds = delta_seconds - emitting_seconds;
    if draining_seconds > 0.0 {
        effect.runtime.update_with_secondary(
            draining_seconds,
            effect.transform,
            effect.secondary_transform,
        );
    }
}

fn live_effect_needed(effect: &LiveEffect) -> bool {
    effect.attached
        || effect
            .runtime
            .emitters()
            .iter()
            .any(|emitter| emitter.state() != ParticleEmitterState::Killed)
}

fn append_effect_instances(
    batch_indices: &HashMap<BatchKey, usize>,
    batches: &mut [ParticleBatch],
    effect: &LiveEffect,
    scene: &ParticleScene,
    sun_color: [f32; 4],
) {
    let context = ParticleRenderContext {
        sun_color,
        emitter_opacity: effect.emitter_opacity,
        camera_position: scene.camera_position,
        ..ParticleRenderContext::default()
    };
    for (emitter_index, emitter) in effect.runtime.emitters().iter().enumerate() {
        let key = BatchKey {
            asset_key: effect.asset_key.clone(),
            emitter_index,
        };
        let Some(&batch_index) = batch_indices.get(&key) else {
            continue;
        };
        let batch = &mut batches[batch_index];
        batch
            .instances
            .extend(emitter.instances(&batch.material, context));
    }
}

#[cfg(test)]
mod tests {
    use crate::particle::canonical_effect_path;

    #[test]
    fn attachment_and_nested_paths_share_one_cache_identity() {
        assert_eq!(
            canonical_effect_path("effects/fire/test.pfx.xmb").to_ascii_lowercase(),
            canonical_effect_path("ART\\EFFECTS\\FIRE\\TEST").to_ascii_lowercase()
        );
    }
}
