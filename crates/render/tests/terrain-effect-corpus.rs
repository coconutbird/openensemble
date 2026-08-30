//! Installed-asset validation for terrain-effect (`.tfx`) routing tables.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use render::terrain_effect::{
    ImpactEffectCatalog, TerrainEffect, TerrainEffectAction, TerrainEffectSize,
    TerrainImpactAssets, TerrainSurfaceCatalog,
};

#[test]
#[ignore = "requires OPENENSEMBLE_GAME_DIR pointing at Halo Wars DE"]
fn impact_route_asset_paths_are_enumerated() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point at Halo Wars DE");
    let mut source = pipeline::hw1::loader::load_game_dir(&game_dir);
    let assets = TerrainImpactAssets::load(&mut source).expect("terrain impact assets");
    println!("impact_particle_paths {:?}", assets.particle_paths());
    println!("impact_light_paths {:?}", assets.light_paths());
    println!("impact_decal_paths {:?}", assets.decal_paths());
    println!("impact_visual_names {:?}", assets.visual_names());
    for path in assets.light_paths() {
        let canonical = format!("art\\{}.lgt", path.trim_start_matches(['\\', '/']));
        println!(
            "impact_light_load {canonical}: {:?}",
            render::light_effect::LightEffect::load(&mut source, &canonical)
                .map(|effect| effect.frames.len())
        );
    }
    assert!(!assets.particle_paths().is_empty());
    assert!(!assets.light_paths().is_empty());
}

#[test]
#[ignore = "requires OPENENSEMBLE_GAME_DIR pointing at Halo Wars DE"]
fn all_shipped_terrain_effects_decode_with_finite_parameters() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point at Halo Wars DE");
    let mut source = pipeline::hw1::loader::load_game_dir(&game_dir);
    let catalog = TerrainSurfaceCatalog::load(&mut source).expect("terrain surface catalog");
    assert_eq!(catalog.len(), 21);
    assert_eq!(catalog.name(0), Some("UNDEFINED"));
    assert_eq!(catalog.name(20), Some("HunterFlesh"));

    let paths = installed_tfx_paths(&source);
    assert_eq!(paths.len(), 94, "unexpected installed TFX corpus size");

    let impacts = audit_impact_effects(&mut source);
    audit_preloaded_impact_assets(&mut source, &impacts);
    let corpus = audit_terrain_effects(&mut source, &paths);

    assert_eq!(corpus.items, 399);
    println!("terrain_effect_files {}", paths.len());
    println!("impact_effect_prototypes {}", impacts.prototype_count);
    println!("impact_effect_routes {}", impacts.routes.len());
    println!("terrain_effect_items {}", corpus.items);
    println!("terrain_effect_sizes {:?}", corpus.sizes);
    println!("terrain_effect_actions {:?}", corpus.actions);
}

type InstalledSource = pipeline::source::AssetSource<pipeline::source::StdFileProvider>;

fn installed_tfx_paths(source: &InstalledSource) -> BTreeSet<String> {
    source
        .files_per_archive()
        .into_iter()
        .flat_map(|(_, paths)| paths)
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("tfx"))
        })
        .map(str::to_owned)
        .collect()
}

struct ImpactAudit {
    prototype_count: usize,
    routes: BTreeSet<String>,
    assets: TerrainImpactAssets,
}

fn audit_impact_effects(source: &mut InstalledSource) -> ImpactAudit {
    let impact_effects = ImpactEffectCatalog::load(source).expect("impact-effect catalog");
    let mut impact_names = BTreeSet::new();
    let mut impact_routes = BTreeSet::new();
    for definition in impact_effects.definitions() {
        assert!(
            impact_names.insert(definition.name.to_ascii_lowercase()),
            "duplicate impact-effect name {}",
            definition.name
        );
        assert_finite_nonnegative(
            &definition.name,
            "impact lifespan",
            definition.lifespan_seconds,
        );
        assert!(
            definition.meter_limit > 0,
            "{}: zero meter limit",
            definition.name
        );
        assert_near(definition.bounding_radius, 10.0);
        TerrainEffect::load(source, &definition.terrain_effect_path).unwrap_or_else(|error| {
            panic!(
                "impact effect {} routes {}: {error}",
                definition.name, definition.terrain_effect_path
            )
        });
        impact_routes.insert(definition.terrain_effect_path.to_ascii_lowercase());
    }

    let impact_assets = TerrainImpactAssets::load(source).expect("preloaded terrain-impact assets");
    assert!(
        impact_assets.issues().is_empty(),
        "{:?}",
        impact_assets.issues()
    );
    assert_eq!(impact_assets.loaded_effect_count(), impact_routes.len() + 1);
    assert!(!impact_assets.particle_paths().is_empty());
    assert!(!impact_assets.light_paths().is_empty());

    ImpactAudit {
        prototype_count: impact_effects.len(),
        routes: impact_routes,
        assets: impact_assets,
    }
}

fn audit_preloaded_impact_assets(source: &mut InstalledSource, impacts: &ImpactAudit) {
    let empty_world = sim::World::new();
    let scene = render::ugx::UnitScene::load_world(
        source,
        &empty_world,
        &std::collections::HashMap::new(),
        &[],
    );
    assert_eq!(
        scene.terrain_impact_effect_count(),
        impacts.routes.len() + 1
    );
    assert_eq!(scene.terrain_impact_effect_issue_count(), 0);
    println!("impact_particle_graphs {}", scene.particle_effect_count());
    println!(
        "impact_particle_issues {}",
        scene.particle_effect_issue_count()
    );
    println!(
        "impact_particle_issue_list {:?}",
        scene.particle_effect_issues()
    );
    println!("impact_light_graphs {}", scene.light_effect_count());
    println!("impact_light_issues {}", scene.light_effect_issue_count());
    println!("impact_light_issue_list {:?}", scene.light_effect_issues());
    println!(
        "impact_decal_materials {}",
        scene.impact_decal_material_count()
    );
    println!(
        "impact_decal_issues {}",
        scene.impact_decal_material_issue_count()
    );
    println!("impact_visual_graphs {}", scene.impact_visual_count());
    println!("impact_visual_issues {}", scene.impact_visual_issue_count());
    println!(
        "impact_visual_issue_list {:?}",
        scene.impact_visual_issues()
    );
    assert!(scene.particle_effect_count() > 0);
    assert!(scene.light_effect_count() > 0);
    assert_eq!(scene.impact_decal_material_count(), 0);
    assert_eq!(
        scene.impact_visual_count() + scene.impact_visual_issue_count(),
        impacts.assets.visual_names().len()
    );
    assert!(
        scene
            .impact_visual_issues()
            .iter()
            .all(|issue| issue.contains("atomicblast_01")),
        "unexpected impact visual failures: {:?}",
        scene.impact_visual_issues()
    );
}

struct TerrainCorpus {
    items: usize,
    sizes: BTreeMap<&'static str, usize>,
    actions: BTreeMap<&'static str, usize>,
}

fn audit_terrain_effects(source: &mut InstalledSource, paths: &BTreeSet<String>) -> TerrainCorpus {
    let mut corpus = TerrainCorpus {
        items: 0,
        sizes: BTreeMap::new(),
        actions: BTreeMap::new(),
    };
    for path in paths {
        let effect =
            TerrainEffect::load(source, path).unwrap_or_else(|error| panic!("{path}: {error}"));
        for item in &effect.surfaces {
            corpus.items += 1;
            assert!(item.weight > 0, "{path}: zero normalized weight");
            *corpus.sizes.entry(size_name(item.size)).or_default() += 1;
            for action in &item.actions {
                *corpus
                    .actions
                    .entry(audit_action(path, action))
                    .or_default() += 1;
            }
        }
    }
    corpus
}

fn audit_action(path: &str, action: &TerrainEffectAction) -> &'static str {
    match action {
        TerrainEffectAction::Particle(_) => "particle",
        TerrainEffectAction::ImpactDecal(decal) => {
            assert_finite_nonnegative(path, "decal sizeX", decal.size_x);
            assert_finite_nonnegative(path, "decal sizeZ", decal.size_z);
            assert_finite_nonnegative(path, "decal opaque time", decal.fully_opaque_seconds);
            assert_finite_nonnegative(path, "decal fade time", decal.fade_out_seconds);
            "impact_decal"
        }
        TerrainEffectAction::Trail(trail) => {
            assert_finite_nonnegative(path, "trail node distance", trail.minimum_node_distance);
            assert_finite_nonnegative(path, "trail width", trail.width);
            assert!(trail.full_alpha_frames >= 0, "{path}: negative hold frames");
            assert!(trail.fade_out_frames >= 0, "{path}: negative fade frames");
            assert_eq!(trail.max_nodes, 30, "{path}: retail ribbon capacity");
            "trail"
        }
        TerrainEffectAction::Visual(_) => "visual",
        TerrainEffectAction::Light(light) => {
            assert_finite_nonnegative(path, "light lifespan", light.lifespan_seconds);
            "light"
        }
        TerrainEffectAction::Sound(_) => "sound",
    }
}

fn size_name(size: TerrainEffectSize) -> &'static str {
    match size {
        TerrainEffectSize::Small => "small",
        TerrainEffectSize::Medium => "medium",
        TerrainEffectSize::Large => "large",
        TerrainEffectSize::Generic => "generic",
    }
}

fn assert_finite_nonnegative(path: &str, field: &str, value: f32) {
    assert!(
        value.is_finite() && value >= 0.0,
        "{path}: {field} is {value}"
    );
}

fn assert_near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON);
}
