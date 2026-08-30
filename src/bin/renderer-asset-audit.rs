//! Read-only inventory of shipped HW1 visual and UGX renderer features.

use std::collections::{BTreeMap, BTreeSet};

use glam::{FloatExt, Mat4};
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

    if print_particle_xml_if_requested(&arguments, &mut source)? {
        return Ok(());
    }
    if print_asset_search_if_requested(&arguments, &source) {
        return Ok(());
    }

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
    let particle_definitions_only = arguments
        .iter()
        .any(|argument| argument == "--particle-definitions-only");
    if particle_definitions_only || arguments.iter().any(|argument| argument == "--particles") {
        let particle_report = audit_particles(
            &mut source,
            !particle_definitions_only,
            !particle_definitions_only,
        );
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

fn print_asset_search_if_requested(arguments: &[String], source: &Source) -> bool {
    let patterns = arguments
        .iter()
        .filter_map(|argument| argument.strip_prefix("--asset-search="))
        .collect::<Vec<_>>();
    if patterns.is_empty() {
        return false;
    }
    let archives = source.files_per_archive();
    for pattern in patterns {
        let needle = pattern.replace('/', "\\").to_ascii_lowercase();
        println!("asset matches for {pattern}:");
        for (archive, path) in archives
            .iter()
            .flat_map(|(archive, paths)| paths.iter().map(move |path| (*archive, *path)))
            .filter(|(_, path)| path.contains(&needle))
        {
            println!("  {archive}: {path}");
        }
    }
    true
}

fn print_particle_xml_if_requested(
    arguments: &[String],
    source: &mut Source,
) -> anyhow::Result<bool> {
    let Some(path) = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--particle-xml="))
    else {
        return Ok(false);
    };
    let bytes = source
        .resolve_with_fallback(path, &[".pfx.xmb", ".xmb"])
        .ok_or_else(|| anyhow::anyhow!("particle effect not found: {path}"))?;
    let document = pipeline::xmb::Reader::read(&bytes)?;
    println!("{}", document.to_xml());
    Ok(true)
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
            let world_position = render::ugx::scenario_object_position_to_world(position);
            let terrain_position = terrain_position(&raw, world_position);
            println!(
                "  id={} proto={:?} direct={direct:?} transposed={transposed:?} terrain_position={terrain_position:?}",
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
    println!("  player-start height deltas (direct, transposed):");
    for start in scenario.positions() {
        let [x, y, z] = start.position_vec3();
        let direct = height_delta(&raw, x, z, y);
        let transposed = height_delta(&raw, z, x, y);
        println!("    number={}: {direct:?}, {transposed:?}", start.number);
    }
    print_axis_height_oracle(scenario.objects(), &raw);
    print_proto_height_oracle(scenario.objects(), &raw);
}

fn print_proto_height_oracle(
    objects: &[pipeline::hw1::scenario::ScenarioObject],
    raw: &pipeline::xtd::RawTerrainData,
) {
    let mut errors = BTreeMap::<String, (f32, f32, u32)>::new();
    for object in objects {
        let [x, y, z] = object.position_vec3();
        let (Some(direct), Some(transposed)) =
            (height_delta(raw, x, z, y), height_delta(raw, z, x, y))
        else {
            continue;
        };
        let proto_name = object.proto_name.trim();
        let key = if proto_name.is_empty() {
            format!("<{}>", object.editor_name)
        } else {
            proto_name.to_ascii_lowercase()
        };
        let entry = errors.entry(key).or_default();
        entry.0 += direct.abs();
        entry.1 += transposed.abs();
        entry.2 += 1;
    }

    println!("  per-prototype mean absolute height deltas (direct, transposed):");
    for (proto_name, (direct, transposed, count)) in errors {
        let count = count.to_f32().unwrap_or(1.0);
        println!(
            "    {proto_name}: {:.3}, {:.3} ({count:.0})",
            direct / count,
            transposed / count,
        );
    }
}

fn print_axis_height_oracle(
    objects: &[pipeline::hw1::scenario::ScenarioObject],
    raw: &pipeline::xtd::RawTerrainData,
) {
    type AxisTransform = fn(f32, f32, f32) -> [f32; 2];
    let extent = raw
        .num_verts_per_axis
        .saturating_sub(1)
        .to_f32()
        .unwrap_or_default()
        * raw.tile_scale;
    let transforms: [(&str, AxisTransform); 8] = [
        ("x,z", |x: f32, z: f32, _extent: f32| [x, z]),
        ("z,x", |x: f32, z: f32, _extent: f32| [z, x]),
        ("-x,z", |x: f32, z: f32, extent: f32| [extent - x, z]),
        ("x,-z", |x: f32, z: f32, extent: f32| [x, extent - z]),
        ("-x,-z", |x: f32, z: f32, extent: f32| {
            [extent - x, extent - z]
        }),
        ("-z,-x", |x: f32, z: f32, extent: f32| {
            [extent - z, extent - x]
        }),
        ("-z,x", |x: f32, z: f32, extent: f32| [extent - z, x]),
        ("z,-x", |x: f32, z: f32, extent: f32| [z, extent - x]),
    ];
    println!("  axis transform mean absolute height deltas:");
    for (name, transform) in transforms {
        let mut error = 0.0_f32;
        let mut count = 0_u32;
        for object in objects {
            let [x, y, z] = object.position_vec3();
            let [world_x, world_z] = transform(x, z, extent);
            if let Some(delta) = height_delta(raw, world_x, world_z, y) {
                error += delta.abs();
                count += 1;
            }
        }
        if let Some(count) = count.to_f32().filter(|count| *count > 0.0) {
            println!("    {name}: {:.3}", error / count);
        }
    }
}

fn terrain_position(
    raw: &pipeline::xtd::RawTerrainData,
    grid_position: [f32; 3],
) -> Option<[f32; 3]> {
    let dimension = raw.num_verts_per_axis;
    let last = dimension.checked_sub(1)?.to_f32()?;
    let grid_x = grid_position[0].round().clamp(0.0, last).to_u32()?;
    let grid_z = grid_position[2].round().clamp(0.0, last).to_u32()?;
    let index = grid_z.checked_mul(dimension)?.checked_add(grid_x)?;
    let packed = *raw.packed_positions.get(index.to_usize()?)?;
    let displacement = pipeline::xtd::unpack_position(packed, &raw.mid, &raw.range);
    Some([
        grid_position[0] * raw.tile_scale + displacement[2],
        grid_position[1],
        grid_position[2] * raw.tile_scale + displacement[0],
    ])
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
    let max_players = world
        .scenario
        .as_ref()
        .map(|descriptor| descriptor.max_players);
    let position_axes = render::ugx::ScenarioPositionAxes::infer(scenario, max_players);
    println!("scenario max players: {max_players:?}");
    println!("scenario sim bounds: {:?}", scenario.sim_bounds());
    println!("scenario object axes: Transposed");
    println!("scenario player-start axes: {position_axes:?}");
    if let Some(raw) = world
        .terrain_data
        .as_ref()
        .and_then(|terrain| terrain.extract_raw_data().ok())
    {
        println!(
            "terrain coordinates: dimension={} tile_scale={} world_min={:?} world_max={:?} mid={:?} range={:?}",
            raw.num_verts_per_axis,
            raw.tile_scale,
            raw.world_min,
            raw.world_max,
            raw.mid,
            raw.range,
        );
    }
    println!("player start positions:");
    for position in scenario.positions() {
        let authored_position = position.position_vec3();
        let authored_forward = position.forward_vec3();
        println!(
            "  player={} number={} default_camera={} authored={authored_position:?} world={:?} authored_forward={authored_forward:?} world_forward={:?}",
            position.player,
            position.number,
            position.default_camera,
            position_axes.position_to_world(authored_position),
            position_axes.direction_to_world(authored_forward),
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
                    report.record_attachment(attachment);
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
                    report.record_attachment(attachment);
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

fn audit_particles(source: &mut Source, run_runtime: bool, load_materials: bool) -> ParticleReport {
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
    let mut material_cache = ParticleMaterialAuditCache::default();

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
                for (emitter_index, emitter) in effect.emitters.into_iter().enumerate() {
                    report.renderer_particle_budget += u64::from(emitter.max_particles);
                    report.renderer_editor_active_emitters += usize::from(emitter.editor_active);
                    report.renderer_light_volume_emitters +=
                        usize::from(emitter.material.light_volume);
                    report.renderer_nonwhite_corner_emitters += usize::from(
                        emitter
                            .material
                            .corner_colors
                            .iter()
                            .flatten()
                            .any(|channel| channel.to_bits() != 1.0_f32.to_bits()),
                    );
                    match &emitter.kind {
                        render::particle::ParticleEmitterKind::Render(_) => {
                            report.renderer_render_emitters += 1;
                            if load_materials {
                                audit_particle_material(
                                    source,
                                    &path,
                                    &emitter,
                                    &mut material_cache,
                                    &mut report,
                                );
                            }
                        }
                        render::particle::ParticleEmitterKind::NestedEffect(_) => {
                            report.renderer_nested_emitters += 1;
                        }
                    }
                    record_particle_prewarm(&path, &emitter, &mut report);
                    if run_runtime {
                        audit_particle_runtime(emitter, emitter_index, &mut report);
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

fn audit_particle_material(
    source: &mut Source,
    path: &str,
    emitter: &render::particle::ParticleEmitter,
    cache: &mut ParticleMaterialAuditCache,
    report: &mut ParticleReport,
) {
    let definitions = emitter
        .material
        .diffuse
        .iter()
        .chain(std::iter::once(&emitter.material.intensity));
    let mut resampled_layers = 0;
    let mut fallback_layers = 0;
    let mut unavailable_texture_sets = 0;
    for (definition_index, definition) in definitions.enumerate() {
        let mut dimensions = Vec::with_capacity(definition.stages.len());
        let mut first_error = None;
        for stage in &definition.stages {
            let key = particle_texture_audit_key(&stage.path);
            if !cache.textures.contains_key(&key) {
                let result = render::particle::ParticleImage::load(source, &stage.path)
                    .map(|image| [image.width, image.height])
                    .map_err(|error| error.to_string());
                cache.textures.insert(key.clone(), result);
            }
            match &cache.textures[&key] {
                Ok(size) => dimensions.push(Some(*size)),
                Err(error) => {
                    first_error.get_or_insert_with(|| error.clone());
                    dimensions.push(None);
                }
            }
        }
        let Some(target) = dimensions
            .iter()
            .flatten()
            .copied()
            .reduce(|[width, height], size| [width.max(size[0]), height.max(size[1])])
        else {
            if let Some(error) = first_error {
                if definition_index == 0 {
                    record_particle_material_error(path, emitter, &error, report);
                    return;
                }
                unavailable_texture_sets += 1;
            }
            continue;
        };
        fallback_layers += dimensions.iter().filter(|size| size.is_none()).count();
        resampled_layers += dimensions
            .iter()
            .flatten()
            .filter(|size| **size != target)
            .count();
    }
    if resampled_layers > 0 || fallback_layers > 0 || unavailable_texture_sets > 0 {
        match emitter.load_material(source) {
            Ok(material) => {
                let actual_resampled = material
                    .diffuse
                    .iter()
                    .chain(std::iter::once(&material.intensity))
                    .flatten()
                    .map(render::particle::ParticleTextureArray::resampled_layer_count)
                    .sum::<usize>();
                let actual_fallback = material
                    .diffuse
                    .iter()
                    .chain(std::iter::once(&material.intensity))
                    .flatten()
                    .map(render::particle::ParticleTextureArray::fallback_layer_count)
                    .sum::<usize>();
                if actual_resampled != resampled_layers
                    || actual_fallback != fallback_layers
                    || material.unavailable_texture_sets != unavailable_texture_sets
                {
                    record_particle_material_error(
                        path,
                        emitter,
                        &format!(
                            "expected {resampled_layers} resampled/{fallback_layers} fallback layers and {unavailable_texture_sets} unavailable sets, got {actual_resampled}/{actual_fallback}/{}",
                            material.unavailable_texture_sets
                        ),
                        report,
                    );
                    return;
                }
            }
            Err(error) => {
                record_particle_material_error(path, emitter, &error.to_string(), report);
                return;
            }
        }
    }
    report.renderer_materials_loaded += 1;
    report.renderer_resampled_texture_layers += resampled_layers;
    report.renderer_fallback_texture_layers += fallback_layers;
    report.renderer_unavailable_optional_texture_sets += unavailable_texture_sets;
}

fn record_particle_material_error(
    path: &str,
    emitter: &render::particle::ParticleEmitter,
    error: &str,
    report: &mut ParticleReport,
) {
    report.renderer_material_errors += 1;
    if report.renderer_errors.len() < 16 {
        report
            .renderer_errors
            .push(format!("{path} :: {} material: {error}", emitter.name));
    }
}

fn particle_texture_audit_key(path: &str) -> String {
    let normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\")
        .to_ascii_lowercase();
    let stem = normalized
        .strip_suffix(".tga")
        .or_else(|| normalized.strip_suffix(".ddx"))
        .unwrap_or(&normalized);
    if stem.starts_with("art\\") {
        stem.to_owned()
    } else {
        format!("art\\{stem}")
    }
}

fn record_particle_prewarm(
    path: &str,
    emitter: &render::particle::ParticleEmitter,
    report: &mut ParticleReport,
) {
    let initial_update = emitter.runtime.timing.initial_update;
    let maximum = initial_update.value * initial_update.variance.abs().mul_add(1.0, 1.0);
    if maximum > f32::EPSILON {
        report.renderer_prewarmed_emitters += 1;
        report.renderer_total_max_prewarm += maximum;
    }
    if maximum > report.renderer_max_prewarm {
        report.renderer_max_prewarm = maximum;
        report.renderer_max_prewarm_emitter = format!("{path} :: {}", emitter.name);
    }
}

fn audit_particle_runtime(
    emitter: render::particle::ParticleEmitter,
    emitter_index: usize,
    report: &mut ParticleReport,
) {
    let seed = u32::try_from(emitter_index).unwrap_or(u32::MAX);
    let mut runtime = render::particle::ParticleEmitterRuntime::new(
        emitter,
        seed.wrapping_mul(0x9e37_79b9),
        Mat4::IDENTITY,
    );
    runtime.update(0.2, Mat4::IDENTITY, Mat4::IDENTITY);
    let instances = runtime.instances(
        &render::particle::ParticleMaterial::default(),
        render::particle::ParticleRenderContext::default(),
    );
    report.renderer_runtime_emitters += 1;
    report.renderer_runtime_particles += runtime.live_particle_count();
    report.renderer_runtime_instances += instances.len();
    report.renderer_runtime_nested_events += runtime.take_nested_events().len();
    report.renderer_runtime_nonfinite_instances += instances
        .iter()
        .filter(|instance| !instance_is_finite(instance))
        .count();
}

fn instance_is_finite(instance: &render::particle::ParticleInstance) -> bool {
    instance
        .position
        .iter()
        .chain(instance.axis.iter())
        .chain(instance.up_axis.iter())
        .chain(instance.size.iter())
        .chain(instance.color.iter())
        .chain(instance.intensity.iter())
        .chain(instance.uv_rects.iter().flatten())
        .chain([
            &instance.rotation,
            &instance.half_length,
            &instance.soft_fade_scale,
        ])
        .all(|value| value.is_finite())
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
    fn record_attachment(&mut self, attachment: &pipeline::database::hw1::visual::Attachment) {
        let attach_type = attachment.attach_type.to_ascii_lowercase();
        *self
            .attachment_types
            .entry(attach_type.clone())
            .or_default() += 1;
        if attachment.sync_anims.unwrap_or(false) {
            *self
                .synchronized_attachment_types
                .entry(attach_type.clone())
                .or_default() += 1;
        }
        if attachment.disregard_orient.unwrap_or(false) {
            *self
                .disregard_orientation_attachment_types
                .entry(attach_type.clone())
                .or_default() += 1;
        }
        let samples = self.attachment_samples.entry(attach_type).or_default();
        if samples.len() < 16 && !samples.iter().any(|sample| sample == &attachment.name) {
            samples.push(attachment.name.clone());
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
    synchronized_attachment_types: BTreeMap<String, usize>,
    disregard_orientation_attachment_types: BTreeMap<String, usize>,
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
    renderer_editor_active_emitters: usize,
    renderer_light_volume_emitters: usize,
    renderer_nonwhite_corner_emitters: usize,
    renderer_render_emitters: usize,
    renderer_nested_emitters: usize,
    renderer_particle_budget: u64,
    renderer_materials_loaded: usize,
    renderer_material_errors: usize,
    renderer_resampled_texture_layers: usize,
    renderer_fallback_texture_layers: usize,
    renderer_unavailable_optional_texture_sets: usize,
    renderer_prewarmed_emitters: usize,
    renderer_total_max_prewarm: f32,
    renderer_max_prewarm: f32,
    renderer_max_prewarm_emitter: String,
    renderer_runtime_emitters: usize,
    renderer_runtime_particles: usize,
    renderer_runtime_instances: usize,
    renderer_runtime_nested_events: usize,
    renderer_runtime_nonfinite_instances: usize,
    renderer_errors: Vec<String>,
    categorical_values: BTreeMap<String, BTreeMap<String, usize>>,
    true_features: BTreeMap<String, usize>,
}

#[derive(Default)]
struct ParticleMaterialAuditCache {
    textures: BTreeMap<String, Result<[u32; 2], String>>,
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
