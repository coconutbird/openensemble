//! Read-only inventory of shipped HW1 visual and UGX renderer features.

use std::collections::{BTreeMap, BTreeSet};

use glam::FloatExt;
use num_traits::ToPrimitive;
use pipeline::hw1;
use pipeline::ugx::Reader;
use pipeline::ugx::types::material::material_flags;

type Source = pipeline::source::AssetSource<pipeline::source::StdFileProvider>;

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let arguments = std::env::args().collect::<Vec<_>>();
    let scenario = arguments
        .iter()
        .skip(1)
        .find(|argument| !argument.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "blood_gulch".to_owned());
    let game_dir = data::paths::game_dir().to_string_lossy().into_owned();
    let (mut world, mut source) = hw1::World::load(&game_dir)?;
    world.swap_scenario(&mut source, &scenario);

    if arguments
        .iter()
        .any(|argument| argument == "--placements" || argument == "--placements-only")
    {
        print_scenario_placements(&world);
    }
    if arguments
        .iter()
        .any(|argument| argument == "--placement-height-oracle")
    {
        print_placement_height_oracle(&world);
    }
    if arguments
        .iter()
        .any(|argument| argument == "--placements-only")
    {
        return Ok(());
    }

    let mut report = audit_models(&world, &mut source);
    audit_visuals(&world, &mut report);
    print_report(&scenario, &report, &source);
    if arguments.iter().any(|argument| argument == "--particles") {
        let particle_report = audit_particles(&mut source);
        println!("particle effect audit:");
        println!("  files: {}", particle_report.files);
        println!("{particle_report:#?}");
    }
    if arguments
        .iter()
        .any(|argument| argument == "--terrain-effects")
    {
        let terrain_effect_report = audit_terrain_effects(&mut source);
        println!("terrain effect audit:");
        println!("  files: {}", terrain_effect_report.files);
        println!("{terrain_effect_report:#?}");
        print_terrain_effect_resolutions(&terrain_effect_report, &source);
    }
    Ok(())
}

fn print_placement_height_oracle(world: &hw1::World) {
    let Some(scenario) = &world.scenario_data else {
        println!("scenario has no decoded placement data");
        return;
    };
    let Some(raw) = world
        .terrain_data
        .as_ref()
        .and_then(|terrain| terrain.extract_raw_data().ok())
    else {
        println!("scenario has no decoded terrain heightfield");
        return;
    };
    let mut direct_error = 0.0_f32;
    let mut transposed_error = 0.0_f32;
    let mut sample_count = 0_u32;
    println!("placement height oracle (delta from authored Y):");
    for object in scenario.objects() {
        let position = object.position_vec3();
        let direct = height_delta(&raw, position[0], position[2], position[1]);
        let transposed = height_delta(&raw, position[2], position[0], position[1]);
        if let (Some(direct), Some(transposed)) = (direct, transposed) {
            direct_error += direct.abs();
            transposed_error += transposed.abs();
            sample_count += 1;
        }
        let proto = object.proto_name.to_ascii_lowercase();
        if proto.contains("teleporter") || proto.contains("base_socket") {
            println!(
                "  id={} proto={:?} direct={direct:?} transposed={transposed:?}",
                object.id, object.proto_name,
            );
        }
    }
    if let Some(sample_count) = sample_count.to_f32().filter(|count| *count > 0.0) {
        println!(
            "  all-object mean absolute delta: direct={:.3} transposed={:.3}",
            direct_error / sample_count,
            transposed_error / sample_count,
        );
    }
}

fn height_delta(
    raw: &pipeline::xtd::RawTerrainData,
    world_x: f32,
    world_z: f32,
    authored_y: f32,
) -> Option<f32> {
    terrain_height(raw, world_x, world_z).map(|height| height - authored_y)
}

fn terrain_height(raw: &pipeline::xtd::RawTerrainData, world_x: f32, world_z: f32) -> Option<f32> {
    let dimension = raw.num_verts_per_axis;
    let last = dimension.checked_sub(1)?;
    let scale = raw.tile_scale.abs().max(f32::EPSILON);
    let grid_x = ((world_x - raw.world_min[0]) / scale).clamp(0.0, last.to_f32()?);
    let grid_z = ((world_z - raw.world_min[2]) / scale).clamp(0.0, last.to_f32()?);
    let x0 = grid_x.floor().to_u32()?;
    let z0 = grid_z.floor().to_u32()?;
    let x1 = x0.saturating_add(1).min(last);
    let z1 = z0.saturating_add(1).min(last);
    let tx = grid_x - x0.to_f32()?;
    let tz = grid_z - z0.to_f32()?;
    let h00 = packed_height(raw, x0, z0)?;
    let h10 = packed_height(raw, x1, z0)?;
    let h01 = packed_height(raw, x0, z1)?;
    let h11 = packed_height(raw, x1, z1)?;
    Some(h00.lerp(h10, tx).lerp(h01.lerp(h11, tx), tz))
}

fn packed_height(raw: &pipeline::xtd::RawTerrainData, x: u32, z: u32) -> Option<f32> {
    let index = z.checked_mul(raw.num_verts_per_axis)?.checked_add(x)?;
    let packed = *raw.packed_positions.get(index.to_usize()?)?;
    let normalized = ((packed >> 10) & 0x3ff).to_f32()? / 1023.0;
    Some((normalized - render::terrain::NORMALIZED_TERRAIN_Y_OFFSET) * raw.range[1] - raw.mid[1])
}

fn print_scenario_placements(world: &hw1::World) {
    let Some(scenario) = &world.scenario_data else {
        println!("scenario has no decoded placement data");
        return;
    };
    println!("player start positions:");
    for position in scenario.positions() {
        let authored_position = position.position_vec3();
        println!(
            "  player={} number={} authored={authored_position:?} world={:?} forward={:?}",
            position.player,
            position.number,
            authored_position,
            position.forward_vec3(),
        );
    }
    println!("scenario objects:");
    for object in scenario.objects() {
        let authored_position = object.position_vec3();
        println!(
            "  id={} proto={:?} editor={:?} authored={authored_position:?} world={:?} forward={:?} right={:?}",
            object.id,
            object.proto_name,
            object.editor_name,
            render::ugx::scenario_object_position_to_world(authored_position),
            object.forward_vec3(),
            object.right_vec3(),
        );
    }
}

fn audit_terrain_effects(source: &mut Source) -> TerrainEffectReport {
    let paths = source
        .files_per_archive()
        .into_iter()
        .flat_map(|(_, files)| files.into_iter())
        .filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("tfx"))
        })
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut report = TerrainEffectReport {
        files: paths.len(),
        ..TerrainEffectReport::default()
    };
    for path in &paths {
        match render::terrain_effect::TerrainEffect::load(source, path) {
            Ok(effect) => {
                report.adapted += 1;
                report.surfaces += effect.surfaces.len();
                for surface in effect.surfaces {
                    *report.surface_types.entry(surface.name).or_default() += 1;
                    for action in surface.actions {
                        let name = match &action {
                            render::terrain_effect::TerrainEffectAction::Particle(_) => "particle",
                            render::terrain_effect::TerrainEffectAction::ImpactDecal(_) => {
                                "impact_decal"
                            }
                            render::terrain_effect::TerrainEffectAction::Trail(_) => "trail",
                            render::terrain_effect::TerrainEffectAction::Visual(_) => "visual",
                            render::terrain_effect::TerrainEffectAction::Light(_) => "light",
                            render::terrain_effect::TerrainEffectAction::Sound(_) => "sound",
                        };
                        *report.actions.entry(name).or_default() += 1;
                        let samples = report.action_samples.entry(name).or_default();
                        if samples.len() < 16
                            && !samples.iter().any(|sample| sample == action.value())
                        {
                            samples.push(action.value().to_owned());
                        }
                    }
                }
            }
            Err(error) => report.errors.push(format!("{path}: {error}")),
        }
    }
    report
}

fn audit_models(world: &hw1::World, source: &mut Source) -> Report {
    let mut report = Report::default();
    for path in &world.manifest.model_refs {
        report.model_refs += 1;
        let Some(bytes) = source.resolve_with_fallback(path, &[".ugx"]) else {
            report.models_missing += 1;
            continue;
        };
        let Ok(geometry) = Reader::read(&bytes) else {
            report.models_invalid += 1;
            continue;
        };
        report.models_parsed += 1;
        report.sections += geometry.sections.len();
        for material in &geometry.materials {
            report.materials += 1;
            let Some(legacy) = material.legacy() else {
                report.hogan_materials += 1;
                continue;
            };
            report.legacy_materials += 1;
            *report.blend_types.entry(legacy.blend_type).or_default() += 1;
            let identity = format!("{path} :: {}", material.name);
            count_flag(
                &mut report,
                "color_gloss",
                legacy.flags,
                material_flags::COLOR_GLOSS,
            );
            count_flag(
                &mut report,
                "opacity_valid",
                legacy.flags,
                material_flags::OPACITY_VALID,
            );
            count_flag(
                &mut report,
                "two_sided",
                legacy.flags,
                material_flags::TWO_SIDED,
            );
            count_flag(
                &mut report,
                "disable_shadows",
                legacy.flags,
                material_flags::DISABLE_SHADOWS,
            );
            count_flag(
                &mut report,
                "global_environment",
                legacy.flags,
                material_flags::GLOBAL_ENV,
            );
            count_flag(
                &mut report,
                "terrain_conform",
                legacy.flags,
                material_flags::TERRAIN_CONFORM,
            );
            count_flag(
                &mut report,
                "local_reflection",
                legacy.flags,
                material_flags::LOCAL_REFLECTION,
            );
            count_flag(
                &mut report,
                "disable_shadow_reception",
                legacy.flags,
                material_flags::DISABLE_SHADOW_RECEPTION,
            );
            if legacy.flags & material_flags::TERRAIN_CONFORM != 0 {
                report.terrain_conform_materials.push(identity.clone());
            }
            if legacy.flags & material_flags::LOCAL_REFLECTION != 0 {
                report.local_reflection_materials.push(identity.clone());
            }
            for (index, maps) in legacy.maps.iter().enumerate() {
                if !maps.is_empty() {
                    *report.map_slots.entry(index).or_default() += 1;
                }
                if maps.len() > 1 {
                    *report.multi_map_slots.entry(index).or_default() += 1;
                }
                if !maps.is_empty() && index >= 9 {
                    report
                        .uncommon_map_materials
                        .push(format!("slot {index}: {identity}"));
                }
            }
        }
    }
    report
}

fn audit_visuals(world: &hw1::World, report: &mut Report) {
    for visual in world.visuals.values() {
        for model in &visual.models {
            if let Some(component) = &model.component {
                for attachment in &component.attachments {
                    report.record_attachment(&attachment.attach_type, &attachment.name);
                    *report
                        .component_attachment_types
                        .entry(attachment.attach_type.to_ascii_lowercase())
                        .or_default() += 1;
                }
                for point in &component.points {
                    *report
                        .point_types
                        .entry(point.point_type.to_ascii_lowercase())
                        .or_default() += 1;
                }
            }
            for animation in &model.anims {
                for attachment in &animation.attachments {
                    report.record_attachment(&attachment.attach_type, &attachment.name);
                    *report
                        .animation_attachment_types
                        .entry(attachment.attach_type.to_ascii_lowercase())
                        .or_default() += 1;
                }
            }
        }
    }
}

fn print_report(scenario: &str, report: &Report, source: &Source) {
    println!("renderer asset audit for {scenario}");
    println!("{report:#?}");
    let archive_files = source.files_per_archive();
    println!("attachment sample resolutions:");
    for (attach_type, samples) in &report.attachment_samples {
        println!("  {attach_type}:");
        for sample in samples.iter().take(8) {
            let needle = format!("art\\{}", sample.to_ascii_lowercase());
            let matches = archive_files
                .iter()
                .flat_map(|(_, files)| files)
                .copied()
                .filter(|path| path.starts_with(&needle))
                .take(8)
                .collect::<Vec<_>>();
            println!("    {sample}: {matches:?}");
        }
    }
}

fn print_terrain_effect_resolutions(report: &TerrainEffectReport, source: &Source) {
    let archive_files = source.files_per_archive();
    println!("terrain effect action sample resolutions:");
    for (action, samples) in &report.action_samples {
        println!("  {action}:");
        for sample in samples {
            let needle = format!("art\\{}", sample.replace('/', "\\").to_ascii_lowercase());
            let matches = archive_files
                .iter()
                .flat_map(|(_, files)| files)
                .copied()
                .filter(|path| path.starts_with(&needle))
                .take(8)
                .collect::<Vec<_>>();
            println!("    {sample}: {matches:?}");
        }
    }
}

fn audit_particles(source: &mut Source) -> ParticleReport {
    let paths = source
        .files_per_archive()
        .into_iter()
        .flat_map(|(_, files)| files.into_iter().map(str::to_owned))
        .filter(|path| path.ends_with(".pfx.xmb"))
        .collect::<BTreeSet<_>>();
    let mut report = ParticleReport {
        files: paths.len(),
        ..ParticleReport::default()
    };

    for path in paths {
        let Some(bytes) = source.resolve_with_fallback(&path, &[]) else {
            report.missing += 1;
            continue;
        };
        let Ok(document) = pipeline::xmb::Reader::read(&bytes) else {
            report.invalid += 1;
            continue;
        };
        let Some(root) = document.root() else {
            report.invalid += 1;
            continue;
        };
        report.parsed += 1;
        match render::particle::ParticleEffect::from_document(&document) {
            Ok(effect) => {
                report.renderer_adapted += 1;
                report.renderer_emitters += effect.emitters.len();
                for emitter in effect.emitters {
                    report.renderer_particle_budget += u64::from(emitter.max_particles);
                    report.renderer_active_emitters += usize::from(emitter.active);
                    match emitter.kind {
                        render::particle::ParticleEmitterKind::Render(_) => {
                            report.renderer_render_emitters += 1;
                        }
                        render::particle::ParticleEmitterKind::NestedEffect(_) => {
                            report.renderer_nested_emitters += 1;
                        }
                    }
                }
            }
            Err(error) => {
                report.renderer_invalid += 1;
                if report.renderer_errors.len() < 16 {
                    report.renderer_errors.push(format!("{path}: {error}"));
                }
            }
        }
        audit_particle_node(root, &mut report);
    }
    report
}

fn audit_particle_node(node: &pipeline::xmb::Node, report: &mut ParticleReport) {
    if node.name.eq_ignore_ascii_case("ParticleEmitter") {
        report.emitters += 1;
    }
    if node.children.is_empty() {
        let value = node.text_string();
        if matches!(
            node.name.as_str(),
            "BeamAlignmentType"
                | "BlendMode"
                | "DiffuseLayer1To2BlendMode"
                | "DiffuseLayer2To3BlendMode"
                | "MagnetType"
                | "ParticleType"
                | "ShapeType"
                | "TrailEmissionType"
                | "TrailUVType"
                | "Type"
        ) {
            *report
                .categorical_values
                .entry(node.name.clone())
                .or_default()
                .entry(value)
                .or_default() += 1;
        } else if value.eq_ignore_ascii_case("true") {
            *report.true_features.entry(node.name.clone()).or_default() += 1;
        }
    }
    for child in &node.children {
        audit_particle_node(child, report);
    }
}

fn count_flag(report: &mut Report, name: &'static str, flags: u32, mask: u32) {
    if flags & mask != 0 {
        *report.material_flags.entry(name).or_default() += 1;
    }
}

impl Report {
    fn record_attachment(&mut self, attach_type: &str, name: &str) {
        let attach_type = attach_type.to_ascii_lowercase();
        *self
            .attachment_types
            .entry(attach_type.clone())
            .or_default() += 1;
        let samples = self.attachment_samples.entry(attach_type).or_default();
        if samples.len() < 16 && !samples.iter().any(|sample| sample == name) {
            samples.push(name.to_owned());
        }
    }
}

#[derive(Debug, Default)]
struct Report {
    model_refs: usize,
    models_parsed: usize,
    models_missing: usize,
    models_invalid: usize,
    sections: usize,
    materials: usize,
    legacy_materials: usize,
    hogan_materials: usize,
    blend_types: BTreeMap<u8, usize>,
    material_flags: BTreeMap<&'static str, usize>,
    map_slots: BTreeMap<usize, usize>,
    multi_map_slots: BTreeMap<usize, usize>,
    terrain_conform_materials: Vec<String>,
    local_reflection_materials: Vec<String>,
    uncommon_map_materials: Vec<String>,
    attachment_types: BTreeMap<String, usize>,
    component_attachment_types: BTreeMap<String, usize>,
    animation_attachment_types: BTreeMap<String, usize>,
    attachment_samples: BTreeMap<String, Vec<String>>,
    point_types: BTreeMap<String, usize>,
}

#[derive(Debug, Default)]
struct ParticleReport {
    files: usize,
    parsed: usize,
    missing: usize,
    invalid: usize,
    emitters: usize,
    renderer_adapted: usize,
    renderer_invalid: usize,
    renderer_emitters: usize,
    renderer_active_emitters: usize,
    renderer_render_emitters: usize,
    renderer_nested_emitters: usize,
    renderer_particle_budget: u64,
    renderer_errors: Vec<String>,
    categorical_values: BTreeMap<String, BTreeMap<String, usize>>,
    true_features: BTreeMap<String, usize>,
}

#[derive(Debug, Default)]
struct TerrainEffectReport {
    files: usize,
    adapted: usize,
    surfaces: usize,
    surface_types: BTreeMap<String, usize>,
    actions: BTreeMap<&'static str, usize>,
    action_samples: BTreeMap<&'static str, Vec<String>>,
    errors: Vec<String>,
}
