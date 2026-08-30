//! Installed-asset validation for visual animation clips and event tags.

use std::collections::{BTreeMap, BTreeSet};

use pipeline::database::hw1::visual::{Anim, Model, Visual, VisualTag};
use render::terrain_effect::{TerrainEffect, TerrainEffectAction};

#[test]
#[ignore = "requires OPENENSEMBLE_GAME_DIR pointing at Halo Wars DE"]
fn all_shipped_visual_animation_tags_decode() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point at Halo Wars DE");
    let mut source = pipeline::hw1::loader::load_game_dir(&game_dir);
    let paths = installed_visual_paths(&source);
    let stats = scan_visuals(&mut source, &paths);
    stats.report();
    println!(
        "terrain_tag_actions {:?}",
        terrain_tag_actions(&mut source, stats.terrain_tag_effects.keys())
    );
    stats.assert_installed_counts(paths.len());
}

type InstalledSource = pipeline::source::AssetSource<pipeline::source::StdFileProvider>;

fn installed_visual_paths(source: &InstalledSource) -> BTreeSet<String> {
    source
        .files_per_archive()
        .into_iter()
        .flat_map(|(_, paths)| paths)
        .filter(|path| path.ends_with(".vis.xmb"))
        .map(str::to_owned)
        .collect()
}

#[derive(Default)]
struct CorpusStats {
    animation_types: BTreeMap<String, usize>,
    exit_actions: BTreeMap<String, usize>,
    animation_chains: usize,
    transition_chain_lengths: BTreeMap<usize, usize>,
    transition_chain_endings: BTreeMap<&'static str, usize>,
    transition_target_resolutions: BTreeMap<&'static str, usize>,
    transition_target_issues: Vec<String>,
    tag_types: BTreeMap<String, usize>,
    terrain_tag_count: usize,
    terrain_tag_animations: BTreeMap<String, usize>,
    terrain_tag_effects: BTreeMap<String, usize>,
    camera_shake_tags: BTreeMap<String, usize>,
    terrain_alpha_tags: BTreeMap<String, usize>,
    ground_ik_tags: BTreeMap<String, usize>,
    ground_ik_sources: BTreeMap<String, usize>,
    simulation_tag_animations: BTreeMap<(String, String), usize>,
    rare_simulation_tag_sources: Vec<String>,
    ordinary_physics_impulse_tags: BTreeMap<String, usize>,
    movement_attachments: BTreeMap<(String, String), usize>,
    movement_transitions: Vec<String>,
    animation_assets: usize,
    animations: usize,
    visuals: usize,
}

fn scan_visuals(source: &mut InstalledSource, paths: &BTreeSet<String>) -> CorpusStats {
    let mut stats = CorpusStats::default();
    for path in paths {
        let bytes = source
            .resolve_with_fallback(path, &[])
            .unwrap_or_else(|| panic!("missing indexed visual {path}"));
        let document =
            pipeline::xmb::Reader::read(&bytes).unwrap_or_else(|error| panic!("{path}: {error}"));
        let visual = pipeline::database::hw1::visual::parse(&document)
            .unwrap_or_else(|error| panic!("{path}: {error}"));
        scan_visual(path, &visual, &mut stats);
    }
    stats
}

fn scan_visual(path: &str, visual: &Visual, stats: &mut CorpusStats) {
    stats.visuals += 1;
    for model in &visual.models {
        audit_transition_targets(path, model, stats);
        audit_transition_chains(model, stats);
        for animation in &model.anims {
            scan_animation(path, &model.name, animation, stats);
        }
    }
}

fn scan_animation(path: &str, model_name: &str, animation: &Anim, stats: &mut CorpusStats) {
    stats.animations += 1;
    *stats
        .animation_types
        .entry(animation.anim_type.to_ascii_lowercase())
        .or_default() += 1;
    let exit_action = animation
        .exit_action
        .as_deref()
        .unwrap_or("Loop")
        .to_ascii_lowercase();
    *stats.exit_actions.entry(exit_action).or_default() += 1;
    if animation
        .exit_action
        .as_deref()
        .is_some_and(|action| action.eq_ignore_ascii_case("Transition"))
    {
        stats.animation_chains += 1;
    }
    if is_movement_animation(&animation.anim_type) {
        for attachment in &animation.attachments {
            *stats
                .movement_attachments
                .entry((
                    animation.anim_type.to_ascii_lowercase(),
                    attachment.attach_type.to_ascii_lowercase(),
                ))
                .or_default() += 1;
        }
    }
    if is_movement_transition(&animation.anim_type) {
        let assets = animation
            .assets
            .iter()
            .filter_map(|asset| asset.file.as_deref())
            .collect::<Vec<_>>();
        stats.movement_transitions.push(format!(
            "{path} :: {model_name} :: {} exit={:?} tween={:?} to={:?} assets={assets:?}",
            animation.anim_type,
            animation.exit_action,
            animation.tween_time,
            animation.tween_to_animation,
        ));
    }
    for asset in &animation.assets {
        stats.animation_assets += 1;
        for tag in &asset.tags {
            stats.record_tag(
                path,
                model_name,
                &animation.anim_type,
                asset.file.as_deref().unwrap_or_default(),
                tag,
            );
        }
    }
}

fn audit_transition_targets(path: &str, model: &Model, stats: &mut CorpusStats) {
    for animation in model.anims.iter().filter(|animation| {
        animation
            .exit_action
            .as_deref()
            .is_some_and(|action| action.eq_ignore_ascii_case("Transition"))
    }) {
        let target = animation
            .tween_to_animation
            .as_deref()
            .map(str::trim)
            .filter(|target| !target.is_empty());
        let resolution = target.map_or("missing", |target| {
            if has_animation(model, target) {
                "exact"
            } else if transition_substitute(target).is_some_and(|name| has_animation(model, name)) {
                "substitute"
            } else {
                "missing"
            }
        });
        *stats
            .transition_target_resolutions
            .entry(resolution)
            .or_default() += 1;
        if resolution == "missing" {
            stats.transition_target_issues.push(format!(
                "{path} :: {} :: {} -> {:?}",
                model.name, animation.anim_type, animation.tween_to_animation
            ));
        }
    }
}

fn audit_transition_chains(model: &Model, stats: &mut CorpusStats) {
    for start in 0..model.anims.len() {
        if !is_transition(&model.anims[start]) {
            continue;
        }
        let mut visited = BTreeSet::new();
        let mut current = start;
        let ending = loop {
            if !visited.insert(current) {
                break "multi_cycle";
            }
            let Some(animation) = model.anims.get(current) else {
                break "missing_animation";
            };
            if !is_transition(animation) {
                break "terminal";
            }
            let Some(target) = animation.tween_to_animation.as_deref() else {
                break "missing_target";
            };
            let Some(next) = model
                .anims
                .iter()
                .position(|candidate| candidate.anim_type.eq_ignore_ascii_case(target))
            else {
                break "unresolved_target";
            };
            if visited.contains(&next) {
                break if next == current {
                    "self_cycle"
                } else {
                    "multi_cycle"
                };
            }
            current = next;
        };
        *stats
            .transition_chain_lengths
            .entry(visited.len())
            .or_default() += 1;
        *stats.transition_chain_endings.entry(ending).or_default() += 1;
    }
}

fn is_transition(animation: &Anim) -> bool {
    animation
        .exit_action
        .as_deref()
        .is_some_and(|action| action.eq_ignore_ascii_case("Transition"))
}

fn has_animation(model: &Model, name: &str) -> bool {
    model
        .anims
        .iter()
        .any(|animation| animation.anim_type.eq_ignore_ascii_case(name))
}

fn transition_substitute(name: &str) -> Option<&'static str> {
    if name.eq_ignore_ascii_case("Sprint") {
        Some("Run")
    } else if name.eq_ignore_ascii_case("Recover")
        || name.eq_ignore_ascii_case("Research")
        || name.eq_ignore_ascii_case("Train")
    {
        Some("Idle")
    } else if name.eq_ignore_ascii_case("Run") || name.eq_ignore_ascii_case("Jog") {
        Some("Walk")
    } else {
        None
    }
}

fn is_movement_animation(animation_type: &str) -> bool {
    ["walk", "jog", "run", "turnleft", "turnright", "walkidle"]
        .iter()
        .any(|kind| animation_type.eq_ignore_ascii_case(kind))
}

fn is_movement_transition(animation_type: &str) -> bool {
    [
        "idlewalk", "idlejog", "idlerun", "walkidle", "jogidle", "runidle", "turnwalk", "turnjog",
        "turnrun",
    ]
    .iter()
    .any(|kind| animation_type.eq_ignore_ascii_case(kind))
}

impl CorpusStats {
    fn record_tag(
        &mut self,
        path: &str,
        model_name: &str,
        animation_type: &str,
        asset_file: &str,
        tag: &VisualTag,
    ) {
        assert!(
            tag.position.is_none_or(f32::is_finite),
            "{path}: non-finite {} tag position",
            tag.tag_type
        );
        *self
            .tag_types
            .entry(tag.tag_type.to_ascii_lowercase())
            .or_default() += 1;
        if simulation_tag(&tag.tag_type) {
            *self
                .simulation_tag_animations
                .entry((
                    tag.tag_type.to_ascii_lowercase(),
                    animation_type.to_ascii_lowercase(),
                ))
                .or_default() += 1;
            if tag.tag_type.eq_ignore_ascii_case("AttachTarget")
                || tag.tag_type.eq_ignore_ascii_case("SweetSpot")
            {
                self.rare_simulation_tag_sources.push(format!(
                    "{path} :: {model_name} :: {animation_type} :: {asset_file} :: {tag:?}"
                ));
            }
            if tag.tag_type.eq_ignore_ascii_case("PhysicsImpulse")
                && !animation_type.to_ascii_lowercase().contains("hijack")
            {
                *self
                    .ordinary_physics_impulse_tags
                    .entry(format!(
                        "{model_name} :: {animation_type} :: {asset_file} bone={:?} position={:?} kind={:?} force=({:?},{:?},{:?}) attached={:?}",
                        tag.to_bone,
                        tag.position,
                        tag.start,
                        tag.force,
                        tag.force2,
                        tag.lifespan,
                        tag.check_selected,
                    ))
                    .or_default() += 1;
            }
        }
        if tag.tag_type.eq_ignore_ascii_case("TerrainEffect") {
            self.terrain_tag_count += 1;
            *self
                .terrain_tag_animations
                .entry(animation_type.to_ascii_lowercase())
                .or_default() += 1;
            *self
                .terrain_tag_effects
                .entry(tag.name.as_deref().unwrap_or_default().to_ascii_lowercase())
                .or_default() += 1;
        } else if tag.tag_type.eq_ignore_ascii_case("CameraShake") {
            *self
                .camera_shake_tags
                .entry(format!(
                    "force={:?} lifespan={:?} selected={:?}",
                    tag.force, tag.lifespan, tag.check_selected
                ))
                .or_default() += 1;
        } else if tag.tag_type.eq_ignore_ascii_case("TerrainAlpha") {
            *self
                .terrain_alpha_tags
                .entry(tag.user_data.clone().unwrap_or_default())
                .or_default() += 1;
        } else if tag.tag_type.eq_ignore_ascii_case("GroundIK") {
            self.record_ground_ik(path, model_name, animation_type, asset_file, tag);
        }
    }

    fn record_ground_ik(
        &mut self,
        path: &str,
        model_name: &str,
        animation_type: &str,
        asset_file: &str,
        tag: &VisualTag,
    ) {
        *self
            .ground_ik_tags
            .entry(format!(
                "position={:?} end={:?} lock={:?} visible={:?}",
                tag.position, tag.end, tag.lock_to_ground, tag.check_visible
            ))
            .or_default() += 1;
        *self
            .ground_ik_sources
            .entry(format!(
                "{path} :: {model_name} :: {animation_type} :: {asset_file} :: bone={:?} position={:?} end={:?} lock={:?}",
                tag.to_bone, tag.position, tag.end, tag.lock_to_ground,
            ))
            .or_default() += 1;
    }

    fn report(&self) {
        println!("visuals {}", self.visuals);
        println!("animations {}", self.animations);
        println!("animation_assets {}", self.animation_assets);
        println!("animation_type_count {}", self.animation_types.len());
        println!("animation_types {:?}", self.animation_types);
        println!("exit_actions {:?}", self.exit_actions);
        println!("animation_chains {}", self.animation_chains);
        println!(
            "transition_chain_lengths {:?}",
            self.transition_chain_lengths
        );
        println!(
            "transition_chain_endings {:?}",
            self.transition_chain_endings
        );
        println!(
            "transition_target_resolutions {:?}",
            self.transition_target_resolutions
        );
        for issue in &self.transition_target_issues {
            println!("transition_target_issue {issue}");
        }
        println!("tag_types {:?}", self.tag_types);
        println!("terrain_tag_animations {:?}", self.terrain_tag_animations);
        println!("terrain_tag_effects {:?}", self.terrain_tag_effects);
        println!("camera_shake_tags {:?}", self.camera_shake_tags);
        println!("terrain_alpha_tags {:?}", self.terrain_alpha_tags);
        println!("ground_ik_tags {:?}", self.ground_ik_tags);
        println!("ground_ik_sources {:?}", self.ground_ik_sources);
        for source in self.ground_ik_sources.keys() {
            println!("ground_ik_source {source}");
        }
        println!(
            "simulation_tag_animations {:?}",
            self.simulation_tag_animations
        );
        for source in &self.rare_simulation_tag_sources {
            println!("rare_simulation_tag_source {source}");
        }
        println!(
            "ordinary_physics_impulse_tags {:?}",
            self.ordinary_physics_impulse_tags
        );
        println!("movement_attachments {:?}", self.movement_attachments);
        for transition in &self.movement_transitions {
            println!("movement_transition {transition}");
        }
    }

    fn assert_installed_counts(&self, path_count: usize) {
        assert_eq!(self.visuals, path_count);
        assert_eq!(self.visuals, 2_726, "unexpected installed VIS corpus size");
        assert_eq!(
            self.animations, 6_623,
            "unexpected installed animation corpus size"
        );
        assert_eq!(
            self.animation_assets, 9_837,
            "unexpected installed animation asset corpus size"
        );
        assert_eq!(
            self.terrain_tag_count, 153,
            "unexpected terrain-effect tag corpus size"
        );
    }
}

fn simulation_tag(tag_type: &str) -> bool {
    tag_type.eq_ignore_ascii_case("AttachTarget")
        || tag_type.eq_ignore_ascii_case("GroundIK")
        || tag_type.eq_ignore_ascii_case("KillAndThrow")
        || tag_type.eq_ignore_ascii_case("PhysicsImpulse")
        || tag_type.eq_ignore_ascii_case("SweetSpot")
}

fn terrain_tag_actions<'a>(
    source: &mut InstalledSource,
    paths: impl Iterator<Item = &'a String>,
) -> BTreeMap<&'static str, usize> {
    let mut terrain_tag_actions = BTreeMap::<&'static str, usize>::new();
    for path in paths {
        let effect = TerrainEffect::load(source, path)
            .unwrap_or_else(|error| panic!("terrain-effect animation tag {path}: {error}"));
        for action in effect.surfaces.iter().flat_map(|surface| &surface.actions) {
            let name = match action {
                TerrainEffectAction::Sound(_) => "sound",
                TerrainEffectAction::Particle(_) => "particle",
                TerrainEffectAction::Trail(_) => "trail",
                TerrainEffectAction::ImpactDecal(_) => "impact_decal",
                TerrainEffectAction::Light(_) => "light",
                TerrainEffectAction::Visual(_) => "visual",
            };
            *terrain_tag_actions.entry(name).or_default() += 1;
        }
    }
    terrain_tag_actions
}
